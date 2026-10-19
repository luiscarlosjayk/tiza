use std::env;

/// Reads `--max N` from the command line (10 when absent).
fn parse_args(args: &[String]) -> usize {
    let mut max = 10;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--max" && i + 1 < args.len() {
            max = args[i + 1].parse().unwrap_or(max);
            i += 1;
        }
        i += 1;
    }
    max
}

struct Stack {
    items: Vec<i64>,
    max: usize,
}

impl Stack {
    fn push(&mut self, value: i64) -> bool {
        if self.items.len() == self.max {
            return false;
        }
        self.items.push(value);
        true
    }

    fn pop(&mut self) -> Option<i64> {
        self.items.pop()
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let max = parse_args(&args[1..]);
    let mut stack = Stack { items: Vec::new(), max };
    for n in 1..=5 {
        stack.push(n * n);
    }
    let total: i64 = stack.items.iter().sum();
    println!("{} items, total {}", stack.items.len(), total);
    while let Some(top) = stack.pop() {
        println!("{top}");
    }
}
