//! Property tests (t-052): anchors and layout never panic and never overflow
//! for arbitrary code, sizes and annotations; documents round-trip through JSON.

use codedraw_core::canvas::display_width;
use codedraw_core::layout::{LayoutOptions, layout};
use codedraw_core::model::{Anchor, Annotation, Document, Placement, Shape, TextSpan};
use codedraw_core::{Theme, fs};
use proptest::prelude::*;

/// Code made of lines that mix ASCII, tabs, emoji, CJK and RTL text.
fn code_strategy() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        "[a-zA-Z0-9_ (){};=.,&*!<>\"'-]{0,24}",
        Just("\t".to_string()),
        Just("🔥".to_string()),
        Just("名前".to_string()),
        Just("שלום".to_string()),
        Just("".to_string()),
    ];
    prop::collection::vec(
        prop::collection::vec(piece, 0..6).prop_map(|v| v.concat()),
        0..20,
    )
    .prop_map(|lines| lines.join("\n"))
}

fn emphasis_of(e: u8) -> codedraw_core::model::Emphasis {
    use codedraw_core::model::Emphasis::*;
    [Fill, Underline, Brackets, Strike][e as usize % 4]
}

fn shape_strategy(lines: u32) -> impl Strategy<Value = Shape> {
    let line = 1..=lines.max(1);
    let col = 0u32..80;
    prop_oneof![
        (
            line.clone(),
            col.clone(),
            col.clone(),
            any::<bool>(),
            0u8..4
        )
            .prop_map(|(l, a, b, lbl, e)| Shape::Highlight {
                span: TextSpan {
                    line: l,
                    start: a.min(b),
                    end: a.max(b) + 1,
                },
                label: lbl.then(|| "hl label".to_string()),
                emphasis: emphasis_of(e),
            }),
        (line.clone(), col.clone(), 0u8..4).prop_map(|(l, c, e)| Shape::Mark {
            kind: "move".into(),
            style: Default::default(),
            span: TextSpan {
                line: l,
                start: c,
                end: c + 1,
            },
            label: Some("moves here".into()),
            emphasis: emphasis_of(e),
        }),
        (line.clone(), line.clone()).prop_map(|(a, b)| Shape::Box {
            from_line: a.min(b),
            to_line: a.max(b),
            label: Some("a box with a fairly long label to wrap".into()),
            frame: Default::default(),
        }),
        (line.clone(), col.clone(), line.clone(), col.clone()).prop_map(|(l1, c1, l2, c2)| {
            Shape::Arrow {
                from: Anchor { line: l1, col: c1 },
                to: Anchor { line: l2, col: c2 },
                label: Some("arrow".into()),
                line: Default::default(),
            }
        }),
        (line.clone(), "[a-z ]{0,120}").prop_map(|(l, t)| Shape::Note { line: l, text: t }),
        (line.clone(), line.clone(), "[a-z \n]{0,200}", any::<u8>()).prop_map(|(a, b, t, p)| {
            Shape::Callout {
                from_line: a.min(b),
                to_line: Some(a.max(b)),
                text: t,
                title: Some("title".into()),
                placement: match p % 3 {
                    0 => Placement::Margin,
                    1 => Placement::Below,
                    _ => Placement::Auto,
                },
                width: None,
                frame: Default::default(),
            }
        }),
        (line.clone(), line).prop_map(|(a, b)| Shape::Spotlight {
            from_line: Some(a.min(b)),
            to_line: Some(a.max(b)),
            ids: vec![],
            until: None,
        }),
    ]
}

fn doc_strategy() -> impl Strategy<Value = Document> {
    code_strategy().prop_flat_map(|code| {
        let d = Document::from_code(&code, "rs", None);
        let n = d.code.len() as u32;
        prop::collection::vec((shape_strategy(n), 1u32..5), 0..8).prop_map(move |shapes| {
            let mut d = d.clone();
            for (i, (shape, step)) in shapes.into_iter().enumerate() {
                // Shapes outside the text are rejected by `add`; that is fine.
                let _ = d.add(Annotation {
                    id: format!("p{i}"),
                    step,
                    color: None,
                    markers: None,
                    shape,
                });
            }
            d
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn anchors_never_panic(code in code_strategy(), line in 0u32..40, col in 0u32..200, needle in "[a-z ]{0,6}") {
        let d = Document::from_code(&code, "rs", None);
        let _ = d.line_text(line);
        let _ = d.line_len(line);
        let _ = d.find_text(&needle, 1, Some(line));
        let _ = d.validate_shape(&Shape::Highlight {
            span: TextSpan { line, start: col, end: col + 3 },
            label: None,
            emphasis: Default::default(),
        });
        prop_assert!(!d.code.is_empty(), "a document always has a line");
    }

    #[test]
    fn layout_never_overflows_or_panics(d in doc_strategy(), width in 20usize..=240, step in 0u32..6) {
        let th = Theme::dark();
        let l = layout(&d, &th, &LayoutOptions { width, step, focus: None, line_numbers: true, tab_width: 4 });
        for y in 0..l.canvas.height {
            let w = display_width(&l.canvas.row_text(y), 4);
            prop_assert!(w <= width, "row {y} is {w} cells wide at width {width}:\n{}", l.canvas.to_plain_text());
        }
        prop_assert!(l.rows.len() >= d.code.len());
    }

    #[test]
    fn documents_round_trip_through_json(d in doc_strategy()) {
        let json = serde_json::to_string(&d).unwrap();
        let back = fs::parse_document(&json, None).unwrap();
        prop_assert_eq!(serde_json::to_string(&back).unwrap(), json);
        prop_assert_eq!(back.code, d.code);
        prop_assert_eq!(back.annotations, d.annotations);
    }
}
