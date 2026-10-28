//! Readers for the Markdown of the docs, for the tests that compare a page
//! with the code.

/// Each number N that a line of `md` gives as `before` N `after`: the line
/// (from 1) and N. The words around the number find it; they are not a
/// sentence that a test checks.
pub fn cited(md: &str, before: &str, after: &str) -> Vec<(usize, u64)> {
    assert!(!before.is_empty(), "give the words before the number");
    let mut out = Vec::new();
    for (i, line) in md.lines().enumerate() {
        let mut from = 0;
        while let Some(k) = line[from..].find(before) {
            let start = from + k + before.len();
            let digits = line[start..].bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && line[start + digits..].starts_with(after) {
                out.push((i + 1, line[start..start + digits].parse().unwrap()));
            }
            from = start;
        }
    }
    out
}
