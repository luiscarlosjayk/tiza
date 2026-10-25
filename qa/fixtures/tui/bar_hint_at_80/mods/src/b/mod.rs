//! The second module: a queue, in another directory, also mod.rs.
pub struct Queue<T> {
    items: std::collections::VecDeque<T>,
}

impl<T> Queue<T> {
    pub fn enqueue(&mut self, item: T) {
        self.items.push_back(item);
    }
}
