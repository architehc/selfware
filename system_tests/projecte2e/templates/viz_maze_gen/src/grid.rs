/// Walls are represented as a bitfield.
pub const WALL_NORTH: u8 = 0b0001;
pub const WALL_SOUTH: u8 = 0b0010;
pub const WALL_EAST:  u8 = 0b0100;
pub const WALL_WEST:  u8 = 0b1000;
pub const ALL_WALLS:  u8 = 0b1111;

/// A single cell in the maze grid.
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    /// Bitfield of walls present (WALL_NORTH | WALL_SOUTH | WALL_EAST | WALL_WEST).
    pub walls: u8,
    /// Whether this cell has been visited during generation.
    pub visited: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            // BUG 1: Cells start with NO walls instead of ALL walls.
            // The maze generator removes walls as it carves passages.
            // Starting with no walls means the maze is fully open.
            walls: 0,
            visited: false,
        }
    }
}

/// A 2D grid of maze cells.
#[derive(Debug, Clone)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Vec<Cell>>,
}

impl Grid {
    /// Create a new grid with all walls present.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![vec![Cell::default(); width]; height],
        }
    }

    /// Get a reference to a cell.
    pub fn get(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.get(row).and_then(|r| r.get(col))
    }

    /// Get a mutable reference to a cell.
    pub fn get_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(row).and_then(|r| r.get_mut(col))
    }

    /// Check if cell has a specific wall.
    pub fn has_wall(&self, row: usize, col: usize, wall: u8) -> bool {
        self.get(row, col).map(|c| c.walls & wall != 0).unwrap_or(true)
    }

    /// Remove a wall between two adjacent cells.
    pub fn remove_wall(&mut self, row: usize, col: usize, wall: u8) {
        if let Some(cell) = self.get_mut(row, col) {
            // BUG 2: Uses OR (|=) to "remove" a wall — this SETS the bit
            // instead of clearing it. Should use &= !wall to clear.
            cell.walls |= wall;
        }

        // Remove the corresponding wall from the neighbor
        let (nrow, ncol, opposite) = match wall {
            WALL_NORTH if row > 0 => (row - 1, col, WALL_SOUTH),
            WALL_SOUTH => (row + 1, col, WALL_NORTH),
            WALL_EAST => (row, col + 1, WALL_WEST),
            WALL_WEST if col > 0 => (row, col - 1, WALL_EAST),
            _ => return,
        };

        if let Some(neighbor) = self.get_mut(nrow, ncol) {
            neighbor.walls |= opposite;
        }
    }

    /// Get unvisited neighbors of a cell.
    pub fn unvisited_neighbors(&self, row: usize, col: usize) -> Vec<(usize, usize, u8)> {
        let mut neighbors = Vec::new();

        // BUG 3: Wraps around edges — doesn't check boundary for south and east.
        // row + 1 could equal height, col + 1 could equal width.
        // Only north and west check bounds (row > 0, col > 0).
        if row > 0 {
            if let Some(cell) = self.get(row - 1, col) {
                if !cell.visited {
                    neighbors.push((row - 1, col, WALL_NORTH));
                }
            }
        }
        // Missing: row + 1 < self.height check
        if let Some(cell) = self.get(row + 1, col) {
            if !cell.visited {
                neighbors.push((row + 1, col, WALL_SOUTH));
            }
        }
        if col > 0 {
            if let Some(cell) = self.get(row, col - 1) {
                if !cell.visited {
                    neighbors.push((row, col - 1, WALL_WEST));
                }
            }
        }
        // Missing: col + 1 < self.width check
        if let Some(cell) = self.get(row, col + 1) {
            if !cell.visited {
                neighbors.push((row, col + 1, WALL_EAST));
            }
        }

        neighbors
    }
}
