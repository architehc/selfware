use crate::grid::{Grid, WALL_NORTH, WALL_SOUTH, WALL_EAST, WALL_WEST};

/// Render a maze grid as ASCII art using box-drawing characters.
///
/// Each cell becomes a 3x2 character block:
/// ```text
/// +--+--+
/// |  |  |
/// +--+--+
/// |     |
/// +--+--+
/// ```
///
/// Where `+` is a corner, `--` is a horizontal wall, `|` is a vertical wall,
/// and spaces indicate open passages.
pub fn render_ascii(grid: &Grid) -> String {
    if grid.width == 0 || grid.height == 0 {
        return String::new();
    }

    let mut output = String::new();

    for row in 0..grid.height {
        // Top wall line: +--+--+
        for col in 0..grid.width {
            output.push('+');
            if grid.has_wall(row, col, WALL_NORTH) {
                // BUG 1: Uses wrong characters — should use "--" for horizontal walls
                // but uses "||" (vertical chars) instead, making the maze unreadable.
                output.push_str("||");
            } else {
                output.push_str("  ");
            }
        }
        output.push('+');
        output.push('\n');

        // Cell content line: |  |  |
        for col in 0..grid.width {
            if grid.has_wall(row, col, WALL_WEST) {
                output.push('|');
            } else {
                output.push(' ');
            }
            output.push_str("  "); // cell interior
        }
        // Right wall of last cell
        if grid.has_wall(row, grid.width - 1, WALL_EAST) {
            output.push('|');
        } else {
            output.push(' ');
        }
        output.push('\n');
    }

    // Bottom wall line
    for col in 0..grid.width {
        output.push('+');
        if grid.has_wall(grid.height - 1, col, WALL_SOUTH) {
            // BUG 2: Same wrong characters for bottom wall
            output.push_str("||");
        } else {
            output.push_str("  ");
        }
    }
    output.push('+');
    output.push('\n');

    output
}
