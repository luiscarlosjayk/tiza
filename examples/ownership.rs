fn main() {
    let s = String::from("hello");
    takes(s);
    // println!("{s}"); // error: borrow of moved value

    let t = String::from("world");
    let n = len(&t);
    println!("{t} has {n} letters");

    let mut u = String::from("hey");
    push(&mut u);
    println!("{u}");
}

fn takes(s: String) {
    println!("{s}");
}

fn len(s: &String) -> usize {
    s.len()
}

fn push(s: &mut String) {
    s.push('!');
}
