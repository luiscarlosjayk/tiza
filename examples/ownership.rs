fn main() {
    let s = String::from("hola");
    takes(s);
    // println!("{s}"); // error: value borrowed here after move

    let t = String::from("mundo");
    let n = len(&t);
    println!("{t} tiene {n} letras");

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
