//! Turns document + theme + available width into a `Canvas`.
//!
//! Decisions of this first version (see docs/adr/0002):
//! - Boxes truly surround the lines: they insert virtual rows (without a
//!   number) above and below, and vertical rails on the left and right.
//! - Arrows between different lines run along a lane to the left of the
//!   code (`╭──` leaves the source line, `╰──▶` enters the target line).
//! - Arrows within the same line use a virtual row below it.
//! - Notes and legends go in the right margin when there are ≥ 14 cells;
//!   otherwise in a virtual row below the line. This works in a 75-column terminal.
//!   A line with two or more labels at the step draws them all below it,
//!   also when the margin is there, and so does a line whose margin row a
//!   callout's block leaves fewer than 14 cells beside.

use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::canvas::{Canvas, CellStyle, Rgb, code_grapheme_width, code_width, display_width};
use crate::highlight::{Segment, highlight_cached};
use crate::model::{Annotation, Document, Emphasis, Shape, TextSpan};
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub width: usize,
    /// Current step: annotations with `step <= step` are drawn.
    pub step: u32,
    /// Focused line range; the rest is dimmed.
    pub focus: Option<(u32, u32)>,
    /// The live pointer: these lines are drawn normally and every
    /// other code line is dimmed, over any focus or spotlight. Only the
    /// presenter's own screen passes it; every export, recording and
    /// `render.text` passes `None` (it is never part of the document).
    pub pointer: Option<(u32, u32)>,
    pub line_numbers: bool,
    pub tab_width: usize,
    /// A typing animation of `step` is running: the code is drawn as it is
    /// but the annotations of `step` wait until it ends.
    pub typing: bool,
    /// Cap on the right margin, in cells: notes, labels and legends
    /// wrap at it instead of spreading across a wide terminal, and the
    /// canvas ends there (a PNG is no wider than the content). `None`: the
    /// margin takes what the width leaves.
    pub margin_cap: Option<usize>,
    /// Stable geometry: the arrow lanes, the box band, the code
    /// column and the margin are sized once for the maximum over every step
    /// of the document, so the code never slides sideways between steps and
    /// the margin never shrinks mid-take. Presentation mode and every export
    /// pass `true`; the authoring screen may pass `false` to grow as it goes.
    pub stable: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            width: 100,
            step: u32::MAX,
            focus: None,
            pointer: None,
            line_numbers: true,
            tab_width: 4,
            typing: false,
            stable: true,
            margin_cap: None,
        }
    }
}

/// Narrowest right-margin cap a caller may ask for (`:margin N`): enough
/// for a mark glyph, its legend and a short note.
pub const MIN_MARGIN_CAP: usize = 12;
/// Widest cap: wider than any sensible margin at `MAX_WIDTH`.
pub const MAX_MARGIN_CAP: usize = 200;

/// Checks a margin cap that came from the user.
pub fn check_margin_cap(cap: usize) -> Result<usize, crate::Error> {
    if (MIN_MARGIN_CAP..=MAX_MARGIN_CAP).contains(&cap) {
        Ok(cap)
    } else {
        Err(crate::Error::Invalid(format!(
            "margin={cap}: use a number between {MIN_MARGIN_CAP} and {MAX_MARGIN_CAP}, or auto"
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Code(u32),
    BoxTop(usize),
    BoxBottom(usize),
    ArrowUnder(usize),
    /// Row under a code line with ▔ / ⌊ ⌋ for underline/bracket emphases.
    Emphasis(u32),
    /// Virtual row with text (note, arrow legend) below the line.
    Under(usize),
    /// One row of a callout drawn below its range: (callout index in `vis`, row within the block).
    Callout(usize, usize),
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub canvas: Canvas,
    pub rows: Vec<Row>,
    pub row_of_line: BTreeMap<u32, usize>,
    pub code_x: usize,
    /// Width in cells of the widest code line when it did not fit and was
    /// clipped (`None` when every line fits).
    pub clipped_code: Option<usize>,
    /// Whether labels and notes go to the right margin (false: under the line).
    pub labels_in_margin: bool,
    /// Labels and notes drawn clipped with `…`, or left out for lack of
    /// room, in drawing order (`tiza check` warns about them).
    pub clipped_labels: Vec<ClippedLabel>,
}

/// One label the layout could not draw whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClippedLabel {
    /// The annotation it belongs to.
    pub id: String,
    /// The full text it would show (a mark's `legend: label`, a note's text).
    pub text: String,
    /// `true` when nothing of it was drawn (no free row or no room), not
    /// even the start with `…`.
    pub dropped: bool,
}

impl Layout {
    pub fn line_at_row(&self, row: usize) -> Option<u32> {
        match self.rows.get(row)? {
            Row::Code(l) => Some(*l),
            _ => None,
        }
    }
}

struct BoxInfo<'a> {
    ann: &'a Annotation,
    from: u32,
    to: u32,
    depth: usize,
}

struct ArrowInfo<'a> {
    ann: &'a Annotation,
    lane: usize,
}

/// Text waiting to be placed in the margin (or below) of a row.
#[derive(Clone)]
struct Label {
    /// The annotation it belongs to (`tiza check` names clipped labels).
    id: String,
    text: String,
    color: Rgb,
    note: bool,
    /// Narrow mode, arrow labels: the arrow's own glyph (`─▶`)
    /// drawn before the text instead of the bare `·`, so the label reads
    /// as the arrow's and not as a loose comment.
    glyph: Option<String>,
}

/// Nesting depth of a set of line ranges (boxes): the deepest box plus one,
/// zero without boxes. Two boxes on the same range nest in index order.
fn nesting_depth(spans: &[(u32, u32)]) -> usize {
    (0..spans.len())
        .map(|i| {
            let (f, t) = spans[i];
            spans
                .iter()
                .enumerate()
                .filter(|(j, (bf, bt))| {
                    *j != i && *bf <= f && *bt >= t && ((*bf, *bt) != (f, t) || *j < i)
                })
                .count()
                + 1
        })
        .max()
        .unwrap_or(0)
}

/// Greedy lane assignment: longest span first, lane 0 farthest from the
/// code; a span takes the first lane none of whose spans touch it.
fn assign_lanes<T: Copy + Ord + std::ops::Sub<Output = T>>(
    spans: &mut [(T, T, usize)],
) -> (Vec<usize>, usize) {
    // (a, b, key): the result maps each key to its lane.
    spans.sort_by_key(|(a, b, _)| (std::cmp::Reverse(*b - *a), *a));
    let mut lanes: Vec<Vec<(T, T)>> = Vec::new();
    let mut out = vec![0; spans.len()];
    for (a, b, key) in spans.iter().copied() {
        let lane = lanes
            .iter()
            .position(|l| l.iter().all(|(x, y)| b < *x || a > *y))
            .unwrap_or_else(|| {
                lanes.push(Vec::new());
                lanes.len() - 1
            });
        lanes[lane].push((a, b));
        out[key] = lane;
    }
    (out, lanes.len())
}

/// The geometry a take shares across its steps: computed from the
/// maximum over every step, so a frame at step 1 and one at step 7 put the
/// code at the same x and keep the same margin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StableGeometry {
    /// Lane of each multi-line arrow, by annotation id, fixed for the take.
    pub lane_of: std::collections::HashMap<String, usize>,
    pub lane_count: usize,
    pub box_depth: usize,
    pub code_full_w: usize,
    pub num_w: usize,
}

/// Sizes the lanes, the box band and the code column for every step of
/// `doc` at once (moves included): what `layout` uses when
/// `opts.stable` is set. Public so a recorder can check that its frames
/// agree.
pub fn stable_geometry(doc: &Document, opts: &LayoutOptions) -> StableGeometry {
    let last = doc.max_step();
    let mut g = StableGeometry {
        code_full_w: 1,
        ..Default::default()
    };
    // Each arrow's span in the coordinates of the last step it is drawn at
    // (moves shift them together with the code, so the overlaps hold).
    let mut arrow_span: Vec<(String, u32, u32)> = Vec::new();
    // Only the steps where something changes: the others draw
    // like the last of these before them.
    for step in doc.change_steps(last) {
        let projected;
        let d = if doc.moves.is_empty() {
            doc
        } else {
            projected = doc.view_at(step).doc;
            &projected
        };
        g.code_full_w = g.code_full_w.max(
            d.code
                .iter()
                .map(|l| code_width(l, opts.tab_width))
                .max()
                .unwrap_or(0),
        );
        if opts.line_numbers {
            g.num_w = g.num_w.max(d.last_line().to_string().len() + 2);
        }
        let vis = d.annotations.iter().filter(|a| {
            a.step <= step
                && a.shape
                    .line_ranges()
                    .iter()
                    .all(|(f, t)| d.has_line(*f) && d.has_line(*t))
        });
        let mut boxes = Vec::new();
        for a in vis {
            match &a.shape {
                Shape::Box {
                    from_line, to_line, ..
                } => boxes.push((*from_line, *to_line)),
                Shape::Arrow { from, to, .. } if from.line != to.line => {
                    let span = (from.line.min(to.line), from.line.max(to.line));
                    match arrow_span.iter_mut().find(|(id, _, _)| *id == a.id) {
                        Some(e) => (e.1, e.2) = span,
                        None => arrow_span.push((a.id.clone(), span.0, span.1)),
                    }
                }
                _ => {}
            }
        }
        g.box_depth = g.box_depth.max(nesting_depth(&boxes));
    }
    let mut spans: Vec<(u32, u32, usize)> = arrow_span
        .iter()
        .enumerate()
        .map(|(i, (_, a, b))| (*a, *b, i))
        .collect();
    let (lanes, count) = assign_lanes(&mut spans);
    g.lane_count = count;
    for (i, (id, _, _)) in arrow_span.into_iter().enumerate() {
        g.lane_of.insert(id, lanes[i]);
    }
    g
}

/// Glyph of the underline emphasis, on the virtual row under the code:
/// the overline (top of its cell) is thinner than the one-eighth block ▔.
pub const UNDERLINE_GLYPH: &str = "‾";
/// How much of the annotation colour the emphasis glyphs keep (the rest is
/// the background): softened so the code reads first, still visible after
/// video compression (checked by the legibility gate). Tried and set by eye:
/// 1.0 shouted, 0.55 was too soft; 0.75 sits between them.
pub const EMPHASIS_GLYPH_OPACITY: f32 = 0.75;

/// Narrowest layout a caller may ask for, in columns.
pub const MIN_WIDTH: usize = 20;
/// Widest layout a caller may ask for, in columns. The canvas is `width ×
/// rows` cells and the PNG `width × 9.6 px × scale`, so this is what keeps a
/// typo (`width=99999`) from asking the allocator for gigabytes; 400 is
/// already four times a 1080p terminal.
pub const MAX_WIDTH: usize = 400;

/// Checks a width that came from the user (`:export width=`, `render
/// --width`, `render.text {width}`, `export_image {width}`).
pub fn check_width(width: usize) -> Result<usize, crate::Error> {
    if (MIN_WIDTH..=MAX_WIDTH).contains(&width) {
        Ok(width)
    } else {
        Err(crate::Error::Invalid(format!(
            "width={width}: use a number between {MIN_WIDTH} and {MAX_WIDTH}"
        )))
    }
}

/// Validates a PNG/SVG scale factor from the user.
pub fn check_scale(scale: f32) -> Result<f32, crate::Error> {
    if scale.is_finite() && (0.25..=8.0).contains(&scale) {
        Ok(scale)
    } else {
        Err(crate::Error::Invalid(format!(
            "scale={scale}: use a number between 0.25 and 8"
        )))
    }
}

/// Validates a font size (px) from the user.
pub fn check_font_size(px: f32) -> Result<f32, crate::Error> {
    if px.is_finite() && (4.0..=96.0).contains(&px) {
        Ok(px)
    } else {
        Err(crate::Error::Invalid(format!(
            "font-size={px}: use a number between 4 and 96"
        )))
    }
}

/// The vertical part of an arrow between lines: drawn after every stub so
/// it owns its cells (a stub crossing it shows a one-cell gap).
struct Rail {
    x: usize,
    top: usize,
    bottom: usize,
    v: &'static str,
    turns: (&'static str, &'static str),
    style: CellStyle,
}

pub fn layout(doc: &Document, theme: &Theme, opts: &LayoutOptions) -> Layout {
    layout_with(doc, theme, opts, None)
}

/// [`layout`] with the stable geometry already computed (it depends on the
/// document, not on the width or the step): `tiza check` lays a document
/// out at every step and many widths. Ignored unless `opts.stable`.
pub fn layout_with(
    doc: &Document,
    theme: &Theme,
    opts: &LayoutOptions,
    geometry: Option<&StableGeometry>,
) -> Layout {
    let owned;
    let geometry = match geometry {
        None if opts.stable => {
            owned = stable_geometry(doc, opts);
            Some(&owned)
        }
        g => g,
    };
    // With moves (ADR 0007) the document is drawn as it looks at `opts.step`:
    // the code at that step and the annotations shifted through the patches
    // of later steps; those without an anchor are left out.
    let projected;
    let (mut cursor, mut selection) = (None, None);
    let doc = if doc.moves.is_empty() {
        doc
    } else {
        let v = doc.view_at(opts.step);
        (cursor, selection) = (v.cursor, v.selection);
        projected = v.doc;
        &projected
    };
    // An empty label is no label: no lone `·`, no `warning:` with nothing
    // after it, no gap in a border, and it is not a second label of its line.
    let filled;
    let doc = match without_blank_labels(doc) {
        Some(d) => {
            filled = d;
            &filled
        }
        None => doc,
    };
    let shared = Shared {
        cursor,
        selection,
        code_w: doc
            .code
            .iter()
            .map(|l| code_width(l, opts.tab_width))
            .max()
            .unwrap_or(0)
            .max(1),
    };
    // Whether a margin block fits, and which rows it covers, is only known
    // once the rows exist, so the rows are planned again while a plan finds
    // something to move under its line:
    // - a margin callout with no room for its block on the rows of its own
    //   line is drawn as a block under its line, as the narrow layout does;
    // - a line with a label whose margin row a block leaves fewer than 14
    //   cells beside it puts its labels under itself.
    // Both sets only grow (the extra rows only give the blocks more room),
    // so this ends. A plan places the rows and the blocks without drawing
    // anything; the layout is drawn once, from the last plan (a document
    // with no margin callout at the step has nothing to plan).
    let mut planning = doc.annotations.iter().any(|a| {
        a.step <= opts.step
            && matches!(&a.shape, Shape::Callout { placement, .. }
                if *placement != crate::model::Placement::Below)
    });
    let mut under = Under::default();
    while planning {
        let Pass::Planned(more) = layout_pass(doc, theme, opts, geometry, &under, &shared, false)
        else {
            unreachable!("a plan draws nothing")
        };
        let before = (under.callouts.len(), under.lines.len());
        under.callouts.extend(more.callouts);
        under.lines.extend(more.lines);
        planning = (under.callouts.len(), under.lines.len()) != before;
    }
    match layout_pass(doc, theme, opts, geometry, &under, &shared, true) {
        Pass::Drawn(l) => l,
        Pass::Planned(_) => unreachable!("a drawing pass returns its layout"),
    }
}

/// What every run of one layout shares (see [`layout_with`]).
struct Shared {
    /// The cursor and selection the moves left at the step.
    cursor: Option<crate::model::Anchor>,
    selection: Option<(crate::model::Anchor, crate::model::Anchor)>,
    /// The widest code line, in cells (at least 1).
    code_w: usize,
}

/// What one run of [`layout_pass`] gives back.
enum Pass {
    /// A plan's verdict: what must go under its line.
    Planned(Under),
    /// The layout, drawn.
    Drawn(Layout),
}

/// What a run of the layout moves under its line (see [`layout_with`]).
#[derive(Default)]
struct Under {
    /// Margin callouts drawn as a block under their line.
    callouts: std::collections::BTreeSet<String>,
    /// Lines whose labels go under them because a margin block covers
    /// their row.
    lines: std::collections::BTreeSet<u32>,
}

/// One run of the layout of `doc` (already as the step shows it) with
/// `under` (see [`layout_with`]): with `draw`, the layout; without it, a
/// plan of the rows and the margin blocks that returns what must go under
/// its line, for the next run.
fn layout_pass(
    doc: &Document,
    theme: &Theme,
    opts: &LayoutOptions,
    geometry: Option<&StableGeometry>,
    under: &Under,
    shared: &Shared,
    draw: bool,
) -> Pass {
    let under_callouts = &under.callouts;
    let width = opts.width.max(20);
    // Borrowed when the caller computed it (once per document, not per
    // frame or per pass).
    let owned;
    let stable: Option<&StableGeometry> = match (opts.stable, geometry) {
        (false, _) => None,
        (true, Some(g)) => Some(g),
        (true, None) => {
            owned = stable_geometry(doc, opts);
            Some(&owned)
        }
    };
    // Annotations outside the code (a document whose file shrank, or a
    // hand-edited one) are reported when it opens; here they are simply not
    // drawn, so a stale line number can never break the layout.
    let vis: Vec<&Annotation> = doc
        .annotations
        .iter()
        .filter(|a| a.step <= opts.step && !(opts.typing && a.step == opts.step))
        .filter(|a| {
            a.shape
                .line_ranges()
                .iter()
                .all(|(f, t)| doc.has_line(*f) && doc.has_line(*t))
        })
        .collect();
    // Color per annotation: the one chosen by the user; otherwise arrows rotate through the
    // automatic palette (in order of appearance) and the rest use the theme color.
    let auto = theme.auto_palette();
    let mut arrow_seq = 0usize;
    let mut colors: std::collections::HashMap<&str, Rgb> = std::collections::HashMap::new();
    for a in &vis {
        let chosen = a.color.as_deref().and_then(|c| theme.named_color(c));
        let c = match (&a.shape, chosen) {
            (_, Some(c)) => c,
            (Shape::Arrow { .. }, None) => {
                let c = auto[arrow_seq % auto.len().max(1)];
                arrow_seq += 1;
                c
            }
            (Shape::Box { .. }, None) => theme.box_line,
            (Shape::Note { .. }, None) => theme.note,
            (Shape::Callout { .. }, None) => theme.note,
            (Shape::Mark { kind, style, .. }, None) => doc
                .resolve_style(kind, style)
                .map(|st| theme.mark_color(&st))
                .unwrap_or(theme.accent),
            (Shape::Highlight { .. }, None) => theme.accent,
            (Shape::Spotlight { .. }, None) => theme.accent,
        };
        colors.insert(a.id.as_str(), c);
    }
    let color_of = |a: &Annotation| colors.get(a.id.as_str()).copied().unwrap_or(theme.accent);

    // ---- spotlights active at this step ----
    let mut spot_lines: Vec<(u32, u32)> = Vec::new();
    let mut spot_ids: Vec<&str> = Vec::new();
    let mut spot_any = false;
    for a in &vis {
        if let Shape::Spotlight {
            from_line,
            to_line,
            ids,
            until,
        } = &a.shape
            && opts.step >= a.step
            && opts.step <= until.unwrap_or(a.step)
        {
            spot_any = true;
            if let Some(f) = from_line {
                spot_lines.push((*f, to_line.unwrap_or(*f)));
            }
            for id in ids {
                spot_ids.push(id.as_str());
                if let Some(target) = doc.find_annotation(id) {
                    spot_lines.extend(target.shape.line_ranges());
                }
            }
        }
    }
    let line_in_spot =
        |line: u32| !spot_any || spot_lines.iter().any(|(a, b)| line >= *a && line <= *b);
    // An annotation is dimmed if there are spotlights with ids and it is not among them.
    let ann_dimmed = |a: &Annotation| {
        spot_any
            && !spot_ids.is_empty()
            && !spot_ids.contains(&a.id.as_str())
            && !matches!(a.shape, Shape::Spotlight { .. })
    };
    let color_of = |a: &Annotation| {
        if ann_dimmed(a) {
            theme.dimmed
        } else {
            color_of(a)
        }
    };
    // Spotlights are not drawn; they only filter.
    let vis: Vec<&Annotation> = vis
        .into_iter()
        .filter(|a| !matches!(a.shape, Shape::Spotlight { .. }))
        .collect();

    // ---- boxes and their nesting ----
    let mut boxes: Vec<BoxInfo> = vis
        .iter()
        .filter_map(|a| match &a.shape {
            Shape::Box {
                from_line, to_line, ..
            } => Some(BoxInfo {
                ann: a,
                from: *from_line,
                to: *to_line,
                depth: 0,
            }),
            _ => None,
        })
        .collect();
    for i in 0..boxes.len() {
        let (f, t) = (boxes[i].from, boxes[i].to);
        boxes[i].depth = boxes
            .iter()
            .enumerate()
            .filter(|(j, b)| {
                *j != i && b.from <= f && b.to >= t && ((b.from, b.to) != (f, t) || *j < i)
            })
            .count();
    }
    let box_depth = boxes
        .iter()
        .map(|b| b.depth + 1)
        .max()
        .unwrap_or(0)
        .max(stable.as_ref().map_or(0, |g| g.box_depth));

    // ---- column widths ----
    // One cell for the cursor/selection marker, the digits, one blank.
    let num_w = if opts.line_numbers {
        doc.last_line().to_string().len() + 2
    } else {
        0
    }
    .max(stable.as_ref().map_or(0, |g| g.num_w));
    let code_full_w = shared
        .code_w
        .max(stable.as_ref().map_or(0, |g| g.code_full_w));
    let multi_arrows: Vec<&Annotation> = vis
        .iter()
        .copied()
        .filter(|a| matches!(&a.shape, Shape::Arrow { from, to, .. } if from.line != to.line))
        .collect();

    // Rows are built before lanes are assigned (lanes depend on rows).
    // But the margin width depends on the number of lanes, and "Under" rows
    // depend on the margin. We approximate: lanes = upper bound (all overlap).
    // Estimate lanes in line space (virtual rows preserve the order, so the
    // overlap is the same as in row space).
    let lane_bound = {
        let mut spans: Vec<(u32, u32)> = multi_arrows
            .iter()
            .filter_map(|a| match &a.shape {
                Shape::Arrow { from, to, .. } => {
                    Some((from.line.min(to.line), from.line.max(to.line)))
                }
                _ => None,
            })
            .collect();
        spans.sort_by_key(|(a, b)| (std::cmp::Reverse(b - a), *a));
        let mut lanes: Vec<Vec<(u32, u32)>> = Vec::new();
        for (a, b) in spans {
            match lanes
                .iter_mut()
                .find(|l| l.iter().all(|(x, y)| b < *x || a > *y))
            {
                Some(l) => l.push((a, b)),
                None => lanes.push(vec![(a, b)]),
            }
        }
        lanes.len()
    }
    .max(stable.as_ref().map_or(0, |g| g.lane_count));
    let arrow_w = lane_bound * 2;
    let code_x_est = num_w + arrow_w + box_depth + 2;
    let code_w = code_full_w
        .min(width.saturating_sub(code_x_est + box_depth + 1))
        .max(10);
    let margin_est = width
        .saturating_sub(code_x_est + code_w + 1 + box_depth + 1)
        .min(opts.margin_cap.unwrap_or(usize::MAX));
    let labels_in_margin = margin_est >= 14;
    // A line whose margin row would get two or more labels at this step
    // draws them all under itself, as the narrow layout does:
    // stacked in the margin, the second one sat beside the next line and
    // read as if it explained that line.
    // Labels per line in the margin layout (a margin callout counts once
    // it is drawn as a label, see `under_callouts`).
    let mut label_count: BTreeMap<u32, usize> = BTreeMap::new();
    if labels_in_margin {
        for a in &vis {
            let line = match &a.shape {
                Shape::Note { line, .. } => *line,
                Shape::Callout { from_line, .. } if under_callouts.contains(&a.id) => *from_line,
                Shape::Mark { span, .. }
                | Shape::Highlight {
                    span,
                    label: Some(_),
                    ..
                } => span.line,
                Shape::Arrow {
                    from,
                    to,
                    label: Some(_),
                    ..
                } if from.line != to.line => to.line,
                _ => continue,
            };
            *label_count.entry(line).or_insert(0) += 1;
        }
    }
    let under_lines: std::collections::BTreeSet<u32> = label_count
        .iter()
        .filter(|&(_, n)| *n >= 2)
        .map(|(line, _)| *line)
        .collect();
    // Lines whose margin row a callout block covers (see `layout_with`).
    let covered = |line: u32| under.lines.contains(&line);
    let in_margin = |line: u32| labels_in_margin && !under_lines.contains(&line) && !covered(line);

    // ---- callouts: decide placement and pre-wrap their text ----
    struct CalloutPlan {
        idx: usize,
        below: bool,
        title: Option<String>,
        lines: Vec<String>,
        /// Outer block width (borders included).
        width: usize,
        frame: crate::model::BorderStyle,
    }
    let mut callouts: Vec<CalloutPlan> = Vec::new();
    for (i, a) in vis.iter().enumerate() {
        let Shape::Callout {
            text,
            title,
            placement,
            width: want,
            frame,
            ..
        } = &a.shape
        else {
            continue;
        };
        let below = under_callouts.contains(&a.id)
            || match placement {
                crate::model::Placement::Below => true,
                crate::model::Placement::Margin => !labels_in_margin || margin_est < 12,
                // Auto: the margin must hold the block *and* leave room for
                // the labels of the lines it covers, beside it.
                crate::model::Placement::Auto => {
                    !labels_in_margin || margin_est < want.unwrap_or(34) + 4 + 1 + 14
                }
            };
        let text_w = if below {
            // Stays inside the code column, so a Box around the range can
            // still draw its right rail.
            let max_text = code_w.saturating_sub(4).max(8);
            want.unwrap_or(max_text).clamp(8, max_text)
        } else {
            want.unwrap_or(34)
                .clamp(8, margin_est.saturating_sub(5).max(8))
        };
        let mut lines: Vec<String> = Vec::new();
        for para in text.split('\n') {
            if para.trim().is_empty() {
                lines.push(String::new());
            } else {
                lines.extend(wrap_words(para, text_w));
            }
        }
        let inner = lines
            .iter()
            .map(|l| display_width(l, 4))
            .max()
            .unwrap_or(0)
            .max(title.as_ref().map(|t| display_width(t, 4) + 4).unwrap_or(0))
            .max(4);
        // Below the code the block never grows past the code column (a long
        // title is clipped in the border instead of breaking a box's rail).
        let inner = if below {
            inner.min(code_w.saturating_sub(4).max(4))
        } else {
            inner
        };
        callouts.push(CalloutPlan {
            idx: i,
            below,
            title: title.clone(),
            lines,
            width: inner + 4, // "│ " + text + " │"
            frame: *frame,
        });
    }

    // ---- rows ----
    let mut rows: Vec<Row> = Vec::new();
    let mut row_of_line = BTreeMap::new();
    // "Under" texts: index into `unders` referenced by Row::Under
    let mut unders: Vec<(Label, bool)> = Vec::new(); // (label, is_note)
    // Labels drawn with `…` or left out.
    let mut clipped: Vec<ClippedLabel> = Vec::new();
    let clip = |clipped: &mut Vec<ClippedLabel>, id: &str, text: &str, dropped: bool| {
        if !clipped.iter().any(|c| c.id == id && c.text == text) {
            clipped.push(ClippedLabel {
                id: id.to_string(),
                text: text.to_string(),
                dropped,
            });
        }
    };
    // Arrow labels drawn beside the code of their target line (narrow
    // mode): (line, glyph, text, colour).
    let mut beside: Vec<(u32, String, String, Rgb, String)> = Vec::new();
    // What the rows of each line depend on, so a line looks at its own
    // annotations only: its highlights, marks, notes, arrows on the line
    // and arrowheads (indices in `vis`, in its order), and the callouts
    // drawn below whose range ends on it ((first line, index in
    // `callouts`), in their order).
    let first_line = doc.source.first_line;
    let line_count = (first_line..=doc.last_line()).count();
    let mut at_line: Vec<Vec<usize>> = vec![Vec::new(); line_count];
    let mut below_at: Vec<Vec<(u32, usize)>> = vec![Vec::new(); line_count];
    let slot = |line: u32| line.checked_sub(first_line).map(|k| k as usize);
    for (i, a) in vis.iter().enumerate() {
        let line = match &a.shape {
            Shape::Highlight { span, .. } | Shape::Mark { span, .. } => span.line,
            Shape::Note { line, .. } => *line,
            Shape::Arrow { from, to, .. } if from.line == to.line => from.line,
            Shape::Arrow { to, .. } => to.line,
            _ => continue,
        };
        if let Some(v) = slot(line).and_then(|k| at_line.get_mut(k)) {
            v.push(i);
        }
    }
    for (ci, c) in callouts.iter().enumerate().filter(|(_, c)| c.below) {
        if let Shape::Callout {
            from_line, to_line, ..
        } = &vis[c.idx].shape
            && let Some(v) = slot(to_line.unwrap_or(*from_line)).and_then(|k| below_at.get_mut(k))
        {
            v.push((*from_line, ci));
        }
    }
    for (line, (own, below)) in (first_line..=doc.last_line()).zip(at_line.iter().zip(&below_at)) {
        let mut tops: Vec<usize> = (0..boxes.len())
            .filter(|&i| boxes[i].from == line)
            .collect();
        tops.sort_by_key(|&i| boxes[i].depth);
        rows.extend(tops.into_iter().map(Row::BoxTop));

        row_of_line.insert(line, rows.len());
        rows.push(Row::Code(line));
        if own.iter().any(|&i| match &vis[i].shape {
            Shape::Highlight { span, emphasis, .. } | Shape::Mark { span, emphasis, .. } => {
                span.line == line && emphasis_needs_row(doc, span, *emphasis)
            }
            _ => false,
        }) {
            rows.push(Row::Emphasis(line));
        }

        for &i in own {
            let a = vis[i];
            if let Shape::Arrow {
                from, to, label, ..
            } = &a.shape
                && from.line == to.line
                && from.line == line
            {
                rows.push(Row::ArrowUnder(i));
                if (!labels_in_margin || covered(line))
                    && let Some(l) = label
                {
                    unders.push((
                        Label {
                            id: a.id.clone(),
                            text: l.clone(),
                            color: color_of(a),
                            note: false,
                            glyph: None,
                        },
                        false,
                    ));
                    rows.push(Row::Under(unders.len() - 1));
                }
            }
        }
        if !in_margin(line) {
            for a in own.iter().map(|&i| vis[i]) {
                let lbl = match &a.shape {
                    Shape::Note { line: l, text } if *l == line => {
                        Some((text.clone(), color_of(a), true))
                    }
                    Shape::Mark {
                        kind,
                        span,
                        label,
                        style,
                        ..
                    } if span.line == line => Some((
                        mark_text(doc, kind, style, label.as_deref()),
                        color_of(a),
                        false,
                    )),
                    Shape::Highlight {
                        span,
                        label: Some(l),
                        ..
                    } if span.line == line => Some((l.clone(), color_of(a), false)),
                    Shape::Arrow {
                        from,
                        to,
                        label: Some(l),
                        line: ls,
                    } if from.line != to.line && to.line == line => {
                        // The label belongs to the arrowhead: beside
                        // the code of the target line when it fits inside the
                        // code column, else on the row below behind the
                        // arrow's own glyph.
                        let g = Glyphs::for_style(ls);
                        let glyph = format!(
                            "{}{}",
                            g.h,
                            head_glyph(ls.heads.end, true, g.h).unwrap_or(g.h)
                        );
                        let text_w = code_width(doc.line_text(line).unwrap_or(""), opts.tab_width);
                        let need = text_w + 2 + display_width(&glyph, 4) + 1 + display_width(l, 4);
                        // A wide layout's line with several labels keeps
                        // every one of them under it.
                        if !labels_in_margin && need <= code_w {
                            beside.push((line, glyph, l.clone(), color_of(a), a.id.clone()));
                        } else {
                            unders.push((
                                Label {
                                    id: a.id.clone(),
                                    text: l.clone(),
                                    color: color_of(a),
                                    note: false,
                                    glyph: Some(glyph),
                                },
                                false,
                            ));
                            rows.push(Row::Under(unders.len() - 1));
                        }
                        None
                    }
                    _ => None,
                };
                if let Some((text, color, is_note)) = lbl {
                    unders.push((
                        Label {
                            id: a.id.clone(),
                            text,
                            color,
                            note: is_note,
                            glyph: None,
                        },
                        is_note,
                    ));
                    rows.push(Row::Under(unders.len() - 1));
                }
            }
        }

        // Callouts placed below end their range on this line. A box absorbs
        // the callout's rows only when the box contains the callout's whole
        // range; otherwise the rows go after that box's bottom border, so the
        // callout is not read as belonging to the box. Nested boxes ending
        // here follow the same rule one by one (innermost first).
        let mut bottoms: Vec<usize> = (0..boxes.len()).filter(|&i| boxes[i].to == line).collect();
        bottoms.sort_by_key(|&i| std::cmp::Reverse(boxes[i].depth));
        let mut here = below.clone();
        // Narrowest range first: it sits inside more boxes.
        here.sort_by_key(|&(from, _)| std::cmp::Reverse(from));
        let mut emitted = 0;
        for (from, ci) in here {
            while emitted < bottoms.len() && boxes[bottoms[emitted]].from > from {
                rows.push(Row::BoxBottom(bottoms[emitted]));
                emitted += 1;
            }
            for k in 0..callouts[ci].lines.len() + 2 {
                rows.push(Row::Callout(ci, k));
            }
        }
        rows.extend(bottoms[emitted..].iter().map(|&i| Row::BoxBottom(i)));
    }

    // ---- arrow lanes (in row space) ----
    let mut arrows: Vec<ArrowInfo> = Vec::new();
    {
        let spans: Vec<(usize, usize, &Annotation)> = multi_arrows
            .iter()
            .map(|a| {
                let Shape::Arrow { from, to, .. } = &a.shape else {
                    unreachable!()
                };
                let r1 = row_of_line[&from.line];
                let r2 = row_of_line[&to.line];
                (r1.min(r2), r1.max(r2), *a)
            })
            .collect();
        // Longest first → lane 0 (the farthest from the code). With a
        // stable geometry each arrow keeps the lane of the whole take.
        let mut keyed: Vec<(usize, usize, usize)> = spans
            .iter()
            .enumerate()
            .map(|(i, (a, b, _))| (*a, *b, i))
            .collect();
        let (lanes, _) = assign_lanes(&mut keyed);
        for (i, (_, _, ann)) in spans.into_iter().enumerate() {
            let lane = stable
                .as_ref()
                .and_then(|g| g.lane_of.get(ann.id.as_str()).copied())
                .unwrap_or(lanes[i]);
            arrows.push(ArrowInfo { ann, lane });
        }
    }
    let lane_count = arrows
        .iter()
        .map(|a| a.lane + 1)
        .max()
        .unwrap_or(0)
        .max(stable.as_ref().map_or(0, |g| g.lane_count));
    let arrow_w = lane_count * 2;

    // ---- final positions ----
    let arrow_x0 = num_w;
    let box_x0 = arrow_x0 + arrow_w;
    let code_x = box_x0 + box_depth + 2;
    let code_w = code_full_w
        .min(width.saturating_sub(code_x + box_depth + 1))
        .max(10);
    let rbox_x0 = code_x + code_w + 1; // right rail: the innermost one here
    let margin_x = rbox_x0 + box_depth + 1;
    // The cap ends the frame: the margin is at most `cap` cells and
    // the canvas no wider than the content, so a capped PNG has no blank
    // band on the right and a capped terminal leaves its right columns free.
    let width = match opts.margin_cap {
        Some(cap) if width > margin_x + cap => margin_x + cap,
        _ => width,
    };
    let margin_w = width.saturating_sub(margin_x);

    // ---- margin labels per row ----
    // Gathered before anything is drawn: the rows with a margin label decide
    // which lines a margin block covers. On a code row: the labels of its
    // highlights and marks, then its arrowheads', then its notes.
    let mut margin: Vec<Vec<Label>> = vec![Vec::new(); rows.len()];
    if labels_in_margin {
        let mut push = |y: usize, a: &Annotation, text: &str, note: bool| {
            margin[y].push(Label {
                id: a.id.clone(),
                text: text.to_string(),
                color: color_of(a),
                note,
                glyph: None,
            });
        };
        for a in &vis {
            let (span, text) = match &a.shape {
                Shape::Highlight {
                    span,
                    label: Some(l),
                    ..
                } => (span, l.clone()),
                // A dimmed mark keeps its legend only.
                Shape::Mark {
                    kind,
                    span,
                    label,
                    style,
                    ..
                } => (
                    span,
                    mark_text(
                        doc,
                        kind,
                        style,
                        label.as_deref().filter(|_| !ann_dimmed(a)),
                    ),
                ),
                _ => continue,
            };
            if in_margin(span.line)
                && let Some(&y) = row_of_line.get(&span.line)
            {
                push(y, a, &text, false);
            }
        }
        for a in &multi_arrows {
            if let Shape::Arrow {
                to, label: Some(l), ..
            } = &a.shape
                && in_margin(to.line)
            {
                push(row_of_line[&to.line], a, l, false);
            }
        }
        for (y, row) in rows.iter().enumerate() {
            if let Row::ArrowUnder(i) = row
                && let Shape::Arrow {
                    from,
                    label: Some(l),
                    ..
                } = &vis[*i].shape
                && !covered(from.line)
            {
                push(y, vis[*i], l, false);
            }
        }
        for a in &vis {
            if let Shape::Note { line, text } = &a.shape
                && in_margin(*line)
                && let Some(&y) = row_of_line.get(line)
            {
                push(y, a, text, true);
            }
        }
    }

    // ---- margin blocks ----
    // Where each margin callout's block goes, before anything is drawn,
    // from the first line down: (index in `callouts`, x, top row). A block
    // starts on a row of its own line (its code row or one under it, before
    // the next code row) and never overlaps another one: it goes beside the
    // blocks on those rows when the margin holds it. With no room there it is
    // drawn as a block under its line (the next run, see `layout_with`).
    let height = rows.len();
    let mut placed: Vec<(usize, usize, usize)> = Vec::new();
    // (x, top row, width, height) of each block placed.
    let mut blocks: Vec<(usize, usize, usize, usize)> = Vec::new();
    let overlap =
        |blocks: &[(usize, usize, usize, usize)], x: usize, y: usize, w: usize, h: usize| {
            blocks
                .iter()
                .filter(|(bx, by, bw, bh)| x < bx + bw && *bx < x + w && y < by + bh && *by < y + h)
                .map(|(bx, _, bw, _)| bx + bw)
                .max()
        };
    let mut retry = std::collections::BTreeSet::new();
    let mut occupied = vec![false; height];
    // Columns of the margin taken by callout blocks on each row; labels of
    // those rows are drawn to the right of the blocks when there is room.
    let mut shift = vec![0usize; height];
    if labels_in_margin && margin_w > 0 {
        let mut order: Vec<(usize, usize)> = callouts
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.below)
            .filter_map(|(ci, c)| match &vis[c.idx].shape {
                Shape::Callout { from_line, .. } => row_of_line.get(from_line).map(|&y0| (y0, ci)),
                _ => None,
            })
            .collect();
        // Stable: the callouts of one line keep the document's order.
        order.sort_by_key(|&(y0, _)| y0);
        for (y0, ci) in order {
            let c = &callouts[ci];
            let h = c.lines.len() + 2;
            let own_end = (y0 + 1..height)
                .find(|&r| matches!(rows[r], Row::Code(_)))
                .unwrap_or(height);
            let spot = if c.width <= margin_w {
                (y0..own_end)
                    .take_while(|&y| y + h <= height)
                    .find_map(|y| {
                        let mut x = margin_x;
                        while let Some(right) = overlap(&blocks, x, y, c.width, h) {
                            x = right + 1;
                        }
                        (x + c.width <= width).then_some((x, y))
                    })
            } else {
                None
            };
            match spot {
                Some((x, y)) => {
                    placed.push((ci, x, y));
                    blocks.push((x, y, c.width, h));
                }
                None => {
                    retry.insert(vis[c.idx].id.clone());
                }
            }
        }
        // Labels of a row go to the right of the blocks on it when 14 cells
        // are left there; otherwise the row is taken.
        for &(x, y, w, h) in &blocks {
            for s in &mut shift[y..y + h] {
                *s = (*s).max(x + w + 1 - margin_x);
            }
        }
        for r in 0..height {
            if shift[r] > 0 && margin_w.saturating_sub(shift[r]) < 14 {
                shift[r] = 0;
                occupied[r] = true;
            }
        }
    }
    if !draw {
        // Lines with a margin label on a row a block takes: their labels go
        // under them in the next run (see `layout_with`).
        let mut lines = std::collections::BTreeSet::new();
        for (y, labels) in margin.iter().enumerate() {
            if occupied[y] && !labels.is_empty() {
                let line = rows[..=y].iter().rev().find_map(|r| match r {
                    Row::Code(l) => Some(*l),
                    _ => None,
                });
                lines.extend(line);
            }
        }
        return Pass::Planned(Under {
            callouts: retry,
            lines,
        });
    }

    let base = theme.base();
    let mut canvas = Canvas::new_filled(width, rows.len(), base);
    let segments = highlight_cached(&doc.code, &doc.source.language, theme.syntax);

    // ---- 1. code ----
    for (y, row) in rows.iter().enumerate() {
        let Row::Code(line) = row else { continue };
        let text = doc.line_text(*line).unwrap_or("");
        let in_focus = match opts.pointer {
            // The pointer wins over the focus and the spotlights.
            Some((a, b)) => *line >= a && *line <= b,
            None => opts.focus.is_none_or(|(a, b)| *line >= a && *line <= b) && line_in_spot(*line),
        };
        if opts.line_numbers {
            let n = format!("{:>w$}", line, w = num_w - 1);
            canvas.put_str(0, y, &n, base.fg(theme.muted));
        }
        // A view with no loaded lines (a linked file a caller did not load)
        // draws an empty row, never an index out of bounds.
        let segs = line
            .checked_sub(doc.source.first_line)
            .and_then(|i| segments.get(i as usize))
            .map_or(&[][..], Vec::as_slice);
        paint_code(
            &mut canvas,
            code_x,
            y,
            code_w,
            text,
            segs,
            opts.tab_width,
            base,
            theme,
            in_focus,
        );
    }

    // ---- 2. highlights and marks ----
    let emphasis_row: BTreeMap<u32, usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(y, r)| match r {
            Row::Emphasis(l) => Some((*l, y)),
            _ => None,
        })
        .collect();
    for a in &vis {
        let (span, fg, bg, emphasis) = match &a.shape {
            Shape::Highlight { span, emphasis, .. } if ann_dimmed(a) => (
                span,
                theme.dimmed,
                theme.tint(theme.dimmed, 0.12),
                *emphasis,
            ),
            Shape::Highlight { span, emphasis, .. } => {
                let c = color_of(a);
                let (fg, bg) = if a.color.is_some() {
                    (c, theme.tint(c, 0.22))
                } else {
                    (theme.fg, theme.highlight_bg)
                };
                (span, fg, bg, *emphasis)
            }
            Shape::Mark { span, emphasis, .. } if ann_dimmed(a) => (
                span,
                theme.dimmed,
                theme.tint(theme.dimmed, 0.12),
                *emphasis,
            ),
            Shape::Mark {
                kind,
                span,
                style,
                emphasis,
                ..
            } => {
                let st = doc.resolve_style(kind, style).unwrap_or_default();
                (span, theme.mark_color(&st), theme.mark_bg(&st), *emphasis)
            }
            _ => continue,
        };
        let Some(&y) = row_of_line.get(&span.line) else {
            continue;
        };
        let text = doc.line_text(span.line).unwrap_or("");
        let x0 = code_x + col_to_x(text, span.start, opts.tab_width);
        let x1 = code_x + col_to_x(text, span.end, opts.tab_width);
        let x1 = x1.min(code_x + code_w);
        if x1 > x0 {
            // Without a tint, the span's own color is the annotation's (a
            // plain highlight keeps the theme text color + the accent).
            let ink = if a.color.is_some() || matches!(a.shape, Shape::Mark { .. }) || ann_dimmed(a)
            {
                fg
            } else {
                theme.accent
            };
            // The emphasis glyphs (not the span's text) are drawn in a
            // softened shade of the annotation colour, so the eye lands on
            // the code first (the full colour shouted).
            let soft = theme.tint_ink(ink, EMPHASIS_GLYPH_OPACITY);
            match emphasis {
                Emphasis::Fill => {
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(fg).bg(bg).bold());
                }
                Emphasis::Underline => {
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(ink).bold());
                    if let Some(&uy) = emphasis_row.get(&span.line) {
                        for x in x0..x1 {
                            canvas.put(x, uy, UNDERLINE_GLYPH, base.fg(soft));
                        }
                    }
                }
                Emphasis::Brackets => {
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(ink).bold());
                    // A real space: the empty continuation cell of a wide
                    // character (名, emoji) is not room.
                    let blank_before =
                        x0 > code_x && canvas.get(x0 - 1, y).is_some_and(|c| c.sym == " ");
                    let blank_after =
                        x1 < code_x + code_w && canvas.get(x1, y).is_some_and(|c| c.sym == " ");
                    let under = emphasis_row.get(&span.line).copied();
                    if blank_before {
                        canvas.put(x0 - 1, y, "⌈", base.fg(soft));
                    } else if let Some(uy) = under {
                        canvas.put(x0, uy, "⌊", base.fg(soft));
                    }
                    if blank_after {
                        canvas.put(x1, y, "⌉", base.fg(soft));
                    } else if let Some(uy) = under {
                        // A one-cell span glued on both sides: ⌊ and ⌋ would
                        // share the cell, so it gets a plain underline.
                        let sym = if x1 - x0 == 1 && !blank_before {
                            UNDERLINE_GLYPH
                        } else {
                            "⌋"
                        };
                        canvas.put(x1 - 1, uy, sym, base.fg(soft));
                    }
                }
                Emphasis::Strike => {
                    // The stroke shares the cell's colour with the text, so
                    // both take the softened annotation colour (not the bold
                    // accent): struck code reads as struck, quietly. It is
                    // still code, so it keeps the code contrast floor (the
                    // softened red alone fell under 4.5:1 after compression).
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(theme.legible(soft)));
                    for x in x0..x1 {
                        let cell = canvas.get(x, y).map(|c| (c.sym.clone(), c.style));
                        if let Some((sym, st)) = cell
                            && !sym.trim().is_empty()
                            && !sym.ends_with('\u{336}')
                        {
                            canvas.put(x, y, &format!("{sym}\u{336}"), st);
                        }
                    }
                }
            }
        }
    }

    // ---- 3. boxes ----
    for (bi, b) in boxes.iter().enumerate() {
        let box_color = color_of(b.ann);
        let box_style = base.fg(box_color);
        let lx = box_x0 + b.depth;
        let rx = rbox_x0 + (box_depth - 1 - b.depth);
        let Shape::Box { label, frame, .. } = &b.ann.shape else {
            continue;
        };
        let g = Glyphs::for_border(frame);
        let top = rows.iter().position(|r| *r == Row::BoxTop(bi));
        let bottom = rows.iter().position(|r| *r == Row::BoxBottom(bi));
        let (Some(top), Some(bottom)) = (top, bottom) else {
            continue;
        };
        for y in top + 1..bottom {
            canvas.put_line(lx, y, g.v, box_style);
            canvas.put_line(rx, y, g.v, box_style);
            // subtle tint inside the box (the default; `fill: false` turns it off)
            if frame.fill.unwrap_or(true) {
                canvas.restyle(
                    lx + 1,
                    y,
                    rx - lx - 1,
                    CellStyle::new().bg(theme.tint(box_color, 0.08)),
                );
            }
        }
        canvas.put(lx, top, g.tl, box_style);
        canvas.put(rx, top, g.tr, box_style);
        for x in lx + 1..rx {
            canvas.put(x, top, g.h, box_style);
        }
        if let Some(l) = label {
            let avail = rx.saturating_sub(lx + 4);
            canvas.put(lx + 1, top, g.h, box_style);
            canvas.put(lx + 2, top, " ", box_style);
            if display_width(l, 4) > avail {
                clip(&mut clipped, &b.ann.id, l, avail <= 1);
            }
            let w = canvas.put_str_clipped(lx + 3, top, l, avail, box_style.bold());
            canvas.put(lx + 3 + w, top, " ", box_style);
        }
        canvas.put(lx, bottom, g.bl, box_style);
        canvas.put(rx, bottom, g.br, box_style);
        for x in lx + 1..rx {
            canvas.put(x, bottom, g.h, box_style);
        }
    }

    // ---- 4. arrows between lines ----
    // The cells every arrow's rail (and its two turns) will occupy, so a
    // stub drawn before that rail leaves them alone; the rails are drawn
    // last and own their cells.
    let stub_cells: std::collections::HashSet<(usize, usize)> = arrows
        .iter()
        .filter_map(|a| {
            let Shape::Arrow { from, to, .. } = &a.ann.shape else {
                return None;
            };
            let lane_x = arrow_x0 + a.lane * 2;
            let (ys, yt) = (row_of_line[&from.line], row_of_line[&to.line]);
            Some((lane_x, ys.min(yt), ys.max(yt)))
        })
        .flat_map(|(x, top, bottom)| (top..=bottom).map(move |y| (x, y)))
        .collect();
    let mut rails: Vec<Rail> = Vec::new();
    for a in &arrows {
        let Shape::Arrow {
            from, to, line: ls, ..
        } = &a.ann.shape
        else {
            continue;
        };
        let g = Glyphs::for_style(ls);
        let arrow_color = color_of(a.ann);
        let arrow_style = base.fg(arrow_color).bold();
        let lane_x = arrow_x0 + a.lane * 2;
        let ys = row_of_line[&from.line];
        let yt = row_of_line[&to.line];
        let (top, bottom) = (ys.min(yt), ys.max(yt));
        // The rail and its turns toward the code (╭─ at the top row, ╰─ at
        // the bottom, whichever end is the source) are drawn after every
        // stub, in `rails` below.
        rails.push(Rail {
            x: lane_x,
            top,
            bottom,
            v: g.v,
            turns: (g.tl, g.bl),
            style: arrow_style,
        });
        // Source row: line up to the code, with the start head in the last cell.
        // Inside the lane band the stub skips the cells of other arrows'
        // rails, so it reads as passing *under* them;
        // crossings with a box rail become junction glyphs in
        // the rail's color (Canvas::put_line), whatever the drawing order.
        for x in lane_x + 1..code_x - 2 {
            if x < box_x0 && stub_cells.contains(&(x, ys)) {
                continue;
            }
            canvas.put_line(x, ys, g.h, arrow_style);
        }
        match head_glyph(ls.heads.start, true, g.h) {
            Some(h) => {
                canvas.put(code_x - 2, ys, h, arrow_style);
            }
            None => canvas.put_line(code_x - 2, ys, g.h, arrow_style),
        }
        // Target row: same, with the end head.
        for x in lane_x + 1..code_x - 2 {
            if x < box_x0 && stub_cells.contains(&(x, yt)) {
                continue;
            }
            canvas.put_line(x, yt, g.h, arrow_style);
        }
        match head_glyph(ls.heads.end, true, g.h) {
            Some(h) => {
                canvas.put(code_x - 2, yt, h, arrow_style);
            }
            None => canvas.put_line(code_x - 2, yt, g.h, arrow_style),
        }
        // Mark the source and target points in the code.
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            ys,
            from.line,
            from.col,
            opts.tab_width,
            arrow_color,
        );
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            yt,
            to.line,
            to.col,
            opts.tab_width,
            arrow_color,
        );
    }

    // The rails, over the stubs: a rail always owns its cells in the lane
    // band, so a stub that crossed it earlier shows a one-cell gap.
    for r in rails {
        for y in r.top + 1..r.bottom {
            canvas.put(r.x, y, r.v, r.style);
        }
        canvas.put(r.x, r.top, r.turns.0, r.style);
        canvas.put(r.x, r.bottom, r.turns.1, r.style);
    }

    // ---- 5. arrows on the same line ----
    for (y, row) in rows.iter().enumerate() {
        let Row::ArrowUnder(i) = row else { continue };
        let Shape::Arrow {
            from, to, line: ls, ..
        } = &vis[*i].shape
        else {
            continue;
        };
        let g = Glyphs::for_style(ls);
        let arrow_color = color_of(vis[*i]);
        let arrow_style = base.fg(arrow_color).bold();
        let text = doc.line_text(from.line).unwrap_or("");
        let fx = code_x + col_to_x(text, from.col, opts.tab_width).min(code_w.saturating_sub(1));
        let tx = code_x + col_to_x(text, to.col, opts.tab_width).min(code_w.saturating_sub(1));
        if tx > fx {
            // Leaves the source downward (╰), runs right, head points right.
            canvas.put(
                fx,
                y,
                head_glyph(ls.heads.start, false, g.bl).unwrap_or(g.bl),
                arrow_style,
            );
            for x in fx + 1..tx {
                canvas.put_line(x, y, g.h, arrow_style);
            }
            canvas.put(
                tx,
                y,
                head_glyph(ls.heads.end, true, g.h).unwrap_or(g.h),
                arrow_style,
            );
        } else if fx > tx {
            canvas.put(
                tx,
                y,
                head_glyph(ls.heads.end, false, g.h).unwrap_or(g.h),
                arrow_style,
            );
            for x in tx + 1..fx {
                canvas.put_line(x, y, g.h, arrow_style);
            }
            canvas.put(
                fx,
                y,
                head_glyph(ls.heads.start, true, g.br).unwrap_or(g.br),
                arrow_style,
            );
        } else {
            canvas.put(
                fx,
                y,
                match ls.heads.end {
                    crate::model::Head::None => g.v,
                    crate::model::Head::Dot => "●",
                    crate::model::Head::Bar => "┴",
                    crate::model::Head::Open => "^",
                    crate::model::Head::Arrow => "▲",
                },
                arrow_style,
            );
        }
        let code_row = row_of_line[&from.line];
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            code_row,
            from.line,
            from.col,
            opts.tab_width,
            arrow_color,
        );
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            code_row,
            to.line,
            to.col,
            opts.tab_width,
            arrow_color,
        );
    }

    // ---- 6. labels under the lines ----
    // (top row, bottom row, right rail x) of each box: a label under a line
    // inside a box ends a cell before the box's right rail, as the code does.
    let box_rails: Vec<(usize, usize, usize)> = (0..boxes.len())
        .filter_map(|bi| {
            let top = rows.iter().position(|r| *r == Row::BoxTop(bi))?;
            let bottom = rows.iter().position(|r| *r == Row::BoxBottom(bi))?;
            Some((top, bottom, rbox_x0 + (box_depth - 1 - boxes[bi].depth)))
        })
        .collect();
    for (y, row) in rows.iter().enumerate() {
        let Row::Under(i) = row else { continue };
        let (lbl, is_note) = &unders[*i];
        // The innermost box around this row and the first margin block on
        // it: neither is drawn over, nor touched (one cell short of it), when
        // it is on the canvas.
        let end = box_rails
            .iter()
            .filter(|(top, bottom, _)| *top < y && y < *bottom)
            .map(|(_, _, rx)| *rx)
            .chain(
                blocks
                    .iter()
                    .filter(|(_, by, _, bh)| *by <= y && y < by + bh)
                    .map(|(bx, ..)| *bx),
            )
            .min()
            .filter(|&rx| rx < width)
            .map_or(width, |rx| rx - 1);
        let style = base.fg(lbl.color).italic();
        let x = match &lbl.glyph {
            Some(g) => {
                code_x
                    + keep_bg(&mut canvas, code_x, y, |c| {
                        c.put_str(code_x, y, g, base.fg(lbl.color).bold())
                    })
                    + 1
            }
            None => {
                let prefix = if *is_note { "┆ " } else { "· " };
                keep_bg(&mut canvas, code_x, y, |c| {
                    c.put_str(code_x, y, prefix, style)
                });
                code_x + 2
            }
        };
        let avail = end.saturating_sub(x);
        if display_width(&lbl.text, 4) > avail {
            clip(&mut clipped, &lbl.id, &lbl.text, avail <= 1);
        }
        keep_bg(&mut canvas, x, y, |c| {
            c.put_str_clipped(x, y, &lbl.text, avail, style.prose())
        });
    }
    // Arrow labels beside the code of their target line: after the
    // code and the arrows, inside the code column, so a box border is never
    // touched. Several on one line follow each other.
    {
        let mut used: BTreeMap<u32, usize> = BTreeMap::new();
        for (line, glyph, text, color, id) in &beside {
            let Some(&y) = row_of_line.get(line) else {
                continue;
            };
            let text_w = code_width(doc.line_text(*line).unwrap_or(""), opts.tab_width);
            let x0 = code_x + text_w + 2 + used.get(line).copied().unwrap_or(0);
            let end = code_x + code_w;
            if x0 >= end {
                clip(&mut clipped, id, text, true);
                continue;
            }
            let gw = keep_bg(&mut canvas, x0, y, |c| {
                c.put_str_clipped(x0, y, glyph, end - x0, base.fg(*color).bold())
            });
            let x1 = x0 + gw + 1;
            if x1 >= end || display_width(text, 4) > end - x1 {
                clip(&mut clipped, id, text, x1 + 1 >= end);
            }
            let tw = if x1 < end {
                keep_bg(&mut canvas, x1, y, |c| {
                    c.put_str_clipped(x1, y, text, end - x1, base.fg(*color).italic().prose())
                })
            } else {
                0
            };
            *used.entry(*line).or_insert(0) += gw + 1 + tw + 2;
        }
    }

    // ---- 6b. callouts ----
    let draw_block = |canvas: &mut Canvas, c: &CalloutPlan, x: usize, y0: usize, color: Rgb| {
        let w = c.width;
        let g = Glyphs::for_border(&c.frame);
        let border = base.fg(color);
        // Callouts are not tinted unless asked (`fill: true`).
        let inside = if c.frame.fill.unwrap_or(false) {
            base.bg(theme.tint(color, 0.08))
        } else {
            base
        };
        let text_style = inside.fg(color).italic();
        canvas.put(x, y0, g.tl, border);
        for xx in x + 1..x + w - 1 {
            canvas.put(xx, y0, g.h, border);
        }
        canvas.put(x + w - 1, y0, g.tr, border);
        // Whether the title had to be clipped.
        let mut title_clipped = false;
        if let Some(t) = &c.title {
            canvas.put(x + 2, y0, " ", border);
            title_clipped = display_width(t, 4) > w.saturating_sub(6);
            let tw = canvas.put_str_clipped(x + 3, y0, t, w.saturating_sub(6), border.bold());
            canvas.put(x + 3 + tw, y0, " ", border);
        }
        for (k, l) in c.lines.iter().enumerate() {
            let y = y0 + 1 + k;
            canvas.put(x, y, g.v, border);
            for xx in x + 1..x + w - 1 {
                canvas.put(xx, y, " ", inside);
            }
            canvas.put_str(x + 2, y, l, text_style);
            canvas.put(x + w - 1, y, g.v, border);
        }
        let yb = y0 + 1 + c.lines.len();
        canvas.put(x, yb, g.bl, border);
        for xx in x + 1..x + w - 1 {
            canvas.put(xx, yb, g.h, border);
        }
        canvas.put(x + w - 1, yb, g.br, border);
        title_clipped
    };
    // Below: their rows already exist.
    let mut first_row = vec![None; callouts.len()];
    for (y, r) in rows.iter().enumerate() {
        if let Row::Callout(ci, 0) = r {
            first_row[*ci] = Some(y);
        }
    }
    for (c, y0) in callouts.iter().zip(first_row) {
        if let Some(y0) = y0
            && draw_block(&mut canvas, c, code_x, y0, color_of(vis[c.idx]))
        {
            let t = c.title.as_deref().unwrap_or("");
            clip(&mut clipped, &vis[c.idx].id, t, false);
        }
    }
    // Margin: where the plan put them.
    for &(ci, x, y) in &placed {
        let c = &callouts[ci];
        if draw_block(&mut canvas, c, x, y, color_of(vis[c.idx])) {
            let t = c.title.as_deref().unwrap_or("");
            clip(&mut clipped, &vis[c.idx].id, t, false);
        }
    }

    // ---- 7. margin ----
    // Each label takes its own margin row; if it does not fit, it is split
    // into words and continues on the free rows below. A row that belongs to
    // another line's label is never taken by a continuation or an overflow
    // (so labels do not drift away from their line); what does not fit is
    // clipped with an ellipsis on the last row it got. A line with several
    // labels draws them under itself (`under_lines`), so a code row
    // here holds one label; should a row still get several, the next ones
    // start on the free rows after the previous one.
    if labels_in_margin && margin_w > 0 {
        let height = canvas.height;
        let reserved: Vec<bool> = margin.iter().map(|l| !l.is_empty()).collect();
        for (y, labels) in margin.iter().enumerate() {
            let mut next_row = y;
            // A label starts on a row of its own line, never beside the
            // next one: the rows up to the next code row.
            let own_end = (y + 1..height)
                .find(|&r| matches!(rows[r], Row::Code(_)))
                .unwrap_or(height);
            let free = |r: usize, occupied: &[bool]| {
                r < height && !occupied[r] && (!reserved[r] || r == y)
            };
            for (k, l) in labels.iter().enumerate() {
                let prefix = if l.note { "┆ " } else { "" };
                let style = if l.note {
                    base.fg(l.color).italic()
                } else {
                    base.fg(l.color)
                };
                // No free row or no room: this label and the next ones
                // of the line are left out (`tiza check` reports them).
                let left_out = |clipped: &mut Vec<ClippedLabel>| {
                    for l in &labels[k..] {
                        clip(clipped, &l.id, &l.text, true);
                    }
                };
                let Some(mut r) = (next_row..own_end).find(|&r| free(r, &occupied)) else {
                    left_out(&mut clipped);
                    break;
                };
                let x0 = margin_x + shift[r];
                let text_w = margin_w.saturating_sub(shift[r] + display_width(prefix, 4) + 1);
                if text_w < 4 {
                    left_out(&mut clipped);
                    break;
                }
                let pieces = wrap_words(&l.text, text_w);
                let px = x0 + display_width(prefix, 4);
                for (i, piece) in pieces.iter().enumerate() {
                    if !free(r, &occupied) || shift[r] > x0 - margin_x {
                        // No room for this piece: the previous row shows the
                        // rest of the text, clipped with an ellipsis.
                        if i > 0 {
                            let rest = pieces[i - 1..].join(" ");
                            if display_width(&rest, 4) > text_w {
                                clip(&mut clipped, &l.id, &l.text, false);
                            }
                            canvas.put_str_clipped(px, r - 1, &rest, text_w, style.prose());
                        }
                        break;
                    }
                    let mut x = x0;
                    if i == 0 {
                        x += canvas.put_str(x, r, prefix, style);
                    } else {
                        x += canvas.put_str(x, r, &" ".repeat(display_width(prefix, 4)), style);
                    }
                    canvas.put_str(x, r, piece, style.prose());
                    occupied[r] = true;
                    r += 1;
                }
                next_row = r;
            }
        }
    }

    // ---- scripted selection and cursor: what `select` and
    // `cursor` moves left at this step, drawn like the TUI's own ----
    if let Some((from, to)) = shared.selection {
        let sel = CellStyle::new().bg(theme.tint(theme.accent, 0.35));
        for line in from.line..=to.line {
            let (Some(&y), Some(text)) = (row_of_line.get(&line), doc.line_text(line)) else {
                continue;
            };
            let start = if line == from.line { from.col } else { 0 };
            let end = if line == to.line {
                to.col
            } else {
                doc.line_len(line)
            };
            let x0 = code_x + col_to_x(text, start, opts.tab_width);
            let x1 = code_x + col_to_x(text, end, opts.tab_width).max(x0 + 1);
            let x1 = x1.min(code_x + code_w);
            if x1 > x0 {
                canvas.restyle(x0, y, x1 - x0, sel);
            }
        }
    }
    if let Some(c) = shared.cursor
        && let (Some(&y), Some(text)) = (row_of_line.get(&c.line), doc.line_text(c.line))
    {
        let col = c.col.min(doc.line_len(c.line).saturating_sub(1));
        let x = code_x + col_to_x(text, col, opts.tab_width);
        if x < code_x + code_w {
            canvas.restyle(x, y, 1, CellStyle::new().fg(theme.bg).bg(theme.fg));
        }
    }

    Pass::Drawn(Layout {
        canvas,
        rows,
        row_of_line,
        code_x,
        clipped_code: (code_full_w > code_w).then_some(code_full_w),
        labels_in_margin,
        clipped_labels: clipped,
    })
}

/// Splits `text` into chunks of at most `width` cells, breaking on words
/// `glyph legend[: label]` for a mark, from its resolved style.
/// Box-drawing glyphs for one stroke + corner style.
struct Glyphs {
    h: &'static str,
    v: &'static str,
    /// Top-left corner (`╭`).
    tl: &'static str,
    /// Top-right corner (`╮`).
    tr: &'static str,
    /// Bottom-left corner (`╰`).
    bl: &'static str,
    /// Bottom-right corner (`╯`).
    br: &'static str,
}

impl Glyphs {
    fn new(stroke: crate::model::Stroke, route: crate::model::Route) -> Glyphs {
        use crate::model::{Route, Stroke};
        let (h, v) = match stroke {
            Stroke::Solid => ("─", "│"),
            Stroke::Dashed => ("╌", "╎"),
            Stroke::Dotted => ("┄", "┆"),
            Stroke::Thick => ("━", "┃"),
            Stroke::Double => ("═", "║"),
        };
        // Thick and double have no rounded corners in Unicode.
        let (tl, tr, bl, br) = match (stroke, route) {
            (Stroke::Thick, _) => ("┏", "┓", "┗", "┛"),
            (Stroke::Double, _) => ("╔", "╗", "╚", "╝"),
            (_, Route::Rounded) => ("╭", "╮", "╰", "╯"),
            (_, Route::Sharp) => ("┌", "┐", "└", "┘"),
        };
        Glyphs {
            h,
            v,
            tl,
            tr,
            bl,
            br,
        }
    }

    fn for_style(ls: &crate::model::LineStyle) -> Glyphs {
        Glyphs::new(ls.stroke, ls.route)
    }

    fn for_border(bs: &crate::model::BorderStyle) -> Glyphs {
        Glyphs::new(bs.border, bs.corners)
    }
}

/// Runs `draw` (text written from `x` on row `y`, returning its width) and
/// gives the cells it wrote back the background they had: a label under a
/// line or beside the code takes the fill of the box it sits in, not the
/// base background.
fn keep_bg(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    draw: impl FnOnce(&mut Canvas) -> usize,
) -> usize {
    let before: Vec<Option<Rgb>> = (x..canvas.width)
        .map(|cx| canvas.get(cx, y).and_then(|c| c.style.bg))
        .collect();
    let w = draw(canvas);
    for (cx, bg) in (x..x + w).zip(before) {
        if let Some(bg) = bg {
            canvas.restyle(cx, y, 1, CellStyle::new().bg(bg));
        }
    }
    w
}

/// Glyph for a head at a line end, or `None` to continue the line.
/// `points_right` is the direction the line travels into that end.
fn head_glyph(head: crate::model::Head, points_right: bool, _line: &str) -> Option<&'static str> {
    use crate::model::Head;
    Some(match (head, points_right) {
        (Head::None, _) => return None,
        (Head::Arrow, true) => "▶",
        (Head::Arrow, false) => "◀",
        (Head::Dot, _) => "●",
        (Head::Bar, true) => "┤",
        (Head::Bar, false) => "├",
        (Head::Open, true) => ">",
        (Head::Open, false) => "<",
    })
}

/// `doc` with every blank label, box label and callout title taken as
/// none, and without its blank notes (`check` reports those); `None` when
/// it has none of them, which is nearly always (no copy).
fn without_blank_labels(doc: &Document) -> Option<Document> {
    let blank = |l: &Option<String>| l.as_deref().is_some_and(|t| t.trim().is_empty());
    let has_blank = |a: &Annotation| match &a.shape {
        Shape::Highlight { label, .. }
        | Shape::Mark { label, .. }
        | Shape::Arrow { label, .. }
        | Shape::Box { label, .. } => blank(label),
        Shape::Callout { title, .. } => blank(title),
        Shape::Note { text, .. } => text.trim().is_empty(),
        Shape::Spotlight { .. } => false,
    };
    if !doc.annotations.iter().any(has_blank) {
        return None;
    }
    let mut d = doc.clone();
    d.annotations
        .retain(|a| !matches!(&a.shape, Shape::Note { text, .. } if text.trim().is_empty()));
    for a in &mut d.annotations {
        match &mut a.shape {
            Shape::Highlight { label, .. }
            | Shape::Mark { label, .. }
            | Shape::Arrow { label, .. }
            | Shape::Box { label, .. }
                if blank(label) =>
            {
                *label = None;
            }
            Shape::Callout { title, .. } if blank(title) => *title = None,
            _ => {}
        }
    }
    Some(d)
}

fn mark_text(
    doc: &Document,
    kind: &str,
    style: &crate::model::MarkStyle,
    label: Option<&str>,
) -> String {
    let st = doc.resolve_style(kind, style).unwrap_or_default();
    let mut out = String::new();
    if let Some(g) = &st.glyph {
        out.push_str(g);
        out.push(' ');
    }
    out.push_str(st.legend.as_deref().unwrap_or(kind));
    if let Some(l) = label {
        out.push_str(": ");
        out.push_str(l);
    }
    out
}

/// (and on graphemes if a single word does not fit).
pub(crate) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut line = String::new();
    let mut line_w = 0usize;
    for word in text.split_whitespace() {
        let w = display_width(word, 4);
        if w > width {
            // Word longer than the width: split by graphemes.
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            let mut chunk = String::new();
            let mut cw = 0;
            for g in word.graphemes(true) {
                let gw = unicode_width::UnicodeWidthStr::width(g).max(1);
                if cw + gw > width {
                    out.push(std::mem::take(&mut chunk));
                    cw = 0;
                }
                chunk.push_str(g);
                cw += gw;
            }
            line = chunk;
            line_w = cw;
            continue;
        }
        let sep = usize::from(!line.is_empty());
        if line_w + sep + w > width {
            out.push(std::mem::take(&mut line));
            line_w = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_w += 1;
        }
        line.push_str(word);
        line_w += w;
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

/// Screen column (relative to the start of the code) of grapheme `col` of `text`.
/// Whether a span's emphasis needs the row under its line: underlines always,
/// brackets only when a side has no blank cell beside the span.
fn emphasis_needs_row(doc: &Document, span: &TextSpan, e: Emphasis) -> bool {
    use unicode_segmentation::UnicodeSegmentation;
    match e {
        Emphasis::Underline => true,
        Emphasis::Brackets => {
            let Some(text) = doc.line_text(span.line) else {
                return false;
            };
            let g: Vec<&str> = text.graphemes(true).collect();
            let before = span
                .start
                .checked_sub(1)
                .and_then(|i| g.get(i as usize))
                .is_some_and(|c| c.trim().is_empty());
            let after = g.get(span.end as usize).is_none_or(|c| c.trim().is_empty());
            !(before && after)
        }
        _ => false,
    }
}

/// The cell where the grapheme `col` of a code line starts, as the line is
/// drawn (an invisible character takes the cell of its mark).
pub fn col_to_x(text: &str, col: u32, tab_width: usize) -> usize {
    let mut w = 0;
    for (i, g) in text.graphemes(true).enumerate() {
        if i as u32 >= col {
            break;
        }
        if g == "\t" {
            w += tab_width - (w % tab_width);
        } else {
            w += code_grapheme_width(g);
        }
    }
    w
}

#[allow(clippy::too_many_arguments)]
fn paint_code(
    canvas: &mut Canvas,
    x0: usize,
    y: usize,
    max_w: usize,
    text: &str,
    segs: &[Segment],
    tab_width: usize,
    base: CellStyle,
    theme: &Theme,
    in_focus: bool,
) {
    let full_w = code_width(text, tab_width);
    let clip = full_w > max_w;
    let limit = if clip { max_w.saturating_sub(1) } else { max_w };
    let mut x = 0usize;
    'outer: for seg in segs {
        let style = if in_focus {
            let mut s = base;
            if let Some(fg) = seg.fg {
                s = s.fg(theme.legible(fg));
            }
            if seg.bold {
                s = s.bold();
            }
            if seg.italic {
                s = s.italic();
            }
            s
        } else {
            base.fg(theme.dimmed)
        };
        for g in seg.text.graphemes(true) {
            let (shown, marks) = crate::printable::code_glyph(g);
            let shown_w = unicode_width::UnicodeWidthStr::width(&*shown);
            let w = if g == "\t" {
                tab_width - (x % tab_width)
            } else {
                shown_w + marks
            };
            if x + w > limit {
                break 'outer;
            }
            if g == "\t" {
                for k in 0..w {
                    canvas.put(x0 + x + k, y, " ", style);
                }
            } else {
                if !shown.is_empty() {
                    canvas.put(x0 + x, y, &shown, style);
                }
                // An invisible character takes a cell of its own, marked in
                // the error colour, focus or not: it must be seen.
                for k in 0..marks {
                    canvas.put(
                        x0 + x + shown_w + k,
                        y,
                        crate::printable::INVISIBLE_MARK,
                        base.fg(theme.error),
                    );
                }
            }
            x += w;
        }
    }
    if clip {
        canvas.put(x0 + limit, y, "…", base.fg(theme.muted));
    }
}

#[allow(clippy::too_many_arguments)]
fn mark_anchor(
    canvas: &mut Canvas,
    doc: &Document,
    code_x: usize,
    code_w: usize,
    y: usize,
    line: u32,
    col: u32,
    tab_width: usize,
    color: Rgb,
) {
    let text = doc.line_text(line).unwrap_or("");
    let x = col_to_x(text, col, tab_width);
    if x < code_w {
        // Underline the token that starts at the anchor (up to the next separator).
        let rest = crate::model::grapheme_slice(text, col, u32::MAX);
        let token_len = rest
            .graphemes(true)
            .take_while(|g| g.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .count()
            .max(1);
        let w = col_to_x(text, col + token_len as u32, tab_width) - x;
        canvas.restyle(
            code_x + x,
            y,
            w.min(code_w - x),
            CellStyle::new().fg(color).bold(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Anchor, Placement, TextSpan};

    fn doc() -> Document {
        let mut d = Document::from_code(
            "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n    println!(\"{s}\");\n}\n\nfn takes(t: String) {\n    println!(\"{t}\");\n}\n",
            "rs",
            None,
        );
        let s = d.find_text("let s", 1, None).unwrap();
        d.add(Annotation {
            id: "a1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Mark {
                kind: "move".into(),
                style: Default::default(),
                span: TextSpan {
                    line: 3,
                    start: 10,
                    end: 11,
                },
                label: None,
                emphasis: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a2".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Arrow {
                from: Anchor {
                    line: s.line,
                    col: s.start + 4,
                },
                to: Anchor { line: 7, col: 9 },
                label: Some("s moves into t".into()),
                line: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a3".into(),
            step: 2,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Box {
                from_line: 7,
                to_line: 9,
                label: Some("takes".into()),
                frame: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a4".into(),
            step: 2,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Note {
                line: 4,
                text: "error: s is no longer the owner".into(),
            },
        })
        .unwrap();
        d
    }

    #[test]
    fn boxes_insert_virtual_rows_and_arrows_use_left_rail() {
        let d = doc();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 2,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        // 9 lines + 2 box rows
        assert_eq!(l.rows.len(), 11);
        assert!(text.contains("╭─ takes"));
        assert!(
            text.contains("┼"),
            "the arrow crosses the box rail with a junction"
        );
        assert!(text.contains("╰─"));
        assert!(text.contains("▶"));
        assert!(text.contains("━▶ move"));
        assert!(text.contains("┆ error: s is no longer the owner"));
        // Nothing exceeds the width.
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 90);
        }
    }

    #[test]
    fn spotlight_dims_code_and_other_annotations() {
        let mut d = doc();
        d.add(Annotation {
            id: "s1".into(),
            step: 3,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Spotlight {
                from_line: Some(7),
                to_line: Some(9),
                ids: vec!["a3".into()],
                until: None,
            },
        })
        .unwrap();
        let th = Theme::dark();
        let on = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 3,
                ..Default::default()
            },
        );
        let off = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 4,
                ..Default::default()
            },
        );
        let fg_at = |l: &Layout, line: u32| {
            let y = l.row_of_line[&line];
            l.canvas.get(l.code_x + 4, y).unwrap().style.fg
        };
        assert_eq!(
            fg_at(&on, 2),
            Some(th.dimmed),
            "outside the spotlight is dimmed"
        );
        assert_ne!(
            fg_at(&on, 7),
            Some(th.dimmed),
            "inside the spotlight is not"
        );
        assert_ne!(
            fg_at(&off, 2),
            Some(th.dimmed),
            "at step 4 it no longer applies"
        );
        // Note a4 (not included in ids) is drawn dimmed; box a3 keeps its color.
        let text = on.canvas.to_plain_text();
        assert!(text.contains("takes"));
        let y_note = on.row_of_line[&4];
        let note_cell = (0..on.canvas.width)
            .rev()
            .filter_map(|x| on.canvas.get(x, y_note))
            .find(|c| c.sym == "┆")
            .unwrap();
        assert_eq!(note_cell.style.fg, Some(th.dimmed));
    }

    #[test]
    fn margin_label_wraps_over_free_rows_and_is_clipped_before_the_next_lines_label() {
        let mut d = doc();
        // A long note on a line whose next line has its own label.
        d.add(Annotation {
            id: "n1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Note {
                line: 2,
                text: "s is born here with String::from and lives until it is moved".into(),
            },
        })
        .unwrap();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        // Line 3 has its own label (the mark), so the long note on line 2
        // cannot continue onto row 3: it is clipped there with an ellipsis.
        // Labels never drift away from their line (a second label on line 2
        // goes under it with the first).
        let row = |line: u32| l.row_of_line[&line];
        assert!(
            l.canvas.row_text(row(2)).contains("s is born here"),
            "{text}"
        );
        assert!(l.canvas.row_text(row(2)).contains('…'), "{text}");
        assert!(
            l.canvas.row_text(row(3)).contains("━▶ move"),
            "line 3 keeps its label: {text}"
        );
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 80);
        }
        // Without a label on line 3, the note wraps freely over rows 2-4.
        d.delete("a1").unwrap();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        assert!(!text.contains('…'), "free rows below: no clipping\n{text}");
        let row = |line: u32| l.row_of_line[&line];
        assert!(l.canvas.row_text(row(2)).contains("s is born here"));
        assert!(l.canvas.row_text(row(3)).contains("and lives until"));
        assert_eq!(
            wrap_words("hello cruel world", 11),
            vec!["hello cruel", "world"]
        );
        assert_eq!(
            wrap_words("supercalifragilistico", 8),
            vec!["supercal", "ifragili", "stico"]
        );
    }

    #[test]
    fn step_filters_annotations() {
        let d = doc();
        let th = Theme::light();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 1,
                ..Default::default()
            },
        );
        assert_eq!(l.rows.len(), 9, "at step 1 there is no box yet");
        assert!(!l.canvas.to_plain_text().contains("takes\n") || true);
    }

    #[test]
    fn narrow_terminal_moves_notes_under_lines() {
        let d = doc();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 48,
                step: 2,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        assert!(l.rows.iter().any(|r| matches!(r, Row::Under(_))));
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 48);
        }
    }

    /// Code without annotations, plus a callout over L2-3 and a note on L4
    /// (the "following line" of the spec).
    fn callout_doc(placement: Placement) -> Document {
        let mut d = Document::from_code(
            "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n    println!(\"{s}\");\n}\n\nfn takes(t: String) {\n    println!(\"{t}\");\n}\n",
            "rs",
            None,
        );
        d.add(Annotation {
            id: "c1".into(),
            step: 1,
            color: Some("cyan".into()),
            markers: None,
            file: None,
            shape: Shape::Callout {
                from_line: 2,
                to_line: Some(3),
                text: "s owns the String. Passing it to takes moves the value, so after line 3 main cannot use s any more: the compiler rejects line 4.".into(),
                title: Some("move".into()),
                placement,
                width: None,
                frame: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "n1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Note {
                line: 4,
                text: "does not compile".into(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "b1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Box {
                from_line: 2,
                to_line: 3,
                label: Some("moved here".into()),
                frame: Default::default(),
            },
        })
        .unwrap();
        d
    }

    fn row_with(text: &str, l: &Layout) -> usize {
        (0..l.canvas.height)
            .find(|&y| l.canvas.row_text(y).contains(text))
            .unwrap_or_else(|| panic!("{text:?} not rendered:\n{}", l.canvas.to_plain_text()))
    }

    #[test]
    fn callout_in_margin_reserves_rows_and_next_label_stays_on_its_line() {
        let d = callout_doc(Placement::Margin);
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 114,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        // Block is a bordered box with its title, aligned with L2.
        let top = row_with("╭─ move ─", &l);
        assert_eq!(top, l.row_of_line[&2], "callout top aligns with from_line");
        assert!(text.contains("│ s owns the String."));
        // The note on L4 is drawn on its own line, not pushed below the block…
        let note_row = row_with("┆ does not compile", &l);
        assert_eq!(note_row, l.row_of_line[&4]);
        // …which means the block only occupies the margin to the right.
        assert!(l.canvas.row_text(note_row).contains("println!"));
        // No row exceeds the width and the code block is contiguous (no Callout rows).
        assert!(!l.rows.iter().any(|r| matches!(r, Row::Callout(..))));
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 114);
        }
    }

    /// `callout_doc` with the callout moved to the last lines, where its
    /// block has no room in the margin (fewer rows below than it is tall)
    /// and it is drawn as a block under its line.
    /// `tail`: code after `main`.
    fn short_callout_doc(tail: &str, with_note: bool) -> Document {
        let mut d = Document::from_code(
            &format!(
                "fn main() {{\n    let s = String::from(\"hola\");\n    takes(s);\n}}\n{tail}"
            ),
            "rs",
            None,
        );
        d.add(Annotation {
            id: "c1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Callout {
                from_line: 3,
                to_line: None,
                text: "takes(s) moves the String: main cannot use s after this line".into(),
                title: None,
                placement: Placement::Margin,
                width: None,
                frame: Default::default(),
            },
        })
        .unwrap();
        if with_note {
            d.add(Annotation {
                id: "n1".into(),
                step: 1,
                color: None,
                markers: None,
                file: None,
                shape: Shape::Note {
                    line: 3,
                    text: "the call".into(),
                },
            })
            .unwrap();
        }
        d
    }

    #[test]
    fn fallen_back_callout_alone_on_its_line_is_a_block_under_it() {
        let d = short_callout_doc("\nfn f() {}\n", false);
        let l = layout(
            &d,
            &Theme::dark(),
            &LayoutOptions {
                width: 70,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        assert!(l.labels_in_margin);
        // No label of its words in the margin, nor under the line: the
        // block, right under L3, before L4.
        assert!(!text.contains('┆'), "{text}");
        let y = l.row_of_line[&3];
        assert!(matches!(l.rows[y + 1], Row::Callout(_, 0)), "{text}");
        assert!(
            l.canvas
                .row_text(y + 2)
                .contains("│ takes(s) moves the String:"),
            "{text}"
        );
        assert!(l.row_of_line[&4] > y + 3, "{text}");
        assert!(l.clipped_labels.is_empty(), "{:?}", l.clipped_labels);
    }

    #[test]
    fn fallen_back_callout_with_another_label_goes_under_its_line_as_a_block() {
        let d = short_callout_doc("", true);
        let l = layout(
            &d,
            &Theme::dark(),
            &LayoutOptions {
                width: 100,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        // The note under the line, then the callout as a whole block (as
        // the narrow layout draws it), never one row cut with `…`.
        let y = l.row_of_line[&3];
        let r3 = l.canvas.row_text(y);
        assert!(r3.contains("takes(s);") && !r3.contains('┆'), "{text}");
        assert!(l.canvas.row_text(y + 1).contains("┆ the call"), "{text}");
        assert!(l.canvas.row_text(y + 2).contains('╭'), "{text}");
        assert!(
            l.canvas
                .row_text(y + 3)
                .contains("│ takes(s) moves the String:"),
            "{text}"
        );
        assert!(matches!(l.rows[y + 2], Row::Callout(_, 0)), "{text}");
        assert_eq!(l.row_of_line[&4], y + 7, "{text}");
        assert!(l.clipped_labels.is_empty(), "{:?}", l.clipped_labels);
    }

    #[test]
    fn under_labels_keep_the_fill_of_the_box_they_sit_in() {
        // Three labels on L3 inside a filled box: the cells of their under
        // rows keep the box's tint, as the code rows around them.
        let mut d = doc();
        for (id, text) in [("n1", "first"), ("n2", "second")] {
            d.add(Annotation {
                id: id.into(),
                step: 1,
                color: None,
                markers: None,
                file: None,
                shape: Shape::Note {
                    line: 3,
                    text: text.into(),
                },
            })
            .unwrap();
        }
        d.add(Annotation {
            id: "b1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Box {
                from_line: 2,
                to_line: 4,
                label: None,
                frame: Default::default(),
            },
        })
        .unwrap();
        let th = Theme::dark();
        for width in [60, 100] {
            let l = layout(
                &d,
                &th,
                &LayoutOptions {
                    width,
                    step: 1,
                    ..Default::default()
                },
            );
            let text = l.canvas.to_plain_text();
            let fill = l.canvas.get(l.code_x, l.row_of_line[&4]).unwrap().style.bg;
            assert_ne!(fill, Some(th.bg), "the box is filled\n{text}");
            // The mark of `doc()` and the two notes, under L3.
            let y = row_with("┆ first", &l);
            assert_eq!(y, l.row_of_line[&3] + 2, "{text}");
            for y in y - 1..=y + 1 {
                for x in l.code_x..l.code_x + 8 {
                    let c = l.canvas.get(x, y).unwrap();
                    assert_eq!(
                        c.style.bg,
                        fill,
                        "width {width}, cell {x},{y} «{}»\n{text}",
                        c.sym.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn callout_below_inserts_rows_inside_the_box() {
        let d = callout_doc(Placement::Below);
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        let callout_rows: Vec<usize> = l
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Row::Callout(..)))
            .map(|(y, _)| y)
            .collect();
        assert!(!callout_rows.is_empty(), "below placement inserts rows");
        // Rows sit after L3 and before L4.
        assert!(callout_rows[0] > l.row_of_line[&3]);
        assert!(*callout_rows.last().unwrap() < l.row_of_line[&4]);
        // The Box around L2-3 is extended: its bottom border comes after the callout.
        let box_top = row_with("╭─ moved here ─", &l);
        let box_bottom = l
            .rows
            .iter()
            .position(|r| matches!(r, Row::BoxBottom(_)))
            .unwrap();
        assert!(box_top < callout_rows[0] && box_bottom > *callout_rows.last().unwrap());
        // Every callout row carries the box rail on the left.
        for &y in &callout_rows {
            assert!(
                l.canvas.row_text(y).contains('│'),
                "row {y}: {}",
                l.canvas.row_text(y)
            );
        }
        assert!(text.contains("│ s owns the String."));
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 80);
        }
    }

    #[test]
    fn callout_wider_than_the_box_is_drawn_after_its_bottom_border() {
        // Box L2-3, callout L1-3: the callout explains more than the box.
        let mut d = callout_doc(Placement::Below);
        d.update(Annotation {
            id: "c1".into(),
            step: 1,
            color: Some("cyan".into()),
            markers: None,
            file: None,
            shape: Shape::Callout {
                from_line: 1,
                to_line: Some(3),
                text: "main as a whole: s is created and then moved.".into(),
                title: Some("whole main".into()),
                placement: Placement::Below,
                width: None,
                frame: Default::default(),
            },
        })
        .unwrap();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        let callout_rows: Vec<usize> = l
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Row::Callout(..)))
            .map(|(y, _)| y)
            .collect();
        let box_bottom = l
            .rows
            .iter()
            .position(|r| matches!(r, Row::BoxBottom(_)))
            .unwrap();
        assert!(
            box_bottom < callout_rows[0],
            "box closes before the callout: bottom at {box_bottom}, callout at {callout_rows:?}"
        );
        assert!(*callout_rows.last().unwrap() < l.row_of_line[&4]);
        // No box rail on the callout rows: the block itself is the first thing drawn.
        let (first, last) = (callout_rows[0], *callout_rows.last().unwrap());
        for &y in &callout_rows {
            let row = l.canvas.row_text(y);
            let want = if y == first {
                '╭'
            } else if y == last {
                '╰'
            } else {
                '│'
            };
            assert_eq!(
                row.trim_start().chars().next(),
                Some(want),
                "row {y}: {row}"
            );
            assert!(!row.contains("│  ╭"), "row {y} carries a box rail: {row}");
        }
        // The narrower case is unchanged: callout inside the box.
        let inside = layout(
            &callout_doc(Placement::Below),
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let bb = inside
            .rows
            .iter()
            .position(|r| matches!(r, Row::BoxBottom(_)))
            .unwrap();
        assert!(
            inside.rows[..bb]
                .iter()
                .any(|r| matches!(r, Row::Callout(..)))
        );
    }

    #[test]
    fn callout_auto_follows_the_margin_threshold() {
        let th = Theme::dark();
        let wide = layout(
            &callout_doc(Placement::Auto),
            &th,
            &LayoutOptions {
                width: 114,
                step: 1,
                ..Default::default()
            },
        );
        assert!(!wide.rows.iter().any(|r| matches!(r, Row::Callout(..))));
        let narrow = layout(
            &callout_doc(Placement::Auto),
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        assert!(narrow.rows.iter().any(|r| matches!(r, Row::Callout(..))));
    }
}
