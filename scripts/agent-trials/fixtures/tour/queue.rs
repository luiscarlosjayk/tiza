use std::collections::VecDeque;

/// First in, first out.
pub struct Queue {
    items: VecDeque<String>,
}

impl Queue {
    pub fn new() -> Self {
        Queue { items: VecDeque::new() }
    }

    pub fn push(&mut self, item: String) {
        self.items.push_back(item);
    }

    pub fn pop(&mut self) -> Option<String> {
        self.items.pop_front()
    }
}
