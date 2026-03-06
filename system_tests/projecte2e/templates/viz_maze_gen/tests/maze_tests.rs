use viz_maze_gen::{Grid, Cell, generate, render_ascii, maze_to_ascii};
use viz_maze_gen::grid::*;

// ─── Grid basics ───

#[test]
fn test_grid_new_all_walls() {
    let grid = Grid::new(3, 3);
    // Every cell should start with all 4 walls
    for row in 0..3 {
        for col in 0..3 {
            let cell = grid.get(row, col).unwrap();
            assert_eq!(
                cell.walls, ALL_WALLS,
                "Cell ({},{}) should start with all walls (0b1111), got {:04b}",
                row, col, cell.walls
            );
        }
    }
}

#[test]
fn test_grid_remove_wall() {
    let mut grid = Grid::new(3, 3);
    // Initialize all walls first
    for row in 0..3 {
        for col in 0..3 {
            grid.get_mut(row, col).unwrap().walls = ALL_WALLS;
        }
    }

    grid.remove_wall(0, 0, WALL_EAST);

    // Cell (0,0) should not have east wall
    assert!(
        !grid.has_wall(0, 0, WALL_EAST),
        "Cell (0,0) should not have east wall after removal"
    );
    // Neighbor (0,1) should not have west wall
    assert!(
        !grid.has_wall(0, 1, WALL_WEST),
        "Cell (0,1) should not have west wall after removing east from (0,0)"
    );
}

#[test]
fn test_grid_remove_wall_south() {
    let mut grid = Grid::new(3, 3);
    for row in 0..3 {
        for col in 0..3 {
            grid.get_mut(row, col).unwrap().walls = ALL_WALLS;
        }
    }

    grid.remove_wall(0, 0, WALL_SOUTH);

    assert!(!grid.has_wall(0, 0, WALL_SOUTH));
    assert!(!grid.has_wall(1, 0, WALL_NORTH));
}

// ─── Maze generation ───

#[test]
fn test_generate_all_cells_visited() {
    let grid = generate(5, 5, 42);
    for row in 0..5 {
        for col in 0..5 {
            assert!(
                grid.get(row, col).unwrap().visited,
                "Cell ({},{}) should be visited after generation",
                row, col
            );
        }
    }
}

#[test]
fn test_generate_1x1() {
    let grid = generate(1, 1, 1);
    assert!(grid.get(0, 0).unwrap().visited);
}

#[test]
fn test_generate_different_seeds_different_mazes() {
    let maze1 = render_ascii(&generate(5, 5, 1));
    let maze2 = render_ascii(&generate(5, 5, 2));
    // Different seeds should (very likely) produce different mazes
    // This could theoretically fail but is extremely unlikely for 5x5
    assert_ne!(maze1, maze2, "Different seeds should produce different mazes");
}

#[test]
fn test_generate_same_seed_same_maze() {
    let maze1 = render_ascii(&generate(5, 5, 42));
    let maze2 = render_ascii(&generate(5, 5, 42));
    assert_eq!(maze1, maze2, "Same seed should produce identical maze");
}

// ─── ASCII rendering ───

#[test]
fn test_ascii_render_uses_correct_wall_chars() {
    let ascii = maze_to_ascii(3, 3, 42);
    // Horizontal walls must use "--", not "||"
    assert!(
        ascii.contains("--"),
        "ASCII maze must use '--' for horizontal walls.\nGot:\n{}",
        ascii
    );
    assert!(
        !ascii.contains("||"),
        "ASCII maze must NOT use '||' for horizontal walls.\nGot:\n{}",
        ascii
    );
}

#[test]
fn test_ascii_render_has_corners() {
    let ascii = maze_to_ascii(3, 3, 42);
    let plus_count = ascii.matches('+').count();
    // A 3x3 grid should have (3+1)*(3+1) = 16 corners
    assert_eq!(
        plus_count, 16,
        "3x3 maze should have 16 '+' corners, got {}.\n{}",
        plus_count, ascii
    );
}

#[test]
fn test_ascii_render_has_vertical_walls() {
    let ascii = maze_to_ascii(3, 3, 42);
    assert!(
        ascii.contains('|'),
        "Maze must have vertical wall characters '|'"
    );
}

#[test]
fn test_ascii_render_non_empty() {
    let ascii = maze_to_ascii(2, 2, 1);
    assert!(!ascii.is_empty(), "2x2 maze must produce non-empty output");
    let lines: Vec<&str> = ascii.lines().collect();
    // 2x2 grid: 3 wall lines + 2 cell lines = 5 lines
    assert_eq!(
        lines.len(), 5,
        "2x2 maze should have 5 lines (3 horizontal + 2 cell), got {}.\n{}",
        lines.len(), ascii
    );
}

#[test]
fn test_ascii_render_empty_grid() {
    let grid = Grid::new(0, 0);
    let ascii = render_ascii(&grid);
    assert!(ascii.is_empty());
}

#[test]
fn test_maze_has_path_between_cells() {
    // A valid maze should have at least one open passage (missing wall)
    let grid = generate(5, 5, 42);
    let mut has_open_passage = false;
    for row in 0..5 {
        for col in 0..5 {
            let cell = grid.get(row, col).unwrap();
            if cell.walls != ALL_WALLS {
                has_open_passage = true;
                break;
            }
        }
        if has_open_passage {
            break;
        }
    }
    assert!(
        has_open_passage,
        "Generated maze must have at least one open passage"
    );
}

// ─── Border integrity ───

#[test]
fn test_maze_outer_walls_intact() {
    let grid = generate(4, 4, 99);
    // Top row should have north walls
    for col in 0..4 {
        assert!(
            grid.has_wall(0, col, WALL_NORTH),
            "Top row cell ({},{}) must have north wall",
            0, col
        );
    }
    // Bottom row should have south walls
    for col in 0..4 {
        assert!(
            grid.has_wall(3, col, WALL_SOUTH),
            "Bottom row cell ({},{}) must have south wall",
            3, col
        );
    }
    // Left column should have west walls
    for row in 0..4 {
        assert!(
            grid.has_wall(row, 0, WALL_WEST),
            "Left column cell ({},{}) must have west wall",
            row, 0
        );
    }
    // Right column should have east walls
    for row in 0..4 {
        assert!(
            grid.has_wall(row, 3, WALL_EAST),
            "Right column cell ({},{}) must have east wall",
            row, 3
        );
    }
}
