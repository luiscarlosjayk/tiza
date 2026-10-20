//! Animated GIF from Tiza's own frames (t-120): every frame is rasterized
//! by the PNG path (same pixels as `render --out x.png`), the palette is
//! built once for the whole animation (the colours that cover most pixels
//! — background, code, rails — are kept exactly; the anti-aliasing blends
//! are median-cut into what is left of the 256 entries; no dithering, so
//! text stays crisp), and every frame after the first is written as the
//! sub-rectangle that changed, so a seven-step demo stays small.

use std::collections::HashMap;

use tiza_core::canvas::Canvas;
use tiza_core::theme::Theme;

use crate::Error;
use crate::png::{PngOptions, rasterize};
use crate::svg::{SvgOptions, to_svg};

/// Shortest delay a GIF frame may have, in milliseconds (the format counts
/// in 10 ms units and players treat anything under 20 ms as 100 ms).
pub const MIN_DELAY_MS: u64 = 20;
/// Longest delay of one frame (u16 centiseconds); longer holds are split.
const MAX_DELAY_CS: u64 = u16::MAX as u64;

/// How long a frame asked to last `delay_ms` really lasts in the GIF, in
/// milliseconds: at least [`MIN_DELAY_MS`], at most two maximum holds, and
/// rounded up to the format's 10 ms unit. MP4/WebM time their frames with
/// the same function (t-170), so a typed move at 25 ms per character lasts
/// 30 ms per character in every format and the videos add up to the GIF's
/// length exactly.
pub fn frame_delay_ms(delay_ms: u64) -> u64 {
    delay_ms
        .clamp(MIN_DELAY_MS, tiza_core::model::MAX_HOLD_MS * 2)
        .div_ceil(10)
        * 10
}

#[derive(Debug, Clone)]
pub struct GifOptions {
    pub svg: SvgOptions,
    pub png: PngOptions,
    /// `None` loops forever; `Some(n)` plays n + 1 times (`Some(0)` once).
    pub repeat: Option<u16>,
}

impl Default for GifOptions {
    fn default() -> Self {
        Self {
            svg: SvgOptions {
                embed_fonts: false,
                ..Default::default()
            },
            png: PngOptions {
                scale: 1.0,
                system_fonts: false,
            },
            repeat: None,
        }
    }
}

/// What the encoder did, for the CLI's summary line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifReport {
    pub frames: usize,
    pub width: u32,
    pub height: u32,
    pub colors: usize,
    /// Milliseconds the animation lasts (one loop).
    pub duration_ms: u64,
}

/// Encodes `frames` (canvas, delay in ms) — all of one size, as
/// [`tiza_core::anim::frames`] gives them — into an animated GIF.
pub fn to_gif(
    frames: &[(Canvas, u64)],
    theme: &Theme,
    o: &GifOptions,
) -> Result<(Vec<u8>, GifReport), Error> {
    if frames.is_empty() {
        return Err(Error::Gif("no frames".into()));
    }
    // 1. Rasterize every frame (RGB).
    let mut rgb: Vec<Vec<[u8; 3]>> = Vec::with_capacity(frames.len());
    let (mut width, mut height) = (0u32, 0u32);
    for (i, (canvas, _)) in frames.iter().enumerate() {
        let svg = to_svg(canvas, theme, &o.svg);
        let pm = rasterize(&svg, &o.png)?;
        if i == 0 {
            width = pm.width();
            height = pm.height();
        } else if (pm.width(), pm.height()) != (width, height) {
            return Err(Error::Gif(format!(
                "frame {i} is {}×{} px, the first one {width}×{height}: every frame of a take must have the same size",
                pm.width(),
                pm.height()
            )));
        }
        // The background is opaque, so un-premultiplying is exact for the
        // text and the drawing and harmless elsewhere.
        rgb.push(
            pm.pixels()
                .iter()
                .map(|p| {
                    let c = p.demultiply();
                    [c.red(), c.green(), c.blue()]
                })
                .collect(),
        );
    }
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err(Error::TooBig(width, height));
    }
    // 2. One palette for the animation.
    let mut histogram: HashMap<[u8; 3], u64> = HashMap::new();
    for f in &rgb {
        for px in f {
            *histogram.entry(*px).or_insert(0) += 1;
        }
    }
    let palette = build_palette(&histogram, 256);
    let lookup = index_map(&histogram, &palette);
    let mut flat = Vec::with_capacity(palette.len() * 3);
    for c in &palette {
        flat.extend_from_slice(c);
    }
    // 3. Encode: the first frame whole, the rest as the changed rectangle.
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, width as u16, height as u16, &flat)
            .map_err(|e| Error::Gif(e.to_string()))?;
        enc.set_repeat(match o.repeat {
            None => gif::Repeat::Infinite,
            Some(n) => gif::Repeat::Finite(n),
        })
        .map_err(|e| Error::Gif(e.to_string()))?;
        let mut prev: Option<Vec<u8>> = None;
        for (i, f) in rgb.iter().enumerate() {
            let idx: Vec<u8> = f.iter().map(|px| lookup[px]).collect();
            let (left, top, w, h) = match &prev {
                None => (0, 0, width, height),
                Some(p) => changed_rect(p, &idx, width, height).unwrap_or((0, 0, 1, 1)),
            };
            let mut buffer = Vec::with_capacity((w * h) as usize);
            for y in top..top + h {
                let row = (y * width + left) as usize;
                buffer.extend_from_slice(&idx[row..row + w as usize]);
            }
            // A hold longer than the format's 655 s is split into frames
            // of the same rectangle.
            // Bounded (t-166): a hold past an hour is a bug upstream, and
            // u64::MAX ms would be 2.8e14 frames of 655 s.
            let mut cs = frame_delay_ms(frames[i].1) / 10;
            loop {
                let this = cs.min(MAX_DELAY_CS);
                enc.write_frame(&gif::Frame {
                    delay: this as u16,
                    dispose: gif::DisposalMethod::Keep,
                    transparent: None,
                    needs_user_input: false,
                    top: top as u16,
                    left: left as u16,
                    width: w as u16,
                    height: h as u16,
                    interlaced: false,
                    palette: None,
                    buffer: std::borrow::Cow::Borrowed(&buffer),
                })
                .map_err(|e| Error::Gif(e.to_string()))?;
                cs -= this;
                if cs == 0 {
                    break;
                }
            }
            prev = Some(idx);
        }
    }
    let duration_ms = frames
        .iter()
        .map(|(_, d)| frame_delay_ms(*d))
        .fold(0u64, u64::saturating_add);
    Ok((
        out,
        GifReport {
            frames: frames.len(),
            width,
            height,
            colors: palette.len(),
            duration_ms,
        },
    ))
}

/// Bounding rectangle of the pixels that differ between two index frames;
/// `None` when nothing changed.
fn changed_rect(a: &[u8], b: &[u8], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..height {
        let row = (y * width) as usize;
        let ra = &a[row..row + width as usize];
        let rb = &b[row..row + width as usize];
        if ra == rb {
            continue;
        }
        let first = ra.iter().zip(rb).position(|(p, q)| p != q).unwrap() as u32;
        let last = width
            - 1
            - ra.iter()
                .rev()
                .zip(rb.iter().rev())
                .position(|(p, q)| p != q)
                .unwrap() as u32;
        x0 = x0.min(first);
        x1 = x1.max(last);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// The palette: every colour when there are at most `max`; otherwise the
/// `max / 4` most frequent ones exactly (background, code, rails, labels —
/// the colours that must not shift) and a median cut of the rest.
fn build_palette(histogram: &HashMap<[u8; 3], u64>, max: usize) -> Vec<[u8; 3]> {
    if histogram.len() <= max {
        let mut all: Vec<[u8; 3]> = histogram.keys().copied().collect();
        all.sort_unstable();
        return all;
    }
    let mut by_count: Vec<([u8; 3], u64)> = histogram.iter().map(|(c, n)| (*c, *n)).collect();
    by_count.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let exact = max / 4;
    let mut palette: Vec<[u8; 3]> = by_count[..exact].iter().map(|(c, _)| *c).collect();
    let rest: Vec<([u8; 3], u64)> = by_count[exact..].to_vec();
    palette.extend(median_cut(rest, max - exact));
    palette
}

/// Weighted median cut: splits the box with the widest channel range at its
/// weighted median until `boxes` boxes exist, then averages each.
fn median_cut(colors: Vec<([u8; 3], u64)>, boxes: usize) -> Vec<[u8; 3]> {
    let mut groups: Vec<Vec<([u8; 3], u64)>> = vec![colors];
    while groups.len() < boxes {
        // The box with the widest range (ties: the heavier one).
        let Some((gi, ch)) = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.len() > 1)
            .map(|(i, g)| {
                let (ch, range) = widest_channel(g);
                (i, ch, range, g.iter().map(|(_, n)| *n).sum::<u64>())
            })
            .max_by_key(|(_, _, range, weight)| (*range, *weight))
            .map(|(i, ch, _, _)| (i, ch))
        else {
            break;
        };
        let mut g = groups.swap_remove(gi);
        g.sort_unstable_by_key(|(c, _)| c[ch]);
        let total: u64 = g.iter().map(|(_, n)| *n).sum();
        let mut acc = 0u64;
        let mut cut = 0;
        for (i, (_, n)) in g.iter().enumerate() {
            acc += n;
            if acc * 2 >= total {
                cut = i + 1;
                break;
            }
        }
        let cut = cut.clamp(1, g.len() - 1);
        let right = g.split_off(cut);
        groups.push(g);
        groups.push(right);
    }
    groups
        .iter()
        .map(|g| {
            let w: u64 = g.iter().map(|(_, n)| *n).sum::<u64>().max(1);
            let mut sum = [0u64; 3];
            for (c, n) in g {
                for k in 0..3 {
                    sum[k] += c[k] as u64 * n;
                }
            }
            [(sum[0] / w) as u8, (sum[1] / w) as u8, (sum[2] / w) as u8]
        })
        .collect()
}

fn widest_channel(g: &[([u8; 3], u64)]) -> (usize, u8) {
    let mut best = (0, 0u8);
    for ch in 0..3 {
        let (lo, hi) = g.iter().fold((255u8, 0u8), |(lo, hi), (c, _)| {
            (lo.min(c[ch]), hi.max(c[ch]))
        });
        let range = hi - lo;
        if range > best.1 {
            best = (ch, range);
        }
    }
    best
}

/// Nearest palette entry for every colour of the histogram (exact for the
/// colours that are in the palette).
fn index_map(histogram: &HashMap<[u8; 3], u64>, palette: &[[u8; 3]]) -> HashMap<[u8; 3], u8> {
    let exact: HashMap<[u8; 3], u8> = palette
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, i as u8))
        .collect();
    histogram
        .keys()
        .map(|c| {
            let i = exact.get(c).copied().unwrap_or_else(|| {
                palette
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| {
                        (0..3)
                            .map(|k| (c[k] as i32 - p[k] as i32).pow(2))
                            .sum::<i32>()
                    })
                    .map(|(i, _)| i as u8)
                    .unwrap_or(0)
            });
            (*c, i)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiza_core::canvas::{CellStyle, Rgb};

    fn canvas(text: &str) -> Canvas {
        let mut c = Canvas::new_filled(20, 3, Theme::dark().base());
        c.put_str(0, 1, text, CellStyle::new().fg(Rgb(255, 255, 255)));
        c
    }

    #[test]
    fn frames_become_a_looping_gif_with_sub_rectangles() {
        let frames = vec![
            (canvas("fn main() {}"), 2000),
            (canvas("fn main() { x }"), 25),
            (canvas("fn main() { x }"), 4000),
        ];
        let (bytes, report) = to_gif(&frames, &Theme::dark(), &GifOptions::default()).unwrap();
        assert_eq!(&bytes[..6], b"GIF89a");
        assert_eq!(report.frames, 3);
        assert!(report.colors <= 256 && report.colors > 2);
        // 2000 + 25→30 + 4000
        assert_eq!(report.duration_ms, 6030);
        // The NETSCAPE loop block is there, and three image descriptors.
        assert!(bytes.windows(11).any(|w| w == b"NETSCAPE2.0"));
        let mut d = gif::DecodeOptions::new();
        d.set_color_output(gif::ColorOutput::Indexed);
        let mut dec = d.read_info(std::io::Cursor::new(&bytes)).unwrap();
        let first = dec.read_next_frame().unwrap().unwrap().clone();
        assert_eq!((first.left, first.top), (0, 0));
        assert_eq!(
            (first.width as u32, first.height as u32),
            (report.width, report.height)
        );
        let second = dec.read_next_frame().unwrap().unwrap().clone();
        assert!(
            second.width < first.width,
            "the diff frame is a sub-rectangle"
        );
        assert_eq!(second.delay, 3);
        let third = dec.read_next_frame().unwrap().unwrap().clone();
        assert_eq!(
            (third.width, third.height),
            (1, 1),
            "nothing changed: a 1×1 frame holds"
        );
        assert_eq!(third.delay, 400);
        assert!(dec.read_next_frame().unwrap().is_none());
    }

    #[test]
    fn the_palette_keeps_the_frequent_colours_exactly() {
        let mut h = HashMap::new();
        h.insert([10, 10, 10], 1_000_000);
        h.insert([250, 250, 250], 100_000);
        for i in 0..1000u32 {
            h.insert(
                [(i % 256) as u8, (i / 4 % 256) as u8, (i * 7 % 256) as u8],
                1 + i as u64 % 3,
            );
        }
        let p = build_palette(&h, 256);
        assert_eq!(p.len(), 256);
        assert!(p.contains(&[10, 10, 10]) && p.contains(&[250, 250, 250]));
        let m = index_map(&h, &p);
        assert_eq!(p[m[&[10, 10, 10]] as usize], [10, 10, 10]);
        let (w, _) = changed_rect(&[0, 0, 0, 0], &[0, 1, 0, 0], 2, 2)
            .map(|r| (r, ()))
            .unwrap();
        assert_eq!(w, (1, 0, 1, 1));
        assert!(changed_rect(&[0, 0], &[0, 0], 2, 1).is_none());
    }
}
