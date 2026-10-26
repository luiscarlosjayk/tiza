//! The calls into libwebp: the only `unsafe` code of Tiza.
//!
//! Each type here owns one libwebp object and frees it in `Drop`, so a
//! return at any point leaves nothing behind. The checks that the calls
//! rely on are in this file, next to the calls: the size of a picture and
//! the length and the alignment of its pixels. Only [`Rgba::new`] makes an
//! [`Rgba`], but the checks here do not count on it.
//!
//! A struct of libwebp starts as all zeros and then goes through its init
//! function: all zeros is a valid value of each one (integers, floats,
//! null pointers, `None` callbacks, and enums whose 0 is a variant), and
//! the init functions do not set every field. The structs are `Copy`: each
//! one is built inside the expression that wraps it, so no copy of it
//! stays outside its owner.
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block
)]

use std::ffi::{CStr, c_int};
use std::ptr::{self, NonNull};

use libwebp_sys as sys;

use super::rgba::{byte_len, check_size};
use super::{EncoderError, Rgba, WebpError};

const ENCODER_ABI: c_int = sys::WEBP_ENCODER_ABI_VERSION as c_int;
const MUX_ABI: c_int = sys::WEBP_MUX_ABI_VERSION as c_int;

/// Width and height as libwebp takes them: 1 to 16383 each.
fn sides(width: u32, height: u32) -> Result<(c_int, c_int), WebpError> {
    check_size(width, height)?;
    match (c_int::try_from(width), c_int::try_from(height)) {
        (Ok(w), Ok(h)) => Ok((w, h)),
        _ => Err(WebpError::Size(width, height)),
    }
}

/// The error that a libwebp error code stands for (`WebPEncodingError`).
fn encoding_error(code: u32) -> WebpError {
    WebpError::Encoder(match code {
        1 | 2 => EncoderError::OutOfMemory,
        4 => EncoderError::InvalidConfiguration,
        5 => EncoderError::BadDimension,
        8 => EncoderError::BadWrite,
        9 => EncoderError::FileTooBig,
        // 0 is VP8_ENC_OK: a failure with no reason.
        0 => EncoderError::Unknown,
        n => EncoderError::Code(n),
    })
}

/// A lossless configuration at `level`: same pixels in, same bytes out.
struct Config(sys::WebPConfig);

impl Config {
    fn lossless(level: u8) -> Result<Self, WebpError> {
        // SAFETY: all zeros is a valid WebPConfig (see the module comment).
        let mut c: sys::WebPConfig = unsafe { std::mem::zeroed() };
        // SAFETY: `c` is a valid, writable WebPConfig.
        let ok = unsafe {
            sys::WebPConfigInitInternal(
                &mut c,
                sys::WebPPreset::WEBP_PRESET_DEFAULT,
                75.0,
                ENCODER_ABI,
            )
        };
        if ok == 0 {
            return Err(WebpError::Encoder(EncoderError::VersionMismatch));
        }
        // SAFETY: `c` is an initialized WebPConfig; libwebp checks `level`.
        if unsafe { sys::WebPConfigLosslessPreset(&mut c, c_int::from(level)) } == 0 {
            return Err(WebpError::Level(level));
        }
        // Keep the color of fully transparent pixels.
        c.exact = 1;
        // One thread: the output does not depend on timing.
        c.thread_level = 0;
        // SAFETY: `c` is an initialized WebPConfig, only read.
        if unsafe { sys::WebPValidateConfig(&c) } == 0 {
            return Err(WebpError::Encoder(EncoderError::InvalidConfiguration));
        }
        Ok(Self(c))
    }
}

/// A picture with its own copy of the pixels, in ARGB.
struct Picture(sys::WebPPicture);

impl Picture {
    fn import(img: &Rgba<'_>) -> Result<Self, WebpError> {
        let (width, height) = (img.width(), img.height());
        let (w, h) = sides(width, height)?;
        let data = img.bytes();
        // The length that libwebp reads, checked here, next to the call.
        let expected = byte_len(width, height).ok_or(WebpError::Size(width, height))?;
        if data.len() != expected {
            return Err(WebpError::Length {
                width,
                height,
                got: data.len(),
                expected,
            });
        }
        let stride = w.checked_mul(4).ok_or(WebpError::Size(width, height))?;
        // libwebp reads the pixels as `uint32_t`: pixels that do not start
        // on a multiple of 4 bytes are copied first.
        let copy: Vec<u32>;
        let pixels: *const u8 = if data.as_ptr().cast::<u32>().is_aligned() {
            data.as_ptr()
        } else {
            // No remainder: the length is a multiple of 4 (checked above).
            let (px, _) = data.as_chunks::<4>();
            copy = px.iter().map(|p| u32::from_ne_bytes(*p)).collect();
            copy.as_ptr().cast()
        };
        // From here, Drop frees what the import allocates, also on an error.
        let mut pic = Self({
            // SAFETY: all zeros is a valid WebPPicture (see the module comment).
            let mut raw: sys::WebPPicture = unsafe { std::mem::zeroed() };
            // SAFETY: `raw` is a valid, writable WebPPicture; it owns no memory yet.
            if unsafe { sys::WebPPictureInitInternal(&mut raw, ENCODER_ABI) } == 0 {
                return Err(WebpError::Encoder(EncoderError::VersionMismatch));
            }
            raw.use_argb = 1;
            raw.width = w;
            raw.height = h;
            raw
        });
        // SAFETY: `pixels` points to `h` rows of `stride` bytes, exactly
        // (the length is checked above; `copy` has the same bytes), aligned
        // to 4 bytes. libwebp reads only those and copies them into a buffer
        // of its own, owned by `pic`. `data` and `copy` live for the whole
        // call, and libwebp keeps no pointer to them after it.
        let ok = unsafe { sys::WebPPictureImportRGBA(&mut pic.0, pixels, stride) };
        if ok == 0 {
            return Err(pic.error());
        }
        Ok(pic)
    }

    /// What `error_code` says, read as a plain integer: a value that is not
    /// a variant of the Rust enum is then never made.
    fn error(&self) -> WebpError {
        // SAFETY: the pointer comes from a live reference, so it is aligned
        // and valid; the enum is `repr(u32)`, so the field is a u32.
        let code = unsafe { ptr::addr_of!(self.0.error_code).cast::<u32>().read() };
        encoding_error(code)
    }
}

impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: `self.0` was initialized by WebPPictureInitInternal and is
        // freed only here; WebPPictureFree also accepts a picture that owns
        // no buffer.
        unsafe { sys::WebPPictureFree(&mut self.0) };
    }
}

/// The output buffer of WebPEncode. Boxed, so the address that the picture
/// holds stays the same when the writer moves.
struct MemoryWriter(Box<sys::WebPMemoryWriter>);

impl MemoryWriter {
    fn new() -> Self {
        // SAFETY: all zeros is a valid WebPMemoryWriter (null, 0, 0).
        let mut w: Box<sys::WebPMemoryWriter> = Box::new(unsafe { std::mem::zeroed() });
        // SAFETY: `w` is a valid, writable WebPMemoryWriter.
        unsafe { sys::WebPMemoryWriterInit(&mut *w) };
        Self(w)
    }

    /// A copy of the output: the Vec owns its bytes, so it lives on after
    /// the writer.
    fn to_vec(&self) -> Result<Vec<u8>, WebpError> {
        if self.0.mem.is_null() || self.0.size == 0 {
            return Err(WebpError::Encoder(EncoderError::NoOutput));
        }
        // SAFETY: libwebp keeps `mem` pointing to `size` initialized bytes
        // that it allocated; they stay until Drop.
        let bytes = unsafe { std::slice::from_raw_parts(self.0.mem, self.0.size) };
        Ok(bytes.to_vec())
    }
}

impl Drop for MemoryWriter {
    fn drop(&mut self) {
        // SAFETY: the writer was initialized by WebPMemoryWriterInit and is
        // cleared only here.
        unsafe { sys::WebPMemoryWriterClear(&mut *self.0) };
    }
}

pub(super) fn encode_still(img: &Rgba<'_>, level: u8) -> Result<Vec<u8>, WebpError> {
    let config = Config::lossless(level)?;
    let mut pic = Picture::import(img)?;
    let mut out = MemoryWriter::new();
    pic.0.writer = Some(sys::WebPMemoryWrite);
    pic.0.custom_ptr = ptr::addr_of_mut!(*out.0).cast();
    // SAFETY: `config` is validated, `pic` holds its pixels, and
    // `custom_ptr` points to the boxed writer, which `WebPMemoryWrite`
    // expects and which lives until after this call.
    let ok = unsafe { sys::WebPEncode(&config.0, &mut pic.0) };
    // The picture must not keep a pointer to the writer.
    pic.0.writer = None;
    pic.0.custom_ptr = ptr::null_mut();
    if ok == 0 {
        return Err(pic.error());
    }
    out.to_vec()
}

/// Bytes that libwebp allocated for an assembled file.
struct Data(sys::WebPData);

impl Drop for Data {
    fn drop(&mut self) {
        // SAFETY: `bytes` is null or a buffer from libwebp's allocator, and
        // it is freed only here.
        unsafe { sys::WebPDataClear(&mut self.0) };
    }
}

/// A WebPAnimEncoder. The `PhantomData` of `AnimEncoder`, which owns it,
/// keeps it on one thread.
pub(super) struct Anim {
    enc: NonNull<sys::WebPAnimEncoder>,
    config: Config,
}

impl Anim {
    /// `plays`: how many times the animation plays, 0 for forever.
    pub(super) fn new(width: u32, height: u32, level: u8, plays: u16) -> Result<Self, WebpError> {
        let (w, h) = sides(width, height)?;
        let config = Config::lossless(level)?;
        // SAFETY: all zeros is a valid WebPAnimEncoderOptions.
        let mut opts: sys::WebPAnimEncoderOptions = unsafe { std::mem::zeroed() };
        // SAFETY: `opts` is a valid, writable WebPAnimEncoderOptions.
        if unsafe { sys::WebPAnimEncoderOptionsInitInternal(&mut opts, MUX_ABI) } == 0 {
            return Err(WebpError::Encoder(EncoderError::VersionMismatch));
        }
        opts.anim_params.loop_count = c_int::from(plays);
        // Transparent, as the corners of a frame: libwebp defaults to white.
        opts.anim_params.bgcolor = 0;
        // SAFETY: `opts` is initialized and only read; the sizes are 1 to
        // 16383. The encoder copies the options.
        let raw = unsafe { sys::WebPAnimEncoderNewInternal(w, h, &opts, MUX_ABI) };
        let enc = NonNull::new(raw).ok_or(WebpError::Encoder(EncoderError::OutOfMemory))?;
        Ok(Self { enc, config })
    }

    /// Adds a frame that starts at `timestamp_ms`. The caller gives frames
    /// of the encoder's size, with timestamps 1 to 2^24 - 1 ms apart. libwebp
    /// only subtracts timestamps, as `uint32_t`, so a timestamp can wrap
    /// past `i32::MAX`.
    pub(super) fn add(&mut self, img: &Rgba<'_>, timestamp_ms: c_int) -> Result<(), WebpError> {
        let mut pic = Picture::import(img)?;
        // SAFETY: `enc` is live until Drop; `pic` is a valid ARGB picture
        // and `config` a validated configuration. The encoder copies the
        // pixels during the call and keeps no pointer to `pic` or `config`.
        let ok = unsafe {
            sys::WebPAnimEncoderAdd(self.enc.as_ptr(), &mut pic.0, timestamp_ms, &self.config.0)
        };
        if ok == 0 {
            return Err(self.error());
        }
        Ok(())
    }

    /// The file: the last frame lasts until `end_ms`.
    pub(super) fn finish(self, end_ms: c_int) -> Result<Vec<u8>, WebpError> {
        // SAFETY: `enc` is live; a null frame marks the end of the
        // animation, and a null config is allowed with it.
        let ok = unsafe {
            sys::WebPAnimEncoderAdd(self.enc.as_ptr(), ptr::null_mut(), end_ms, ptr::null())
        };
        if ok == 0 {
            return Err(self.error());
        }
        let mut data = Data(sys::WebPData::default());
        // SAFETY: `enc` is live and `data.0` is an empty WebPData that the
        // encoder fills with a buffer that Drop of `data` frees.
        if unsafe { sys::WebPAnimEncoderAssemble(self.enc.as_ptr(), &mut data.0) } == 0 {
            return Err(self.error());
        }
        if data.0.bytes.is_null() || data.0.size == 0 {
            return Err(WebpError::Encoder(EncoderError::NoOutput));
        }
        // SAFETY: after a successful assembly, `bytes` points to `size`
        // initialized bytes owned by `data`.
        let bytes = unsafe { std::slice::from_raw_parts(data.0.bytes, data.0.size) };
        // A copy: the Vec lives on after `data` and the encoder.
        Ok(bytes.to_vec())
    }

    fn error(&self) -> WebpError {
        // SAFETY: `enc` is live.
        let p = unsafe { sys::WebPAnimEncoderGetError(self.enc.as_ptr()) };
        let msg = if p.is_null() {
            String::new()
        } else {
            // SAFETY: `p` points to a string that ends in NUL, inside the
            // encoder; it is copied here, before the encoder can change it.
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        };
        WebpError::Encoder(if msg.is_empty() {
            EncoderError::Unknown
        } else {
            EncoderError::Animation(msg)
        })
    }
}

impl Drop for Anim {
    fn drop(&mut self) {
        // SAFETY: `enc` came from WebPAnimEncoderNewInternal and is deleted
        // only here.
        unsafe { sys::WebPAnimEncoderDelete(self.enc.as_ptr()) };
    }
}
