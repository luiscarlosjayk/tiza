// Long lines on purpose: no margin is left for labels at 80 cols.
fn main() {
    let items: Vec<String> = vec!["alpha".into(), "beta".into()];
    let total: usize = items.iter().map(|s| s.len()).sum::<usize>();
    println!("{} items, {} bytes, first {:?}", items.len(), total);
}

fn takes(s: String) {
    println!("{s}");
}
