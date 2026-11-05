//! Helpers of the tests that lay out a document, and a big document.

use std::path::PathBuf;

use serde_json::{Value, json};
use tiza_core::layout::{LayoutOptions, layout};

use crate::scratch;

/// `lines` lines of Rust with 16 labelled annotations, four per step
/// (boxes, notes, highlights and arrows on function headers).
pub fn big_doc(lines: usize) -> Value {
    let code: Vec<String> = (0..lines)
        .map(|i| match i % 6 {
            0 => format!("fn step_{i}(queue: &mut Vec<u64>, limit: usize) -> Option<u64> {{"),
            1 => format!(
                "    let value = queue.iter().copied().filter(|v| v % {} == 0).sum::<u64>();",
                i + 3
            ),
            2 => "    if queue.len() > limit { queue.truncate(limit); }".into(),
            3 => format!("    queue.push(value + {i});"),
            4 => "    queue.last().copied()".into(),
            _ => "}".into(),
        })
        .collect();
    let annotations: Vec<Value> = (0..16)
        .map(|j| {
            let step = j / 4 + 1;
            let line = 1 + 6 * ((j * (lines - 10)) / 16 / 6);
            match j % 4 {
                0 => json!({"id": format!("b{j}"), "step": step, "type": "box",
                    "from_line": line, "to_line": line + 4,
                    "label": format!("function number {j}: filters, truncates and pushes")}),
                1 => json!({"id": format!("n{j}"), "step": step, "type": "note", "line": line,
                    "text": format!("note {j}: the sum only keeps multiples of the line number")}),
                2 => json!({"id": format!("h{j}"), "step": step, "type": "highlight",
                    "span": {"line": line, "start": 4, "end": 20},
                    "label": format!("highlight {j} on the signature")}),
                _ => json!({"id": format!("a{j}"), "step": step, "type": "arrow",
                    "from": {"line": line, "col": 4}, "to": {"line": line + 3, "col": 4},
                    "label": format!("arrow {j}: value flows into the push")}),
            }
        })
        .collect();
    json!({
        "schema": 1,
        "title": format!("Big tutorial ({lines} lines)"),
        "source": {"language": "rust", "first_line": 1},
        "code": code,
        "annotations": annotations,
    })
}

/// The big document written as `big.tiza` in a scratch directory.
pub fn big_file(name: &str) -> PathBuf {
    let dir = scratch(name);
    let file = dir.join("big.tiza");
    std::fs::write(&file, serde_json::to_string_pretty(&big_doc(300)).unwrap()).unwrap();
    file
}

/// The document of the JSON `v`.
pub fn doc_of(v: &Value) -> tiza_core::Document {
    tiza_core::fs::parse_document(&v.to_string(), None).unwrap()
}

/// The layout `tiza render --step STEP --width WIDTH` draws of the whole
/// document.
pub fn lay(doc: &tiza_core::Document, width: usize, step: u32) -> tiza_core::layout::Layout {
    let opts = LayoutOptions {
        width,
        step,
        line_numbers: doc.numbers_or(true),
        margin_cap: doc.margin.map(usize::from),
        ..Default::default()
    };
    layout(doc, &tiza_core::Theme::dark(), &opts)
}
