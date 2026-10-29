//! Lossless WebP, still and animated, with libwebp (the reference encoder,
//! in C, through `libwebp-sys`). Its palette transform and LZ77 make a code
//! frame about three times smaller than a pure Rust encoder does.
//!
//! This module is the safe face of the encoder: it checks every size and
//! length before libwebp sees them, and nothing of libwebp (pointer, handle,
//! type) leaves it. All the `unsafe` code of Tiza is in [`ffi`], which is
//! private.
//!
//! [`to_webp`] and [`to_webp_anim`] export Tiza's frames: the pixels of the
//! PNG path, and for an animation the frame times of the GIF.

#[allow(unsafe_code)]
mod ffi;
mod rgba;

pub use rgba::Rgba;
use rgba::check_size;

use resvg::tiny_skia::Pixmap;
use tiza_core::canvas::Canvas;
use tiza_core::theme::Theme;

use crate::Error;
use crate::gif::frame_delay_ms;
use crate::png::{PngOptions, rasterize};
use crate::svg::{SvgOptions, to_svg};

/// Largest side of a WebP image, in pixels (`WEBP_MAX_DIMENSION`).
pub const MAX_SIDE: u32 = 16_383;
/// Longest frame of an animation, in milliseconds (a 24-bit field).
pub const MAX_FRAME_MS: u32 = (1 << 24) - 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WebpError {
    #[error("a {0}×{1} px image is not a WebP size (1 to {MAX_SIDE} px per side)")]
    Size(u32, u32),
    #[error("{got} bytes of pixels for a {width}×{height} RGBA image, not {expected}")]
    Length {
        width: u32,
        height: u32,
        got: usize,
        expected: usize,
    },
    #[error("a {width}×{height} px frame in a {expected_width}×{expected_height} px animation")]
    FrameSize {
        width: u32,
        height: u32,
        expected_width: u32,
        expected_height: u32,
    },
    #[error("a frame of {0} ms (a frame lasts 1 to {MAX_FRAME_MS} ms)")]
    Duration(u32),
    #[error("compression level {0} (use 0 to 9)")]
    Level(u8),
    #[error("an animation with no frames")]
    NoFrames,
    #[error("the encoder stopped after an earlier error")]
    Failed,
    /// Its text says that the encoder failed: a caller puts its own
    /// context before it, as the CLI's `could not encode the WebP`.
    #[error(transparent)]
    Encoder(EncoderError),
}

/// Why the encoder failed, from the error code of libwebp.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EncoderError {
    #[error("the encoder ran out of memory")]
    OutOfMemory,
    #[error("the encoder refused its configuration")]
    InvalidConfiguration,
    #[error("the encoder refused the size of the image")]
    BadDimension,
    #[error("the encoder could not write its output")]
    BadWrite,
    #[error("the image is too large for a WebP file")]
    FileTooBig,
    #[error("the encoder wrote nothing")]
    NoOutput,
    #[error("the encoder library has another version")]
    VersionMismatch,
    /// What the animation encoder said.
    #[error("the animation encoder failed: {0}")]
    Animation(String),
    /// A code of libwebp that has no variant here.
    #[error("the encoder failed with error code {0}")]
    Code(u32),
    /// The encoder failed and gave no reason.
    #[error("the encoder failed and gave no reason")]
    Unknown,
}

/// Lossless compression level, 0 (fastest) to 9 (smallest), as in
/// `cwebp -lossless -z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level(u8);

impl Level {
    /// Level 6: the documentation frames are 2 % larger than at level 9
    /// and encode about 90 times faster.
    pub const DEFAULT: Level = Level(6);

    pub fn new(level: u8) -> Result<Self, WebpError> {
        if level <= 9 {
            Ok(Self(level))
        } else {
            Err(WebpError::Level(level))
        }
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One lossless WebP image. The same pixels give the same bytes.
pub fn encode_still(img: Rgba<'_>, level: Level) -> Result<Vec<u8>, WebpError> {
    ffi::encode_still(&img, level.0)
}

/// An animated lossless WebP, frame by frame. Each frame is copied into
/// the encoder, which writes only the part that changed. After an error
/// from libwebp, every later call returns [`WebpError::Failed`].
///
/// Neither `Send` nor `Sync`, on purpose: no caller needs it, and the
/// encoder of libwebp then stays on the thread that made it.
pub struct AnimEncoder {
    inner: ffi::Anim,
    width: u32,
    height: u32,
    /// When the next frame starts, in milliseconds from the first. It wraps
    /// past `i32::MAX`: libwebp only takes the difference of two
    /// timestamps, and a frame lasts less than 2^24 ms.
    next_ms: i32,
    frames: usize,
    failed: bool,
    one_thread: std::marker::PhantomData<*const ()>,
}

impl AnimEncoder {
    /// An animation that plays `plays` times, or forever with 0 (as
    /// `--loop` of `tiza render`).
    pub fn new(width: u32, height: u32, level: Level, plays: u16) -> Result<Self, WebpError> {
        check_size(width, height)?;
        Ok(Self {
            inner: ffi::Anim::new(width, height, level.0, plays)?,
            width,
            height,
            next_ms: 0,
            frames: 0,
            failed: false,
            one_thread: std::marker::PhantomData,
        })
    }

    /// Adds a frame of the animation's size that shows for `duration_ms`.
    /// A frame equal to the one before it makes that one last longer.
    pub fn add_frame(&mut self, img: Rgba<'_>, duration_ms: u32) -> Result<(), WebpError> {
        if self.failed {
            return Err(WebpError::Failed);
        }
        if (img.width(), img.height()) != (self.width, self.height) {
            return Err(WebpError::FrameSize {
                width: img.width(),
                height: img.height(),
                expected_width: self.width,
                expected_height: self.height,
            });
        }
        if !(1..=MAX_FRAME_MS).contains(&duration_ms) {
            return Err(WebpError::Duration(duration_ms));
        }
        // Cannot fail: MAX_FRAME_MS is less than i32::MAX.
        let duration = i32::try_from(duration_ms).map_err(|_| WebpError::Duration(duration_ms))?;
        let end = self.next_ms.wrapping_add(duration);
        if let Err(e) = self.inner.add(&img, self.next_ms) {
            self.failed = true;
            return Err(e);
        }
        self.next_ms = end;
        self.frames += 1;
        Ok(())
    }

    /// The bytes of the animation. One frame gives a still image.
    pub fn finish(self) -> Result<Vec<u8>, WebpError> {
        if self.failed {
            return Err(WebpError::Failed);
        }
        if self.frames == 0 {
            return Err(WebpError::NoFrames);
        }
        self.inner.finish(self.next_ms)
    }
}

/// The straight RGBA of a rasterized frame, the pixels that `to_png`
/// writes: the buffer of the pixmap, changed in place (no copy of the
/// frame).
fn straight_rgba(mut pm: Pixmap) -> Vec<u8> {
    for i in 0..pm.pixels().len() {
        let c = pm.pixels()[i].demultiply();
        pm.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    pm.take()
}

/// A lossless WebP still and its size in pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebpStill {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Rasterizes an SVG made by [`crate::to_svg`] into a lossless WebP: the
/// pixels of [`crate::to_png`] with the same options.
pub fn to_webp(svg: &str, o: &PngOptions) -> Result<WebpStill, Error> {
    let pm = rasterize(svg, o)?;
    let (width, height) = (pm.width(), pm.height());
    let img = straight_rgba(pm);
    let bytes = encode_still(Rgba::new(&img, width, height)?, Level::DEFAULT)?;
    Ok(WebpStill {
        bytes,
        width,
        height,
    })
}

#[derive(Debug, Clone)]
pub struct WebpAnimOptions {
    pub svg: SvgOptions,
    pub png: PngOptions,
    /// How many times the animation plays; 0 plays it forever.
    pub plays: u16,
}

/// What [`to_webp_anim`] wrote, for the CLI's summary line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebpReport {
    pub frames: usize,
    pub width: u32,
    pub height: u32,
    /// Milliseconds the animation lasts (one loop).
    pub duration_ms: u64,
}

/// Encodes `frames` (canvas, delay in ms), as [`crate::to_gif`] takes them,
/// into an animated lossless WebP. Each frame has the pixels of the PNG
/// path and lasts as long as in the GIF ([`frame_delay_ms`]).
pub fn to_webp_anim(
    frames: &[(Canvas, u64)],
    theme: &Theme,
    o: &WebpAnimOptions,
) -> Result<(Vec<u8>, WebpReport), Error> {
    to_webp_anim_from(frames, theme, o)
}

/// [`to_webp_anim`] from frames given one at a time ([`crate::Frames`]):
/// the frames of a [`tiza_core::anim::Take`] are never all in memory.
pub fn to_webp_anim_from<F: crate::Frames + ?Sized>(
    frames: &F,
    theme: &Theme,
    o: &WebpAnimOptions,
) -> Result<(Vec<u8>, WebpReport), Error> {
    let mut enc = None;
    let mut duration_ms = 0u64;
    let mut count = 0usize;
    frames.each(&mut |canvas, delay| {
        count += 1;
        // Over the limits: an error before the SVG, not after it.
        crate::limits::image_size(canvas, &o.svg, &o.png)?;
        let pm = rasterize(&to_svg(canvas, theme, &o.svg), &o.png)?;
        let (w, h) = (pm.width(), pm.height());
        let enc = match enc {
            Some(ref mut e) => e,
            None => enc.insert(AnimEncoder::new(w, h, Level::DEFAULT, o.plays)?),
        };
        let ms = frame_delay_ms(delay);
        duration_ms = duration_ms.saturating_add(ms);
        // At most two maximum holds (frame_delay_ms), far under MAX_FRAME_MS.
        let ms = u32::try_from(ms).map_err(|_| WebpError::Duration(u32::MAX))?;
        enc.add_frame(Rgba::new(&straight_rgba(pm), w, h)?, ms)?;
        Ok(())
    })?;
    let enc = enc.ok_or(WebpError::NoFrames)?;
    let (width, height) = (enc.width, enc.height);
    Ok((
        enc.finish()?,
        WebpReport {
            frames: count,
            width,
            height,
            duration_ms,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiza_core::canvas::{CellStyle, Rgb};

    /// A canvas with code and a box.
    fn canvas(text: &str) -> Canvas {
        let mut c = Canvas::new(24, 3);
        c.put_str(0, 0, text, CellStyle::new().fg(Rgb(255, 255, 255)));
        c.put_str(
            0,
            1,
            "╭─ ★ ✎ ─╮ é 中",
            CellStyle::new().fg(Rgb(255, 128, 0)),
        );
        c
    }

    fn svg_of(c: &Canvas) -> String {
        let o = SvgOptions {
            embed_fonts: false,
            ..Default::default()
        };
        to_svg(c, &Theme::dark(), &o)
    }

    /// A real frame through the PNG path, as straight RGBA.
    fn frame(text: &str) -> (Vec<u8>, u32, u32) {
        let pm = rasterize(&svg_of(&canvas(text)), &PngOptions::default()).unwrap();
        let (w, h) = (pm.width(), pm.height());
        (straight_rgba(pm), w, h)
    }

    /// A small image with every alpha, and colors under alpha 0.
    fn alpha_ramp() -> (Vec<u8>, u32, u32) {
        let (w, h) = (17u32, 9u32);
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend([(x * 15) as u8, (y * 28) as u8, 200, ((x + y * w) * 2) as u8]);
            }
        }
        (v, w, h)
    }

    fn decode(bytes: &[u8]) -> image_webp::WebPDecoder<std::io::Cursor<&[u8]>> {
        image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap()
    }

    fn decode_still(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
        let mut d = decode(bytes);
        assert!(!d.is_animated());
        let (w, h) = d.dimensions();
        let mut out = vec![0; d.output_buffer_size().unwrap()];
        d.read_image(&mut out).unwrap();
        (rgba(out, d.has_alpha()), w, h)
    }

    /// The decoder gives RGB when the image has no alpha.
    fn rgba(px: Vec<u8>, has_alpha: bool) -> Vec<u8> {
        if has_alpha {
            px
        } else {
            px.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect()
        }
    }

    /// The frames of an animation and their times, put together here as the
    /// decoder of libwebp does it: a frame without blending replaces its
    /// rectangle; with blending, an opaque pixel replaces the one below and
    /// a transparent pixel keeps it.
    ///
    /// Not image-webp's `read_frame`: version 0.2.4 has a bug. It blends the
    /// opaque pixels too, and its rounding takes 1 off some of them, so a
    /// lossless frame would look lossy. libwebp copies them.
    fn frames(webp: &[u8]) -> Vec<(Vec<u8>, u32)> {
        let u24 = |b: &[u8]| u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16;
        let (cw, ch) = decode(webp).dimensions();
        let mut canvas = vec![0u8; (cw * ch * 4) as usize];
        let mut dispose = None;
        let mut out = Vec::new();
        let mut pos = 12;
        while pos + 8 <= webp.len() {
            let size = u32::from_le_bytes(webp[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let (id, body) = (&webp[pos..pos + 4], &webp[pos + 8..pos + 8 + size]);
            pos += 8 + size + (size & 1);
            if id != b"ANMF" {
                continue;
            }
            let (x, y) = (u24(&body[0..]) * 2, u24(&body[3..]) * 2);
            let (w, h) = (u24(&body[6..]) + 1, u24(&body[9..]) + 1);
            let blend = body[15] & 2 == 0;
            // Disposal of the frame before: its rectangle becomes transparent.
            if let Some((dx, dy, dw, dh)) = dispose.take() {
                for r in dy..dy + dh {
                    let i = ((r * cw + dx) * 4) as usize;
                    canvas[i..i + (dw * 4) as usize].fill(0);
                }
            }
            let data = &body[16..];
            let mut still = b"RIFF".to_vec();
            still.extend((4 + data.len() as u32).to_le_bytes());
            still.extend(b"WEBP");
            still.extend(data);
            let (px, fw, fh) = decode_still(&still);
            assert_eq!((fw, fh), (w, h));
            for r in 0..h {
                for c in 0..w {
                    let s = &px[((r * w + c) * 4) as usize..][..4];
                    let i = (((y + r) * cw + x + c) * 4) as usize;
                    match (blend, s[3]) {
                        (true, 0) => {}
                        (false, _) | (true, 255) => canvas[i..i + 4].copy_from_slice(s),
                        (true, a) => panic!("a blended pixel with alpha {a}"),
                    }
                }
            }
            out.push((canvas.clone(), u24(&body[12..])));
            if body[15] & 1 == 1 {
                dispose = Some((x, y, w, h));
            }
        }
        out
    }

    #[test]
    fn to_webp_has_the_pixels_of_to_png() {
        let svg = svg_of(&canvas("fn main() { println!(); }"));
        let o = PngOptions::default();
        let png = Pixmap::decode_png(&crate::to_png(&svg, &o).unwrap()).unwrap();
        let (w, h) = (png.width(), png.height());
        let still = to_webp(&svg, &o).unwrap();
        assert_eq!((still.width, still.height), (w, h));
        assert_eq!(decode_still(&still.bytes), (straight_rgba(png), w, h));
    }

    #[test]
    fn to_webp_anim_has_the_frames_and_times_of_the_gif() {
        let texts = ["let a;", "let a = 1;", "let a = 12;"];
        let delays = [25, 400, 1234];
        let frames: Vec<(Canvas, u64)> = texts.iter().map(|t| canvas(t)).zip(delays).collect();
        let o = WebpAnimOptions {
            svg: SvgOptions {
                embed_fonts: false,
                ..Default::default()
            },
            png: PngOptions::default(),
            plays: 2,
        };
        let (webp, report) = to_webp_anim(&frames, &Theme::dark(), &o).unwrap();
        let got = self::frames(&webp);
        let times: Vec<u32> = got.iter().map(|f| f.1).collect();
        assert_eq!(times, [30, 400, 1240], "frame_delay_ms, as in the GIF");
        for ((px, ..), t) in got.iter().zip(texts) {
            assert!(*px == frame(t).0, "{t}");
        }
        let (_, w, h) = frame(texts[0]);
        assert_eq!(
            report,
            WebpReport {
                frames: 3,
                width: w,
                height: h,
                duration_ms: 1670
            }
        );
        assert_eq!(
            decode(&webp).loop_count(),
            image_webp::LoopCount::Times(2.try_into().unwrap())
        );
        assert!(matches!(
            to_webp_anim(&[], &Theme::dark(), &o),
            Err(Error::Webp(WebpError::NoFrames))
        ));
    }

    #[test]
    fn still_keeps_every_pixel() {
        for (px, w, h) in [frame("fn main() { let x = 1; }"), alpha_ramp()] {
            let webp = encode_still(Rgba::new(&px, w, h).unwrap(), Level::DEFAULT).unwrap();
            assert_eq!(&webp[..4], b"RIFF");
            assert_eq!(&webp[8..12], b"WEBP");
            assert_eq!(decode_still(&webp), (px, w, h));
        }
    }

    #[test]
    fn pixels_at_any_address_encode() {
        // libwebp reads the pixels as `uint32_t`: a slice that does not start
        // on 4 bytes is copied first.
        let (px, w, h) = alpha_ramp();
        let mut buf = vec![0u8];
        buf.extend(&px);
        let shifted = &buf[1..];
        assert!(!shifted.as_ptr().cast::<u32>().is_aligned());
        let webp = encode_still(Rgba::new(shifted, w, h).unwrap(), Level::DEFAULT).unwrap();
        assert_eq!(decode_still(&webp), (px, w, h));
    }

    /// libwebp keeps no pointer to memory of Rust after a call: each frame
    /// buffer goes away right after `add_frame`, and new allocations then
    /// write junk where it was. Under ASan, a read of the old buffer is an
    /// error; here, a wrong frame fails the compare.
    #[test]
    fn frame_buffers_can_go_after_add_frame() {
        let texts = ["let a = 1;", "let b = 2;", "let c = 3;"];
        let (_, w, h) = frame(texts[0]);
        let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
        for t in texts {
            let px = frame(t).0;
            a.add_frame(Rgba::new(&px, w, h).unwrap(), 100).unwrap();
            drop(px);
            let junk: Vec<Vec<u8>> = (0..4)
                .map(|i| vec![0xA5 ^ i; (w * h * 4) as usize])
                .collect();
            drop(junk);
        }
        let webp = a.finish().unwrap();
        let got = self::frames(&webp);
        assert_eq!(got.len(), 3);
        for ((px, _), t) in got.iter().zip(texts) {
            assert!(*px == frame(t).0, "{t}");
        }
    }

    /// An encoder dropped halfway, without `finish`, after frames and after
    /// refused frames, frees all that it has and touches nothing after. The
    /// refusals are of the Rust side (they never reach libwebp). A plain
    /// `cargo test` has no LeakSanitizer: the ASan run of these tests and
    /// the fuzzer check the leaks.
    #[test]
    fn an_encoder_dropped_halfway_frees_everything() {
        let (px, w, h) = alpha_ramp();
        let img = Rgba::new(&px, w, h).unwrap();
        let small = vec![0u8; 16];
        for frames in 0..3 {
            let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
            for _ in 0..frames {
                a.add_frame(img, 50).unwrap();
            }
            assert!(a.add_frame(Rgba::new(&small, 2, 2).unwrap(), 50).is_err());
            assert!(a.add_frame(img, 0).is_err());
            drop(a);
        }
    }

    /// The bytes that `finish` and `encode_still` return are a copy: they
    /// live on after the encoder and its buffers.
    #[test]
    fn the_output_outlives_the_encoder() {
        let (px, w, h) = alpha_ramp();
        let anim = {
            let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
            a.add_frame(Rgba::new(&px, w, h).unwrap(), 100).unwrap();
            a.finish().unwrap()
        };
        let still = {
            let copy = px.clone();
            encode_still(Rgba::new(&copy, w, h).unwrap(), Level::DEFAULT).unwrap()
        };
        let junk: Vec<Vec<u8>> = (0..8)
            .map(|_| vec![0x5A; anim.len().max(still.len())])
            .collect();
        drop(junk);
        assert_eq!(decode_still(&anim), (px.clone(), w, h));
        assert_eq!(decode_still(&still), (px, w, h));
    }

    #[test]
    fn rgba_shows_its_size_not_its_pixels() {
        let px = vec![0u8; 16];
        assert_eq!(
            format!("{:?}", Rgba::new(&px, 2, 2).unwrap()),
            "Rgba { width: 2, height: 2, len: 16 }"
        );
    }

    #[test]
    fn every_level_is_lossless() {
        let (px, w, h) = frame("let v = vec![1, 2];");
        for level in 0..=9 {
            let level = Level::new(level).unwrap();
            let webp = encode_still(Rgba::new(&px, w, h).unwrap(), level).unwrap();
            assert_eq!(decode_still(&webp).0, px, "{level:?}");
        }
    }

    #[test]
    fn same_pixels_give_same_bytes() {
        let (px, w, h) = frame("struct A;");
        let img = Rgba::new(&px, w, h).unwrap();
        assert_eq!(
            encode_still(img, Level::DEFAULT).unwrap(),
            encode_still(img, Level::DEFAULT).unwrap()
        );
        let anim = || {
            let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
            a.add_frame(img, 500).unwrap();
            a.add_frame(Rgba::new(&frame("struct B;").0, w, h).unwrap(), 700)
                .unwrap();
            a.finish().unwrap()
        };
        assert_eq!(anim(), anim());
    }

    #[test]
    fn animation_keeps_every_frame_and_its_time() {
        let texts = ["fn a() {}", "fn b() {}", "fn c() { a(); b(); }"];
        let frames: Vec<_> = texts.iter().map(|t| frame(t)).collect();
        let (w, h) = (frames[0].1, frames[0].2);
        let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
        for ((px, ..), ms) in frames.iter().zip([400, 1000, 2600]) {
            a.add_frame(Rgba::new(px, w, h).unwrap(), ms).unwrap();
        }
        let webp = a.finish().unwrap();

        let d = decode(&webp);
        assert!(d.is_animated());
        assert_eq!(d.loop_count(), image_webp::LoopCount::Forever);
        let got = self::frames(&webp);
        assert_eq!(got.len(), 3);
        for ((px, ..), ((out, delay), ms)) in frames.iter().zip(got.iter().zip([400, 1000, 2600])) {
            assert_eq!(*delay, ms);
            assert!(out == px, "a frame changed");
        }
    }

    #[test]
    fn animation_plays_as_many_times_as_asked() {
        let (px, w, h) = frame("let a = 1;");
        let (px2, ..) = frame("let a = 2;");
        let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 3).unwrap();
        a.add_frame(Rgba::new(&px, w, h).unwrap(), 100).unwrap();
        a.add_frame(Rgba::new(&px2, w, h).unwrap(), 100).unwrap();
        let webp = a.finish().unwrap();
        assert_eq!(
            decode(&webp).loop_count(),
            image_webp::LoopCount::Times(3.try_into().unwrap())
        );
    }

    #[test]
    fn equal_frames_add_up_their_time() {
        let (px, w, h) = frame("loop {}");
        let (px2, ..) = frame("loop { break; }");
        let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
        for (p, ms) in [(&px, 300), (&px, 200), (&px2, 100)] {
            a.add_frame(Rgba::new(p, w, h).unwrap(), ms).unwrap();
        }
        let webp = a.finish().unwrap();
        let mut d = decode(&webp);
        assert_eq!(d.num_frames(), 2);
        let mut out = vec![0; d.output_buffer_size().unwrap()];
        assert_eq!(d.read_frame(&mut out).unwrap(), 500);
        assert_eq!(d.read_frame(&mut out).unwrap(), 100);
    }

    #[test]
    fn one_frame_gives_a_still() {
        let (px, w, h) = alpha_ramp();
        let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
        a.add_frame(Rgba::new(&px, w, h).unwrap(), 1000).unwrap();
        assert_eq!(decode_still(&a.finish().unwrap()), (px, w, h));
    }

    #[test]
    fn bad_sizes_fail_before_libwebp() {
        let px = vec![0u8; 4 * 4];
        for (w, h) in [
            (0, 4),
            (4, 0),
            (MAX_SIDE + 1, 1),
            (1, MAX_SIDE + 1),
            (u32::MAX, u32::MAX),
        ] {
            assert_eq!(Rgba::new(&px, w, h).unwrap_err(), WebpError::Size(w, h));
            assert!(matches!(
                AnimEncoder::new(w, h, Level::DEFAULT, 0),
                Err(WebpError::Size(..))
            ));
        }
        for len in [0, 15, 17, 64] {
            assert_eq!(
                Rgba::new(&vec![0; len], 2, 2).unwrap_err(),
                WebpError::Length {
                    width: 2,
                    height: 2,
                    got: len,
                    expected: 16
                }
            );
        }
        assert!(Rgba::new(&px, 2, 2).is_ok());
        // The largest side is accepted: one row of MAX_SIDE pixels encodes.
        let row = vec![7u8; 4 * MAX_SIDE as usize];
        let webp = encode_still(
            Rgba::new(&row, MAX_SIDE, 1).unwrap(),
            Level::new(0).unwrap(),
        );
        assert_eq!(decode_still(&webp.unwrap()).1, MAX_SIDE);
        assert_eq!(Level::new(10).unwrap_err(), WebpError::Level(10));
    }

    #[test]
    fn bad_frames_are_refused_and_the_encoder_goes_on() {
        let (px, w, h) = alpha_ramp();
        let small = vec![0u8; 4 * 4];
        let mut a = AnimEncoder::new(w, h, Level::DEFAULT, 0).unwrap();
        let img = Rgba::new(&px, w, h).unwrap();
        assert_eq!(
            a.add_frame(Rgba::new(&small, 2, 2).unwrap(), 100)
                .unwrap_err(),
            WebpError::FrameSize {
                width: 2,
                height: 2,
                expected_width: w,
                expected_height: h
            }
        );
        assert_eq!(a.add_frame(img, 0).unwrap_err(), WebpError::Duration(0));
        assert_eq!(
            a.add_frame(img, MAX_FRAME_MS + 1).unwrap_err(),
            WebpError::Duration(MAX_FRAME_MS + 1)
        );
        // 200 frames of the longest time: the timestamps go past i32::MAX
        // milliseconds, and every frame keeps its time.
        let other: Vec<u8> = px.iter().map(|b| b ^ 0x55).collect();
        let other = Rgba::new(&other, w, h).unwrap();
        for i in 0..200 {
            a.add_frame(if i % 2 == 0 { img } else { other }, MAX_FRAME_MS)
                .unwrap();
        }
        let webp = a.finish().unwrap();
        assert_eq!(decode(&webp).loop_duration(), 200 * u64::from(MAX_FRAME_MS));
        assert_eq!(
            AnimEncoder::new(w, h, Level::DEFAULT, 0)
                .unwrap()
                .finish()
                .unwrap_err(),
            WebpError::NoFrames
        );
    }
}
