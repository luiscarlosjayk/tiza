//! [`Rgba`]: pixels whose size and length are checked. Its fields are
//! private to this file, so only [`Rgba::new`] makes one, also in the rest
//! of the `webp` module.

use super::{MAX_SIDE, WebpError};

/// Pixels as straight (not premultiplied) RGBA, 4 bytes per pixel, row by
/// row with no padding, 1 to [`MAX_SIDE`] px per side.
#[derive(Clone, Copy)]
pub struct Rgba<'a> {
    data: &'a [u8],
    width: u32,
    height: u32,
}

impl<'a> Rgba<'a> {
    pub fn new(data: &'a [u8], width: u32, height: u32) -> Result<Self, WebpError> {
        check_size(width, height)?;
        let expected = byte_len(width, height).ok_or(WebpError::Size(width, height))?;
        if data.len() != expected {
            return Err(WebpError::Length {
                width,
                height,
                got: data.len(),
                expected,
            });
        }
        Ok(Self {
            data,
            width,
            height,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixels: `width * height * 4` bytes.
    pub(super) fn bytes(&self) -> &'a [u8] {
        self.data
    }
}

/// The size and the length, not the pixels: a frame has megabytes of them.
impl std::fmt::Debug for Rgba<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rgba")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("len", &self.data.len())
            .finish()
    }
}

pub(super) fn check_size(width: u32, height: u32) -> Result<(), WebpError> {
    if (1..=MAX_SIDE).contains(&width) && (1..=MAX_SIDE).contains(&height) {
        Ok(())
    } else {
        Err(WebpError::Size(width, height))
    }
}

/// `width * height * 4`, or `None` on an overflow.
pub(super) fn byte_len(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
}
