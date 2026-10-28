//! Animated GIF from Tiza's own frames: every frame is rasterized
//! by the PNG path in strips of rows (the pixels of `render --out x.png`,
//! except some edge pixels of a glyph or a round corner at the edge of a
//! strip, and at row 8191 of a taller image),
//! the palette is built once for the whole animation (the colors that
//! cover most pixels — background, code, rails — are kept exactly; the
//! anti-aliasing blends are median-cut into what is left of the 256
//! entries; no dithering, so text stays crisp), and every frame after the
//! first is written as the sub-rectangle that changed, so a seven-step
//! demo stays small.

use std::collections::HashMap;

use resvg::tiny_skia::{Pixmap, PremultipliedColorU8};
use tiza_core::canvas::Canvas;
use tiza_core::theme::Theme;

use crate::Error;
use crate::png::{PngOptions, Raster, STRIP_ROWS, Strips};
use crate::svg::{Geometry, SvgOptions, geometry, to_svg, to_svg_rows};

/// Shortest delay a GIF frame may have, in milliseconds (the format counts
/// in 10 ms units and players treat anything under 20 ms as 100 ms).
pub const MIN_DELAY_MS: u64 = 20;
/// Longest delay of one frame (u16 centiseconds); longer holds are split.
const MAX_DELAY_CS: u64 = u16::MAX as u64;

/// How long a frame asked to last `delay_ms` really lasts in the GIF, in
/// milliseconds: at least [`MIN_DELAY_MS`], at most two maximum holds, and
/// rounded up to the format's 10 ms unit. MP4/WebM time their frames with
/// the same function, so a typed move at 25 ms per character lasts
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
    encode(frames, theme, o, true).map(|(bytes, report, _)| (bytes, report))
}

/// How much [`encode`] rasterized: strips, and bytes of the SVGs it parsed.
#[derive(Debug, Default)]
struct Work {
    strips: usize,
    svg_bytes: usize,
}

/// [`to_gif`], and how much it rasterized. With `reuse`, a strip with no
/// changed row of cells near it is not rasterized again: its colors and its
/// palette indexes are those of the frame before, and the SVG of the frame
/// has only the rows that can draw in the other strips. Without it (only
/// the tests), every strip and every row is.
fn encode(
    frames: &[(Canvas, u64)],
    theme: &Theme,
    o: &GifOptions,
    reuse: bool,
) -> Result<(Vec<u8>, GifReport, Work), Error> {
    if frames.is_empty() {
        return Err(Error::Gif("no frames".into()));
    }
    // 1. The colors of the whole animation. Each frame is rasterized
    // twice, here and in step 3, one strip of rows at a time (after the
    // first frame, only the strips near a changed row): to keep the pixels
    // of every frame between the two steps would take 3 bytes per pixel and
    // frame (2 GB for 22 frames of 300 lines at scale 2), and the pixels of
    // one whole frame take 4 bytes per pixel (117 MB).
    let (mut width, mut height) = (0u32, 0u32);
    let mut histogram: HashMap<[u8; 3], u64> = HashMap::new();
    let mut strips = Strips::default();
    // The strips of frame `i` to rasterize: `None` for all of them.
    let dirty_of = |i: usize, height: u32| {
        (reuse && i > 0).then(|| dirty_strips(&frames[i - 1].0, &frames[i].0, o, height))
    };
    let wanted = |dirty: &Option<Vec<bool>>, k: usize| dirty.as_ref().is_none_or(|d| d[k]);
    // The colors of each strip when it was last rasterized: a strip that
    // is not rasterized again counts them again.
    let mut strip_colors: Vec<HashMap<[u8; 3], u64>> = Vec::new();
    let mut work = Work::default();
    for (i, (canvas, _)) in frames.iter().enumerate() {
        let dirty = dirty_of(i, height);
        if dirty.as_ref().is_none_or(|d| d.contains(&true)) {
            let raster = frame_raster(canvas, theme, o, height, &dirty, &mut work)?;
            let (w, h) = (raster.width, raster.height);
            if i == 0 {
                (width, height) = (w, h);
                strip_colors = vec![HashMap::new(); h.div_ceil(STRIP_ROWS) as usize];
            } else if (w, h) != (width, height) {
                return Err(Error::Gif(format!(
                    "frame {i} is {w}×{h} px, the first one {width}×{height}: every frame of a take must have the same size"
                )));
            }
            raster.strips(
                &mut strips,
                |k| wanted(&dirty, k),
                |k, pm| {
                    work.strips += 1;
                    let colors = &mut strip_colors[k];
                    colors.clear();
                    for (color, n) in color_runs(pm) {
                        *colors.entry(color).or_insert(0) += n as u64;
                    }
                    Ok(())
                },
            )?;
        }
        for colors in &strip_colors {
            for (color, n) in colors {
                *histogram.entry(*color).or_insert(0) += n;
            }
        }
    }
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err(Error::TooBig(width, height));
    }
    // 2. One palette for the animation.
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
        // The palette indexes of the frame on screen: one buffer of one
        // byte per pixel for the whole animation.
        let mut screen = Screen::new(width, height);
        // The changed rectangle of each frame, one buffer for the whole
        // animation.
        let mut buffer: Vec<u8> = Vec::new();
        for (i, (canvas, _)) in frames.iter().enumerate() {
            // Rasterizing is deterministic: the same strips, with the same
            // pixels, as in step 1. A frame with another size, or with a
            // color that step 1 did not count, is an error, not a panic.
            // A strip that is not rasterized again has the pixels of the
            // frame before, so its indexes are on the screen already.
            let dirty = dirty_of(i, height);
            screen.start();
            if dirty.as_ref().is_none_or(|d| d.contains(&true)) {
                let raster = frame_raster(canvas, theme, o, height, &dirty, &mut work)?;
                let differs =
                    || Error::Gif(format!("frame {i} rasterized differently the second time"));
                if (raster.width, raster.height) != (width, height) {
                    return Err(differs());
                }
                raster.strips(
                    &mut strips,
                    |k| wanted(&dirty, k),
                    |k, pm| {
                        work.strips += 1;
                        let start = k * STRIP_ROWS as usize * width as usize;
                        screen.seek(start);
                        for (color, n) in color_runs(pm) {
                            let index = *lookup.get(&color).ok_or_else(differs)?;
                            screen.write(index, n);
                        }
                        // The runs cover the strip.
                        debug_assert_eq!(screen.at, start + pm.data().len() / 4);
                        Ok(())
                    },
                )?;
                // The parsed frame is freed here, before its rectangle is
                // copied.
            }
            let (left, top, w, h) = if i == 0 {
                (0, 0, width, height)
            } else {
                screen.changed().unwrap_or((0, 0, 1, 1))
            };
            buffer.clear();
            for y in top..top + h {
                let row = (y * width + left) as usize;
                buffer.extend_from_slice(&screen.idx[row..row + w as usize]);
            }
            // A hold longer than the format's 655 s is split into frames
            // of the same rectangle.
            // Bounded: a hold past an hour is a bug upstream, and
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
        work,
    ))
}

/// Rows of cells around a changed row whose strips are rasterized again
/// too: a glyph can draw a little outside its row. Measured on every
/// animation of the examples and of the QA fixtures, at scales 1 to 3: at
/// most 1 pixel outside the band of its row. The embedded faces draw at
/// most 0.1 em above it, and a row is 1.32 em, so one row is a wide margin.
const DIRTY_MARGIN_ROWS: usize = 1;

/// A grapheme of more chars than this (a base and one mark) can stack its
/// marks past the margin: a frame that changes a row with one rasterizes
/// every strip, and a frame with one in any row has every row in its SVG.
const MARGIN_CHARS: usize = 2;

/// A grapheme that can draw past [`DIRTY_MARGIN_ROWS`].
fn tall_grapheme(c: &Canvas, x: usize, y: usize) -> bool {
    c.get(x, y)
        .is_some_and(|cell| cell.sym.chars().count() > MARGIN_CHARS)
}

/// The strips that row `y` of cells can draw in: those that its band of
/// pixel rows touches, with [`DIRTY_MARGIN_ROWS`] around it. Row 0 too has
/// a margin above it, in the margin of the image.
fn row_strips(g: &Geometry, scale: f32, height: u32, y: usize) -> std::ops::Range<usize> {
    let pixel_row = |row: f32| (g.margin + row * g.cell_h) * scale;
    let top = pixel_row(y as f32 - DIRTY_MARGIN_ROWS as f32)
        .floor()
        .max(0.0) as u32;
    let bottom = (pixel_row((y + 1 + DIRTY_MARGIN_ROWS) as f32).ceil() as u32).min(height);
    (top / STRIP_ROWS) as usize..bottom.div_ceil(STRIP_ROWS) as usize
}

/// Which strips of the frame `now` can have other pixels than in the frame
/// `before` it: those that touch a row of cells that changed, or the
/// [`DIRTY_MARGIN_ROWS`] around it. The SVG of a row comes only from its
/// cells, so a strip far from every changed row has the same pixels.
/// Every strip changes with frames of other sizes (an error of
/// [`to_gif`]), with the system fonts (their glyphs can be taller than a
/// row), and with a changed row that has a grapheme of more than
/// [`MARGIN_CHARS`] chars.
fn dirty_strips(before: &Canvas, now: &Canvas, o: &GifOptions, height: u32) -> Vec<bool> {
    let strips = height.div_ceil(STRIP_ROWS) as usize;
    if o.png.system_fonts || (before.width, before.height) != (now.width, now.height) {
        return vec![true; strips];
    }
    let g = geometry(now, &o.svg);
    let mut dirty = vec![false; strips];
    for y in 0..now.height {
        if (0..now.width).all(|x| before.get(x, y) == now.get(x, y)) {
            continue;
        }
        if (0..now.width).any(|x| tall_grapheme(before, x, y) || tall_grapheme(now, x, y)) {
            return vec![true; strips];
        }
        for k in row_strips(&g, o.png.scale, height, y) {
            dirty[k] = true;
        }
    }
    dirty
}

/// The rows of cells that the SVG of the frame `now` needs to rasterize
/// its `dirty` strips: those that can draw in one of them. Every row with
/// the system fonts, and when a grapheme of the frame, in a changed row or
/// not, can draw past the margin: a row that is not in the SVG draws
/// nothing.
fn needed_rows(now: &Canvas, o: &GifOptions, height: u32, dirty: &[bool]) -> Vec<bool> {
    if o.png.system_fonts
        || (0..now.height).any(|y| (0..now.width).any(|x| tall_grapheme(now, x, y)))
    {
        return vec![true; now.height];
    }
    let g = geometry(now, &o.svg);
    (0..now.height)
        .map(|y| row_strips(&g, o.png.scale, height, y).any(|k| dirty[k]))
        .collect()
}

/// One frame, parsed for the PNG path: the whole SVG, or with `dirty`
/// strips only the rows that can draw in them. Over the limits, an error
/// comes before the SVG, not after it.
fn frame_raster(
    canvas: &Canvas,
    theme: &Theme,
    o: &GifOptions,
    height: u32,
    dirty: &Option<Vec<bool>>,
    work: &mut Work,
) -> Result<Raster, Error> {
    crate::limits::image_size(canvas, &o.svg, &o.png)?;
    let svg = match dirty {
        None => to_svg(canvas, theme, &o.svg),
        Some(dirty) => {
            let rows = needed_rows(canvas, o, height, dirty);
            to_svg_rows(canvas, theme, &o.svg, |y| rows[y])
        }
    };
    work.svg_bytes += svg.len();
    Raster::new(&svg, &o.png)
}

/// The pixels of a frame as runs of one color, row after row. A frame of
/// code has large areas of one color, so its colors are counted and
/// looked up once for each run, not for each pixel.
fn color_runs(pm: &Pixmap) -> impl Iterator<Item = ([u8; 3], usize)> + '_ {
    pm.pixels()
        .chunk_by(|a, b| a == b)
        .map(|run| (rgb(&run[0]), run.len()))
}

/// The RGB of a pixel. The background is opaque, so un-premultiplying is
/// exact for the text and the drawing and harmless elsewhere.
fn rgb(p: &PremultipliedColorU8) -> [u8; 3] {
    let c = p.demultiply();
    [c.red(), c.green(), c.blue()]
}

/// The palette indexes of the frame on screen, one byte per pixel. Each
/// frame is written over the one before it, run by run, and the pixels
/// that change give the rectangle to encode: no copy of the frame before.
struct Screen {
    idx: Vec<u8>,
    width: usize,
    /// The next pixel to write.
    at: usize,
    /// Left, top, right and bottom (inclusive) of the pixels that changed.
    bounds: Option<(usize, usize, usize, usize)>,
}

impl Screen {
    fn new(width: u32, height: u32) -> Self {
        Self {
            idx: vec![0; width as usize * height as usize],
            width: width as usize,
            at: 0,
            bounds: None,
        }
    }

    /// The next frame starts at the first pixel, with nothing changed.
    fn start(&mut self) {
        self.at = 0;
        self.bounds = None;
    }

    /// The next pixel to write is pixel `at` (row after row). The pixels
    /// before it that are not written keep their indexes.
    fn seek(&mut self, at: usize) {
        self.at = at;
    }

    /// Writes `n` pixels of `index`, row by row: a run can go past the end
    /// of a row.
    fn write(&mut self, index: u8, n: usize) {
        let end = self.at + n;
        while self.at < end {
            let (x, y) = (self.at % self.width, self.at / self.width);
            let stop = end.min((y + 1) * self.width);
            let row = &mut self.idx[self.at..stop];
            if let Some(first) = row.iter().position(|&p| p != index) {
                let last = row.iter().rposition(|&p| p != index).unwrap_or(first);
                row[first..=last].fill(index);
                let (x0, x1) = (x + first, x + last);
                // Rows come in order: the top is the first row that changed.
                self.bounds = Some(match self.bounds {
                    None => (x0, y, x1, y),
                    Some((l, t, r, _)) => (l.min(x0), t, r.max(x1), y),
                });
            }
            self.at = stop;
        }
    }

    /// The rectangle that changed since the frame before (left, top,
    /// width, height); `None` when nothing changed.
    fn changed(&self) -> Option<(u32, u32, u32, u32)> {
        self.bounds
            .map(|(l, t, r, b)| (l as u32, t as u32, (r - l + 1) as u32, (b - t + 1) as u32))
    }
}

/// The palette: every colour when there are at most `max`; otherwise the
/// `max / 4` most frequent ones exactly (background, code, rails, labels —
/// the colors that must not shift) and a median cut of the rest.
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
/// colors that are in the palette).
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

    /// A frame that changes a few rows of cells, or none, gives the same
    /// GIF when only the strips near the changed rows are rasterized again.
    /// More than 256 colors, so that the weight of every color counts for
    /// the palette: a strip or a frame counted once too few changes it.
    #[test]
    fn rasterizing_only_the_strips_that_change_gives_the_same_gif() {
        let o = GifOptions {
            png: PngOptions {
                scale: 2.0,
                system_fonts: false,
            },
            ..Default::default()
        };
        let rows = 90;
        let mut base = Canvas::new_filled(14, rows, Theme::dark().base());
        for y in 0..rows {
            let n = y as u8;
            let style = CellStyle::new()
                .fg(Rgb(255 - n, n.wrapping_mul(29), n.wrapping_mul(53)))
                .bg(Rgb(n.wrapping_mul(7), 40, n.wrapping_mul(3)));
            base.put_str(0, y, &format!("{y:02} let ab = cd;"), style);
        }
        let g = crate::svg::geometry(&base, &o.svg);
        // The row of cells that crosses the edge of the first strip.
        let edge = ((STRIP_ROWS as f32 / o.png.scale - g.margin) / g.cell_h) as usize;
        let change = |c: &Canvas, y: usize, text: &str| {
            let mut c = c.clone();
            c.put_str(3, y, text, CellStyle::new().fg(Rgb(250, 250, 90)));
            c
        };
        let f1 = change(&base, edge, "mut x");
        let f3 = change(&f1, rows - 2, "zz");
        let f4 = change(&f3, 0, "top");
        // Two changes far apart: two strips, and the rows between them out
        // of the SVG.
        let f5 = change(&change(&f4, 2, "two"), rows - 3, "two");
        let frames = vec![
            (base, 400),
            (f1.clone(), 400),
            // The same frame again: no strip is rasterized.
            (f1, 400),
            (f3, 400),
            (f4, 400),
            (f5, 400),
        ];
        let (fast, report, drawn) = encode(&frames, &Theme::dark(), &o, true).unwrap();
        let (every, _, all) = encode(&frames, &Theme::dark(), &o, false).unwrap();
        assert!(
            report.height > 2 * STRIP_ROWS,
            "{} rows: more than two strips",
            report.height
        );
        assert_eq!(report.colors, 256, "a full palette");
        assert!(
            fast == every,
            "the GIF differs when the clean strips are reused"
        );
        assert!(
            drawn.strips * 2 < all.strips && drawn.svg_bytes * 2 < all.svg_bytes,
            "{drawn:?} of {all:?}: the clean strips are reused, their rows not in the SVG"
        );
        assert!(to_gif(&frames, &Theme::dark(), &o).unwrap().0 == fast);
    }

    /// Row 0 can start right at the edge of a strip: a glyph that draws
    /// above its row then draws in the strip above, which is rasterized
    /// again too.
    #[test]
    fn a_glyph_above_row_0_at_the_edge_of_a_strip_is_rasterized_again() {
        let mut o = GifOptions {
            png: PngOptions {
                scale: 2.0,
                system_fonts: false,
            },
            ..Default::default()
        };
        o.svg.font_size = 25.0;
        o.svg.margin_cells = 15.5;
        let mut before = Canvas::new_filled(8, 3, Theme::dark().base());
        before.put_str(0, 0, "aaaa", CellStyle::new().fg(Rgb(255, 255, 255)));
        let mut now = before.clone();
        now.put_str(0, 0, TALL, CellStyle::new().fg(Rgb(255, 255, 255)));
        let g = crate::svg::geometry(&now, &o.svg);
        assert_eq!(
            g.margin * o.png.scale,
            STRIP_ROWS as f32,
            "row 0 starts a strip"
        );
        // The tall glyphs draw above row 0, in the strip above it.
        let ink = |c: &Canvas| {
            let svg = crate::svg::to_svg(c, &Theme::dark(), &o.svg);
            let pm = crate::png::rasterize(&svg, &o.png).unwrap();
            let row = (pm.width() * 4) as usize;
            pm.data()[(STRIP_ROWS as usize - 8) * row..STRIP_ROWS as usize * row].to_vec()
        };
        assert!(ink(&before) != ink(&now), "{TALL:?} draws above its row");
        let frames = vec![(before.clone(), 400), (now, 400), (before, 400)];
        let (fast, _, _) = encode(&frames, &Theme::dark(), &o, true).unwrap();
        let (every, _, _) = encode(&frames, &Theme::dark(), &o, false).unwrap();
        assert!(fast == every, "the strip above row 0 keeps old pixels");
    }

    /// Glyphs that draw above the ascent of the embedded faces.
    const TALL: &str = "ǺǼǾȀ";

    /// Stacked marks and the system fonts can draw past the margin: then a
    /// changed row rasterizes every strip again.
    #[test]
    fn stacked_marks_and_the_system_fonts_rasterize_every_strip() {
        let tall_frames = |text: &str| {
            let mut a = Canvas::new_filled(8, 120, Theme::dark().base());
            a.put_str(0, 60, "abc", CellStyle::new().fg(Rgb(255, 255, 255)));
            let mut b = a.clone();
            b.put_str(0, 60, text, CellStyle::new().fg(Rgb(255, 255, 255)));
            vec![(a, 400), (b, 400)]
        };
        let mut o = GifOptions {
            png: PngOptions {
                scale: 2.0,
                system_fonts: false,
            },
            ..Default::default()
        };
        let count = |frames: &[(Canvas, u64)], o: &GifOptions, reuse: bool| {
            encode(frames, &Theme::dark(), o, reuse).unwrap().2.strips
        };
        let plain = tall_frames("xyz");
        assert!(count(&plain, &o, true) < count(&plain, &o, false));
        // An "a" with three acute accents: four chars in one cell.
        let stacked = tall_frames("a\u{301}\u{301}\u{301}");
        assert_eq!(count(&stacked, &o, true), count(&stacked, &o, false));
        o.png.system_fonts = true;
        assert_eq!(count(&plain, &o, true), count(&plain, &o, false));
    }

    /// A row that does not change is not in the SVG of a frame when it is
    /// far from every changed row. A grapheme with stacked marks can draw
    /// two rows away: then the SVG has every row.
    #[test]
    fn stacked_marks_in_a_row_that_does_not_change_are_still_drawn() {
        let o = GifOptions {
            png: PngOptions {
                scale: 2.0,
                system_fonts: false,
            },
            ..Default::default()
        };
        let rows = 90;
        let mut base = Canvas::new_filled(14, rows, Theme::dark().base());
        for y in 0..rows {
            base.put_str(0, y, "let x = 1;", CellStyle::new().fg(Rgb(200, 200, 200)));
        }
        let g = crate::svg::geometry(&base, &o.svg);
        let row_at = |px: u32| ((px as f32 / o.png.scale - g.margin) / g.cell_h) as usize;
        // A row that changes in the middle of strip 1, and a row of marks
        // that starts one whole row below strip 1.
        let changed = row_at(STRIP_ROWS * 3 / 2);
        let marks = row_at(2 * STRIP_ROWS) + 2;
        let mut with_marks = base.clone();
        with_marks.put_str(5, marks, STACKED, CellStyle::new().fg(Rgb(255, 80, 80)));
        // Its marks draw in strip 1.
        let strip_1 = |c: &Canvas| {
            let svg = crate::svg::to_svg(c, &Theme::dark(), &o.svg);
            let pm = crate::png::rasterize(&svg, &o.png).unwrap();
            let row = (pm.width() * 4) as usize;
            pm.data()[STRIP_ROWS as usize * row..2 * STRIP_ROWS as usize * row].to_vec()
        };
        assert!(
            strip_1(&base) != strip_1(&with_marks),
            "the marks draw in strip 1"
        );
        let mut next = with_marks.clone();
        next.put_str(
            0,
            changed,
            "let y = 2;",
            CellStyle::new().fg(Rgb(250, 250, 90)),
        );
        let frames = vec![(with_marks, 400), (next, 400)];
        let (fast, _, _) = encode(&frames, &Theme::dark(), &o, true).unwrap();
        let (every, _, _) = encode(&frames, &Theme::dark(), &o, false).unwrap();
        assert!(
            fast == every,
            "strip 1 lost the marks of a row out of the SVG"
        );
    }

    /// A row of cells that crosses the edge between two strips draws in
    /// both: it is in the SVG of the strip that changes, even when its
    /// middle is in the other one.
    #[test]
    fn a_row_across_the_edge_of_a_strip_is_in_the_svg_of_both_strips() {
        let o = GifOptions {
            png: PngOptions {
                scale: 2.0,
                system_fonts: false,
            },
            ..Default::default()
        };
        let rows = 60;
        let mut base = Canvas::new_filled(14, rows, Theme::dark().base());
        for y in 0..rows {
            base.put_str(0, y, "gjpqy |{}[]", CellStyle::new().fg(Rgb(200, 220, 120)));
        }
        let g = crate::svg::geometry(&base, &o.svg);
        let top = |y: usize| (g.margin + y as f32 * g.cell_h) * o.png.scale;
        let edge = (0..rows)
            .find(|&y| top(y) < STRIP_ROWS as f32 && top(y + 1) > STRIP_ROWS as f32)
            .expect("a row across the edge");
        // Change a row on the side of the edge where the middle of the
        // crossing row is not: only that strip changes.
        let middle = (top(edge) + top(edge + 1)) / 2.0;
        let changed = if middle > STRIP_ROWS as f32 {
            edge - 2
        } else {
            edge + 2
        };
        let mut next = base.clone();
        next.put_str(0, changed, "zzzzzz", CellStyle::new().fg(Rgb(250, 90, 90)));
        let frames = vec![(base, 400), (next, 400)];
        let (fast, _, drawn) = encode(&frames, &Theme::dark(), &o, true).unwrap();
        let (every, _, all) = encode(&frames, &Theme::dark(), &o, false).unwrap();
        assert!(
            drawn.strips < all.strips,
            "{drawn:?} of {all:?}: one strip is reused"
        );
        assert!(fast == every, "the strip lost the part of the crossing row");
    }

    /// A row of cells that starts a strip can draw in the strip above it.
    /// When only the strip above changes, the row is in its SVG by the
    /// margin, although it does not change.
    #[test]
    fn a_row_that_starts_a_strip_is_in_the_svg_of_the_strip_above() {
        let mut o = GifOptions {
            png: PngOptions {
                scale: 2.0,
                system_fonts: false,
            },
            ..Default::default()
        };
        o.svg.font_size = 25.0;
        // Row 10 starts at pixel row 1024: 182 + 10 × 33 = 512, at scale 2.
        o.svg.margin_cells = 182.0 / 33.0;
        let mut base = Canvas::new_filled(8, 30, Theme::dark().base());
        for y in 0..30 {
            base.put_str(0, y, "aaaa", CellStyle::new().fg(Rgb(255, 255, 255)));
        }
        base.put_str(0, 10, TALL, CellStyle::new().fg(Rgb(255, 255, 255)));
        let g = crate::svg::geometry(&base, &o.svg);
        let top_10 = (g.margin + 10.0 * g.cell_h) * o.png.scale;
        assert!(
            (top_10 - STRIP_ROWS as f32).abs() < 0.01,
            "row 10 starts strip 1: {top_10}"
        );
        // Its glyphs draw in strip 0, above the row.
        let ink = |c: &Canvas| {
            let svg = crate::svg::to_svg(c, &Theme::dark(), &o.svg);
            let pm = crate::png::rasterize(&svg, &o.png).unwrap();
            let row = (pm.width() * 4) as usize;
            pm.data()[(STRIP_ROWS as usize - 8) * row..STRIP_ROWS as usize * row].to_vec()
        };
        let mut plain = base.clone();
        plain.put_str(0, 10, "aaaa", CellStyle::new().fg(Rgb(255, 255, 255)));
        assert!(ink(&plain) != ink(&base), "{TALL:?} draws above row 10");
        let mut next = base.clone();
        next.put_str(0, 5, "zzzz", CellStyle::new().fg(Rgb(250, 90, 90)));
        let height = crate::limits::image_size(&base, &o.svg, &o.png).unwrap().1;
        let dirty = dirty_strips(&base, &next, &o, height);
        assert!(dirty[0] && !dirty[1], "only strip 0 changes: {dirty:?}");
        let frames = vec![(base, 400), (next, 400)];
        let (fast, _, _) = encode(&frames, &Theme::dark(), &o, true).unwrap();
        let (every, _, _) = encode(&frames, &Theme::dark(), &o, false).unwrap();
        assert!(fast == every, "strip 0 lost the glyphs of row 10");
    }

    /// An "a" with 40 acute accents, stacked: one grapheme, one cell.
    const STACKED: &str = "a\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}\u{301}";

    #[test]
    fn frames_of_other_sizes_are_an_error() {
        let a = Canvas::new_filled(20, 3, Theme::dark().base());
        let b = Canvas::new_filled(20, 4, Theme::dark().base());
        let frames = vec![(a.clone(), 100), (a, 100), (b, 100)];
        let err = to_gif(&frames, &Theme::dark(), &GifOptions::default()).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("frame 2 is")
                && text.contains("every frame of a take must have the same size"),
            "{text}"
        );
    }

    #[test]
    fn the_palette_keeps_the_frequent_colors_exactly() {
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
    }

    #[test]
    fn a_frame_written_over_the_last_gives_the_rectangle_that_changed() {
        let mut s = Screen::new(3, 2);
        let mut frame = |runs: &[(u8, usize)]| {
            s.start();
            for &(index, n) in runs {
                s.write(index, n);
            }
            (s.changed(), s.idx.clone())
        };
        assert_eq!(frame(&[(0, 6)]), (None, vec![0; 6]));
        // A run past the end of a row: x 2 of the top row and x 0 of the
        // next one, so the rectangle is the whole width.
        assert_eq!(
            frame(&[(0, 2), (2, 2), (0, 2)]),
            (Some((0, 0, 3, 2)), vec![0, 0, 2, 2, 0, 0])
        );
        // The same frame again: nothing changed.
        assert_eq!(frame(&[(0, 2), (2, 2), (0, 2)]).0, None);
        // One pixel in the second row: the top is that row.
        assert_eq!(
            frame(&[(0, 2), (2, 3), (0, 1)]),
            (Some((1, 1, 1, 1)), vec![0, 0, 2, 2, 2, 0])
        );
        // One run changes the whole top row.
        assert_eq!(
            frame(&[(1, 3), (2, 2), (0, 1)]),
            (Some((0, 0, 3, 1)), vec![1, 1, 1, 2, 2, 0])
        );
    }
}
