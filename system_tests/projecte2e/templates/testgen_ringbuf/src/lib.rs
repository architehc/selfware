/// A fixed-capacity ring buffer (circular buffer).
pub struct RingBuffer<T> {
    buffer: Vec<Option<T>>,
    head: usize,    // next write position
    tail: usize,    // next read position
    len: usize,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(None);
        }
        Self { buffer, head: 0, tail: 0, len: 0, capacity }
    }

    /// Push an item. If full, overwrites the oldest item.
    pub fn push(&mut self, item: T) -> Option<T> {
        let overwritten = if self.len == self.capacity {
            let old = self.buffer[self.tail].take();
            self.tail = (self.tail + 1) % self.capacity;
            old
        } else {
            self.len += 1;
            None
        };
        self.buffer[self.head] = Some(item);
        self.head = (self.head + 1) % self.capacity;
        overwritten
    }

    /// Pop the oldest item.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let item = self.buffer[self.tail].take();
        self.tail = (self.tail + 1) % self.capacity;
        self.len -= 1;
        item
    }

    /// Peek at the oldest item without removing it.
    pub fn peek(&self) -> Option<&T> {
        if self.len == 0 { None } else { self.buffer[self.tail].as_ref() }
    }

    /// Number of items currently in the buffer.
    pub fn len(&self) -> usize { self.len }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool { self.len == 0 }

    /// Whether the buffer is full.
    pub fn is_full(&self) -> bool { self.len == self.capacity }

    /// The total capacity.
    pub fn capacity(&self) -> usize { self.capacity }

    /// Clear all items.
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Iterate over items from oldest to newest.
    pub fn iter(&self) -> RingBufferIter<'_, T> {
        RingBufferIter { buf: self, pos: self.tail, remaining: self.len }
    }

    /// Drain all items from oldest to newest.
    pub fn drain(&mut self) -> Vec<T> {
        let mut result = Vec::with_capacity(self.len);
        while let Some(item) = self.pop() {
            result.push(item);
        }
        result
    }

    /// Extend from an iterator. Overwrites oldest if full.
    pub fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push(item);
        }
    }
}

pub struct RingBufferIter<'a, T> {
    buf: &'a RingBuffer<T>,
    pos: usize,
    remaining: usize,
}

impl<'a, T> Iterator for RingBufferIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        let item = self.buf.buffer[self.pos].as_ref();
        self.pos = (self.pos + 1) % self.buf.capacity;
        self.remaining -= 1;
        item
    }
}

// NO TESTS - the agent must write them
