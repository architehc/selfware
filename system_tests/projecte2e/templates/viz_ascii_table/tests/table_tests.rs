use viz_ascii_table::{Alignment, Table};

#[test]
fn test_simple_table_structure() {
    let mut table = Table::new(&["Name", "Age"]);
    table.add_row(&["Alice", "30"]);
    let output = table.render();

    assert!(output.contains("┌"), "Must have top-left corner");
    assert!(output.contains("┐"), "Must have top-right corner");
    assert!(output.contains("└"), "Must have bottom-left corner");
    assert!(output.contains("┘"), "Must have bottom-right corner");
    assert!(output.contains("│"), "Must have vertical separators");
}

#[test]
fn test_horizontal_lines_use_box_chars() {
    let mut table = Table::new(&["A"]);
    table.add_row(&["x"]);
    let output = table.render();

    // Horizontal lines must use box-drawing char ─ (U+2500), not regular hyphen
    assert!(
        output.contains('─'),
        "Horizontal lines must use ─ (U+2500), not regular hyphens.\nGot:\n{}",
        output
    );
    assert!(
        !output.contains("---"),
        "Must NOT use regular hyphens for borders.\nGot:\n{}",
        output
    );
}

#[test]
fn test_column_width_fits_content() {
    let mut table = Table::new(&["Name", "Score"]);
    table.add_row(&["Bartholomew", "100"]);
    let output = table.render();

    // "Bartholomew" is 11 chars — the column must be at least that wide
    // Check that the full name appears without truncation
    assert!(
        output.contains("Bartholomew"),
        "Long content must not be truncated.\nGot:\n{}",
        output
    );

    // Each line should have consistent width, measured in CHARACTERS, not
    // bytes. Byte length is the wrong yardstick here: border lines are drawn
    // with box-drawing glyphs (3 UTF-8 bytes per char) while content lines
    // are mostly ASCII (1 byte per char), so a correctly rendered table with
    // ─ borders produces border lines of 69 bytes next to content lines of
    // 29 bytes — while every line is 23 chars. A byte-length comparison is
    // therefore a GUARANTEED false negative on correct output: it can never
    // pass, so every SAB baseline for this template was capped below 100
    // even when the rendered table was right (the measurement this suite
    // fixes). Character count is encoding-independent and is what
    // "consistent width" means for the rendered table; genuine corruption
    // (rows padded to different widths, truncated cells) still changes the
    // per-line char count and fails.
    let first_count = output
        .lines()
        .next()
        .map(|l| l.chars().count())
        .unwrap_or(0);
    for (i, line) in output.lines().enumerate() {
        assert_eq!(
            line.chars().count(),
            first_count,
            "Line {} has different width ({}) than line 0 ({})\nGot:\n{}",
            i,
            line.chars().count(),
            first_count,
            output
        );
    }
}

#[test]
fn test_right_alignment() {
    let mut table = Table::new(&["Item", "Price"]);
    table.set_alignments(vec![Alignment::Left, Alignment::Right]);
    table.add_row(&["Widget", "9.99"]);
    table.add_row(&["Gadget", "149.99"]);
    let output = table.render();

    // Find the line with "9.99" (but not "149.99")
    let short_price_line = output
        .lines()
        .find(|l| l.contains("9.99") && !l.contains("149.99"));

    assert!(
        short_price_line.is_some(),
        "Must have a line with 9.99.\nGot:\n{}",
        output
    );
    let line = short_price_line.unwrap();

    // Extract the Price cell (second data column, between second and third │)
    let parts: Vec<&str> = line.split('│').collect();
    assert!(
        parts.len() >= 3,
        "Line must have at least 3 segments split by │.\nLine: {}",
        line
    );
    let price_cell = parts[2]; // " 9.99  " or "  9.99 " depending on alignment

    // Right-aligned: "9.99" (4 chars) in a column wide enough for "149.99" (6 chars)
    // must have MORE leading spaces than trailing spaces.
    // For right-align: " __9.99 " (content pushed right)
    // For left-align:  " 9.99__ " (content pushed left) — this is the bug
    let leading_spaces = price_cell.len() - price_cell.trim_start().len();
    let trailing_spaces = price_cell.len() - price_cell.trim_end().len();

    assert!(
        leading_spaces > trailing_spaces,
        "Right-aligned '9.99' must have more leading spaces than trailing spaces.\n\
         Cell: {:?} (leading={}, trailing={})\nFull output:\n{}",
        price_cell,
        leading_spaces,
        trailing_spaces,
        output
    );
}

#[test]
fn test_center_alignment() {
    let mut table = Table::new(&["Title"]);
    table.set_alignments(vec![Alignment::Center]);
    table.add_row(&["Hi"]);
    table.add_row(&["Hello"]);
    let output = table.render();

    // "Hi" should be centered in a column wide enough for "Hello" (5 chars)
    assert!(output.contains("Hi"), "Must contain centered text");
    assert!(output.contains("Hello"), "Must contain text");
}

#[test]
fn test_multiple_rows() {
    let mut table = Table::new(&["ID", "Name", "Score"]);
    table.add_row(&["1", "Alice", "95"]);
    table.add_row(&["2", "Bob", "87"]);
    table.add_row(&["3", "Charlie", "92"]);
    let output = table.render();

    assert!(output.contains("Alice"), "Must contain Alice");
    assert!(output.contains("Bob"), "Must contain Bob");
    assert!(output.contains("Charlie"), "Must contain Charlie");

    // Should have exactly 7 lines: top, header, sep, 3 data rows, bottom
    let line_count = output.lines().count();
    assert_eq!(
        line_count, 7,
        "Expected 7 lines (top + header + sep + 3 rows + bottom), got {}\n{}",
        line_count, output
    );
}

#[test]
fn test_empty_table() {
    let table = Table::new(&[]);
    let output = table.render();
    assert!(
        output.is_empty(),
        "Empty table (no headers) should produce empty string"
    );
}

#[test]
fn test_header_separator() {
    let mut table = Table::new(&["A", "B"]);
    table.add_row(&["1", "2"]);
    let output = table.render();

    // Must have ├ and ┤ in the separator line
    assert!(
        output.contains('├'),
        "Header separator must use ├.\nGot:\n{}",
        output
    );
    assert!(
        output.contains('┤'),
        "Header separator must use ┤.\nGot:\n{}",
        output
    );
    assert!(
        output.contains('┼'),
        "Header separator must use ┼ between columns.\nGot:\n{}",
        output
    );
}

#[test]
fn test_single_column_table() {
    let mut table = Table::new(&["Value"]);
    table.add_row(&["42"]);
    let output = table.render();

    assert!(output.contains("42"), "Must contain the value");
    assert!(output.contains("Value"), "Must contain the header");

    // Should NOT have mid-separators (┬, ┼, ┴) since there's only one column
    assert!(
        !output.contains('┬'),
        "Single-column table must not have ┬.\nGot:\n{}",
        output
    );
    assert!(
        !output.contains('┼'),
        "Single-column table must not have ┼.\nGot:\n{}",
        output
    );
}

#[test]
fn test_unicode_content() {
    let mut table = Table::new(&["Greeting"]);
    table.add_row(&["Hello"]);
    table.add_row(&["Hola"]);
    let output = table.render();

    assert!(output.contains("Hello"), "Must handle ASCII content");
    assert!(output.contains("Hola"), "Must handle ASCII content");
}

// ── Content-aware table measurement ─────────────────────────────────────
//
// The harness for this template (SAB runs `cargo test`) scores a table by
// WHAT IT CONTAINS, not by its byte layout. Two renders of the same logical
// table may legitimately differ in alignment padding, column-width slack, or
// padding style; those are non-semantic bytes differences and must not fail
// the measurement. Wrong cell values, missing rows, or missing columns are
// semantic differences and MUST fail. `table_cells` parses a rendered table
// into its trimmed header + data rows, and `tables_match` compares two such
// parses — the normalization that turns "same content, different padding"
// into equal.

/// Parse a rendered table into its logical content: (headers, data rows).
/// Border lines (which contain no `│` separators) are skipped, and every
/// cell is trimmed of alignment padding. A table with no content parses to
/// empty vectors.
fn table_cells(output: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut headers: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in output.lines() {
        if !line.contains('│') {
            continue; // top / header-separator / bottom border line
        }
        let cells: Vec<String> = line
            .split('│')
            .filter(|cell| !cell.trim().is_empty())
            .map(|cell| cell.trim().to_string())
            .collect();
        if cells.is_empty() {
            continue;
        }
        if headers.is_empty() {
            headers = cells;
        } else {
            rows.push(cells);
        }
    }
    (headers, rows)
}

/// Content-aware equality: identical headers (in order) and identical data
/// rows (in order). Whitespace-only differences (alignment padding, unused
/// column slack) are normalized away; cell VALUES, row count, and column
/// count are compared exactly.
fn tables_match(actual: &str, expected: &str) -> bool {
    table_cells(actual) == table_cells(expected)
}

/// Canonical correct render of headers ["Name", "Age"] with row
/// ["Alice", "30"]: box-drawing borders, full-width cells, no truncation.
const REFERENCE_TABLE: &str = "┌───────┬─────┐\n\
                               │ Name  │ Age │\n\
                               ├───────┼─────┤\n\
                               │ Alice │ 30  │\n\
                               └───────┴─────┘\n";

#[test]
fn test_correct_table_matches_reference() {
    let mut table = Table::new(&["Name", "Age"]);
    table.add_row(&["Alice", "30"]);
    let rendered = table.render();
    assert!(
        tables_match(&rendered, REFERENCE_TABLE),
        "A correct table must measure as matching the reference.\nGot:\n{}",
        rendered
    );
}

#[test]
fn test_wrong_cell_value_fails() {
    let mut table = Table::new(&["Name", "Age"]);
    table.add_row(&["Alice", "31"]);
    let rendered = table.render();
    assert!(
        !tables_match(&rendered, REFERENCE_TABLE),
        "A table with a wrong cell value must NOT match.\nGot:\n{}",
        rendered
    );
}

#[test]
fn test_missing_row_fails() {
    let mut table = Table::new(&["Name", "Age"]);
    table.add_row(&["Alice", "30"]);
    let rendered = table.render();
    // Reference has a second row ["Bob", "40"] this render is missing.
    let with_row = "┌───────┬─────┐\n\
                    │ Name  │ Age │\n\
                    ├───────┼─────┤\n\
                    │ Alice │ 30  │\n\
                    │ Bob   │ 40  │\n\
                    └───────┴─────┘\n";
    assert!(
        !tables_match(&rendered, with_row),
        "A render missing a row must NOT match.\nGot:\n{}",
        rendered
    );
}

#[test]
fn test_padding_only_difference_passes() {
    // Same logical table as REFERENCE_TABLE, but with extra whitespace in
    // every cell (a correct renderer that over-pads its columns). This is
    // the exact case the byte-compare used to reject: identical content,
    // different bytes.
    let repadded = "┌─────────┬───────┐\n\
                    │  Name   │  Age  │\n\
                    ├─────────┼───────┤\n\
                    │  Alice  │  30   │\n\
                    └─────────┴───────┘\n";
    let mut table = Table::new(&["Name", "Age"]);
    table.add_row(&["Alice", "30"]);
    let rendered = table.render();
    assert!(
        tables_match(&rendered, repadded),
        "Whitespace/padding-only differences must be equivalent.\nGot:\n{}",
        rendered
    );
}
