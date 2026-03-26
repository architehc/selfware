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
    pub fn set(&mut self, index: usize) {
        if index >= self.capacity {
            return;
        }
        let word = index / 64;
        let bit = index % 64;
        self.words[word] |= 1u64 << bit;
    }

    /// Clear bit at `index` to 0.
    pub fn clear(&mut self, index: usize) {
        if index >= self.capacity {
            return;
        }
        let word = index / 64;
        self.words[word] &= !(1u64 << (index % 64));
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
    pub fn union(&self, other: &BitSet) -> BitSet {
        let cap = self.capacity.max(other.capacity);
        let word_count = (cap + 63) / 64;
        let mut result = BitSet::new(cap);
        for i in 0..word_count {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = other.words.get(i).copied().unwrap_or(0);
            result.words[i] = a | b;
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
    pub fn iter_ones(&self) -> Vec<usize> {
        let mut result = Vec::new();
        for (word_idx, &word) in self.words.iter().enumerate() {
            let mut w = word;
            while w != 0 {
                let bit = w.trailing_zeros() as usize;
                result.push(word_idx * 64 + bit);
                w &= w - 1; // clear lowest set bit
            }
        }
        result
    }

    /// Toggle bit at `index` (flip 0 to 1, 1 to 0).
    pub fn toggle(&mut self, index: usize) {
        if index >= self.capacity {
            return;
        }
        let word = index / 64;
        let bit = index % 64;
        self.words[word] ^= 1u64 << bit;
    }

    /// Check if the bitset is empty (no bits set).
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Set all bits to 1 (up to capacity).
    pub fn fill(&mut self) {
        self.words.fill(!0u64);
        // Clear excess bits in the last word
        let excess = self.capacity % 64;
        if excess != 0 {
            let last = self.words.len() - 1;
            self.words[last] &= (1u64 << excess) - 1;
        }
    }

    /// Return the difference of two bitsets (A AND NOT B).
    pub fn difference(&self, other: &BitSet) -> BitSet {
        let cap = self.capacity.max(other.capacity);
        let word_count = (cap + 63) / 64;
        let mut result = BitSet::new(cap);
        for i in 0..word_count {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = other.words.get(i).copied().unwrap_or(0);
            result.words[i] = a & !b;
        }
        result
    }

    /// Return the symmetric difference (XOR) of two bitsets.
    pub fn symmetric_difference(&self, other: &BitSet) -> BitSet {
        let cap = self.capacity.max(other.capacity);
        let word_count = (cap + 63) / 64;
        let mut result = BitSet::new(cap);
        for i in 0..word_count {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = other.words.get(i).copied().unwrap_or(0);
            result.words[i] = a ^ b;
        }
        result
    }

    /// Check if this bitset is a subset of another.
    pub fn is_subset(&self, other: &BitSet) -> bool {
        let word_count = self.capacity.max(other.capacity) / 64 + 1;
        for i in 0..word_count {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = other.words.get(i).copied().unwrap_or(0);
            if (a & !b) != 0 {
                return false;
            }
        }
        true
    }

    /// Check if this bitset and another are disjoint (no common bits).
    pub fn is_disjoint(&self, other: &BitSet) -> bool {
        let word_count = self.capacity.max(other.capacity) / 64 + 1;
        for i in 0..word_count {
            let a = self.words.get(i).copied().unwrap_or(0);
            let b = other.words.get(i).copied().unwrap_or(0);
            if (a & b) != 0 {
                return false;
            }
        }
        true
    }

    /// Get the capacity of this bitset.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
