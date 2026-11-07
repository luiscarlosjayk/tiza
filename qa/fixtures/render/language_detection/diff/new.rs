fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

fn main() {
    let who = "Ferris";
    println!("{}", greet(who));
}
