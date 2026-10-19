mod queue;

use queue::Queue;

fn main() {
    let mut q = Queue::new();
    for word in ["first", "second", "third"] {
        q.push(word.to_string());
    }
    while let Some(word) = q.pop() {
        println!("{word}");
    }
}
