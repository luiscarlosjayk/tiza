struct Stack {
    items: Vec<i32>,
}

impl Stack {
    fn new() -> Self {
        Self { items: Vec::new() }
    }
}

struct Queue {
    items: Vec<i32>,
}

impl Queue {
    fn new() -> Self {
        Self { items: Vec::new() }
    }
}
