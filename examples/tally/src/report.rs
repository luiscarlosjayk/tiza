//! Turns the counts into the table the user sees.

use crate::args::Sort;
use crate::count::Counts;

pub fn table(rows: &[(String, Counts)], sort: Sort) -> String {
    let mut rows = rows.to_vec();
    match sort {
        Sort::Name => rows.sort_by(|a, b| a.0.cmp(&b.0)),
        Sort::Words => rows.sort_by(|a, b| b.1.words.cmp(&a.1.words)),
    }
    let mut out = String::new();
    for (path, c) in &rows {
        out += &format!("{:>6} {:>6} {:>8}  {path}\n", c.lines, c.words, c.bytes);
    }
    out
}
