pub struct Queue {
    items: Vec<u32>,
}

impl Queue {
    pub fn new() -> Self {
        Queue { items: Vec::new() }
    }
}
