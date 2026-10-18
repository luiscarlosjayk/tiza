//! One pass over the text: lines, words and bytes.

#[derive(Clone, Copy, Default)]
pub struct Counts {
    pub lines: usize,
    pub words: usize,
    pub bytes: usize,
}

pub fn count(text: &str) -> Counts {
    let mut c = Counts {
        bytes: text.len(),
        ..Counts::default()
    };
    for line in text.lines() {
        c.lines += 1;
        c.words += line.split_whitespace().count();
    }
    c
}
