/// A fixed-capacity bitset backed by a Vec<u64>.
#[derive(Debug, Clone)]
pub struct BitSet {
    /// Storage words — each u64 holds 64 bits.
    words: Vec<u64>,
    /// Total number of bits this set can hold.
    capacity: usize,
}

impl BitSet {
    /// Create a new bitset that can hold `capacity` bits (all initially clear).
    pub fn new(capacity: usize) -> Self {
        let word_count = (capacity + 63) / 64;
        Self {
            words: vec![0u64; word_count],
            capacity,
        }
    }

    /// Set bit at `index` to 1.
    ///
    /// BUG: uses wrong mask — shifts by `index` instead of `index % 64`.
    pub fn set(&mut self, index: usize) {
        if index >= self.capacity {
            return;
        }
        let word = index / 64;
        self.words[word] |= 1u64 << index; // BUG: should be index % 64
    }

    /// Clear bit at `index` to 0.
    ///
    /// BUG: inverted logic — sets the bit instead of clearing it.
    pub fn clear(&mut self, index: usize) {
        if index >= self.capacity {
            return;
        }
        let word = index / 64;
        self.words[word] |= !(1u64 << (index % 64)); // BUG: should be &= !(...), not |= !(...)
    }

    /// Test whether bit at `index` is set.
    pub fn get(&self, index: usize) -> bool {
        if index >= self.capacity {
            return false;
        }
        let word = index / 64;
        (self.words[word] & (1u64 << (index % 64))) != 0
    }

    /// Count number of set bits.
    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Return the union of two bitsets (OR).
    ///
    /// BUG: uses AND instead of OR.
    pub fn union(&self, other: &BitSet) -> BitSet {
        let cap = self.capacity.max(other.capacity);
        let word_count = (cap + 63) / 64;
        let mut result = BitSet::new(cap);
        for i in 0..word_count {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = other.words.get(i).copied().unwrap_or(0);
            result.words[i] = a & b; // BUG: should be a | b
        }
        result
    }

    /// Return the intersection of two bitsets (AND).
    pub fn intersection(&self, other: &BitSet) -> BitSet {
        let cap = self.capacity.max(other.capacity);
        let word_count = (cap + 63) / 64;
        let mut result = BitSet::new(cap);
        for i in 0..word_count {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = other.words.get(i).copied().unwrap_or(0);
            result.words[i] = a & b;
        }
        result
    }

    /// Iterator over all set bit indices.
    ///
    /// BUG: skips the first word entirely (starts at word index 1).
    pub fn iter_ones(&self) -> Vec<usize> {
        let mut result = Vec::new();
        for (word_idx, &word) in self.words.iter().enumerate().skip(1) { // BUG: skip(1) should be skip(0)
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                result.push(word_idx * 64 + bit);
                w &= w - 1; // clear lowest set bit
            }
        }
        result
    }
}
