// Too long for labels at 60 columns.
fn main() {
    let items = vec!["alpha", "beta", "gamma"];
    let n: usize = items.iter().map(|s| s.len())
        .sum();
    println!("{} items, {n} bytes", items.len());
}

fn takes(s: String) {
    println!("{s}");
}
