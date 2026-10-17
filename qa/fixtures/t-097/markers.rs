fn main() {
    // begin
    let s = String::from("hello");
    takes(s);
    // end
    let t = 1; // begin again
}

fn takes(s: String) {
    println!("{s}");
}

fn len(s: &String) -> usize { s.len() } // begin
