//! The first module: a stack, its lines long enough to push labels.
pub struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }
}
