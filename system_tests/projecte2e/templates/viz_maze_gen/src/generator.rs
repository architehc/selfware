use crate::grid::Grid;

/// Simple pseudo-random number generator (xorshift64).
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Shuffle a slice in place using Fisher-Yates.
    fn shuffle<T>(&mut self, slice: &mut [T]) {
        // BUG 1: Only shuffles the first element — loop starts at 1
        // but the range is wrong, making it a no-op for small slices.
        // Should iterate from len-1 down to 1.
        for i in (1..slice.len()).rev() {
            let j = (self.next() as usize) % (i + 1);
            slice.swap(i, j);
        }
    }
}

/// Generate a maze using recursive backtracking (iterative with explicit stack).
///
/// Returns a Grid with carved passages (walls removed between connected cells).
pub fn generate(width: usize, height: usize, seed: u64) -> Grid {
    if width == 0 || height == 0 {
        return Grid::new(width, height);
    }

    let mut grid = Grid::new(width, height);
    let mut rng = Rng::new(seed);

    // BUG 2: visited tracking doesn't reset between calls.
    // (In this version, the grid is fresh each time, so this bug
    // manifests differently — the visited flag in Cell::default() starts as false,
    // but if generate is called on the same grid, it won't re-visit cells.)

    // Start at (0, 0)
    let mut stack: Vec<(usize, usize)> = Vec::new();
    grid.get_mut(0, 0).unwrap().visited = true;
    stack.push((0, 0));

    while let Some(&(row, col)) = stack.last() {
        let mut neighbors = grid.unvisited_neighbors(row, col);

        if neighbors.is_empty() {
            stack.pop();
            continue;
        }

        // BUG 3: Direction shuffling always picks the first neighbor
        // when there's only 1 neighbor (correct), but the shuffle bias
        // means certain directions are preferred, creating visually
        // non-uniform mazes. (The shuffle itself is actually correct
        // after fix, but the initial bias comes from the neighbor
        // ordering in unvisited_neighbors being deterministic.)
        rng.shuffle(&mut neighbors);

        let (nrow, ncol, wall) = neighbors[0];
        grid.remove_wall(row, col, wall);
        grid.get_mut(nrow, ncol).unwrap().visited = true;
        stack.push((nrow, ncol));
    }

    grid
}
