pub mod grid;
pub mod generator;
pub mod ascii;

pub use grid::{Grid, Cell};
pub use generator::generate;
pub use ascii::render_ascii;

/// Generate a maze and render it as ASCII art.
///
/// Returns a string with box-drawing characters representing the maze.
/// `width` and `height` are the number of cells (not characters).
pub fn maze_to_ascii(width: usize, height: usize, seed: u64) -> String {
    let grid = generate(width, height, seed);
    render_ascii(&grid)
}
