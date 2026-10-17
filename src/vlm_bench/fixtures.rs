//! Fixture generation helpers for VLM benchmarks.
//!
//! Provides utilities for creating test images programmatically,
//! rendering text to simple images, and managing fixture directories.

use std::path::{Path, PathBuf};

/// Metadata for a ground-truth fixture set.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroundTruth {
    /// Fixture set identifier (e.g., "l1_tui_state").
    pub level: String,
    /// Individual scenario ground truths.
    pub scenarios: Vec<ScenarioTruth>,
}

/// Ground truth for a single scenario.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScenarioTruth {
    /// Scenario identifier.
    pub id: String,
    /// Image filename (relative to level directory).
    pub image: String,
    /// Expected keywords or structured answers.
    pub expected: serde_json::Value,
}

/// Create a simple solid-color PNG image for testing.
///
/// Uses raw PNG encoding (no external image crate needed for basic fixtures).
pub fn create_solid_png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    use std::io::Write;

    let mut buf = Vec::new();

    // PNG signature
    buf.write_all(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        .unwrap();

    // IHDR chunk
    let mut ihdr_data = Vec::new();
    ihdr_data.extend_from_slice(&width.to_be_bytes());
    ihdr_data.extend_from_slice(&height.to_be_bytes());
    ihdr_data.push(8); // bit depth
    ihdr_data.push(6); // color type: RGBA
    ihdr_data.push(0); // compression
    ihdr_data.push(0); // filter
    ihdr_data.push(0); // interlace
    write_png_chunk(&mut buf, b"IHDR", &ihdr_data);

    // IDAT chunk — uncompressed deflate of raw pixel data
    let mut raw_data = Vec::new();
    for _y in 0..height {
        raw_data.push(0); // filter byte: None
        for _x in 0..width {
            raw_data.extend_from_slice(&rgba);
        }
    }
    let compressed = deflate_compress(&raw_data);
    write_png_chunk(&mut buf, b"IDAT", &compressed);

    // IEND chunk
    write_png_chunk(&mut buf, b"IEND", &[]);

    buf
}

/// Render a text block as a simple monochrome PNG (white text on black).
///
/// Each character is rendered using a deterministic 5x7 bitmap glyph scaled to
/// `(char_width, char_height)` so distinct characters produce distinct pixel
/// patterns suitable for VLM visual benchmarks.
pub fn text_to_png(text: &str, char_width: u32, char_height: u32) -> Vec<u8> {
    let lines: Vec<&str> = text.lines().collect();
    let max_cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u32;
    let num_rows = lines.len() as u32;

    let width = max_cols * char_width;
    let height = num_rows * char_height;

    if width == 0 || height == 0 {
        return create_solid_png(1, 1, [0, 0, 0, 255]);
    }

    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for px in pixels.chunks_exact_mut(4) {
        px[3] = 255; // Opaque black background
    }

    let inner_w = char_width.saturating_sub(1).max(1);
    let inner_h = char_height.saturating_sub(2).max(1);
    let pad_y = if char_height >= 3 { 1 } else { 0 };

    for (row, line) in lines.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let glyph = glyph_5x7(ch);
            let base_x = col as u32 * char_width;
            let base_y = row as u32 * char_height + pad_y;

            for dy in 0..inner_h {
                let gy = ((dy * 7) / inner_h).min(6) as usize;
                let row_bits = glyph[gy];
                for dx in 0..inner_w {
                    let gx = ((dx * 5) / inner_w).min(4);
                    if (row_bits >> (4 - gx)) & 1 == 1 {
                        let px = base_x + dx;
                        let py = base_y + dy;
                        if px < width && py < height {
                            let idx = ((py * width + px) * 4) as usize;
                            pixels[idx] = 255;
                            pixels[idx + 1] = 255;
                            pixels[idx + 2] = 255;
                            pixels[idx + 3] = 255;
                        }
                    }
                }
            }
        }
    }

    encode_rgba_png(width, height, &pixels)
}

/// Deterministic 5x7 bitmap glyph (each byte holds 5 bits in positions 4..=0).
fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' | '0' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' | '5' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' | '2' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b10010, 0b10010, 0b10010, 0b11111, 0b00010, 0b00010, 0b00010,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '-' | '─' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '|' | '│' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        '┌' => [
            0b00000, 0b00000, 0b00000, 0b00111, 0b00100, 0b00100, 0b00100,
        ],
        '┐' => [
            0b00000, 0b00000, 0b00000, 0b11100, 0b00100, 0b00100, 0b00100,
        ],
        '└' => [
            0b00100, 0b00100, 0b00100, 0b00111, 0b00000, 0b00000, 0b00000,
        ],
        '┘' => [
            0b00100, 0b00100, 0b00100, 0b11100, 0b00000, 0b00000, 0b00000,
        ],
        '├' => [
            0b00100, 0b00100, 0b00100, 0b00111, 0b00100, 0b00100, 0b00100,
        ],
        '┤' => [
            0b00100, 0b00100, 0b00100, 0b11100, 0b00100, 0b00100, 0b00100,
        ],
        '┬' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00100, 0b00100, 0b00100,
        ],
        '┴' => [
            0b00100, 0b00100, 0b00100, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '+' | '┼' => [
            0b00100, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00100,
        ],
        '█' => [
            0b11111, 0b11111, 0b11111, 0b11111, 0b11111, 0b11111, 0b11111,
        ],
        '░' => [
            0b10101, 0b01010, 0b10101, 0b01010, 0b10101, 0b01010, 0b10101,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        '/' => [
            0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        '[' => [
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ],
        ']' => [
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ],
        '✓' => [
            0b00000, 0b00001, 0b00010, 0b10100, 0b01000, 0b00000, 0b00000,
        ],
        '✗' => [
            0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b00000, 0b00000,
        ],
        '●' | '•' => [
            0b00000, 0b01110, 0b11111, 0b11111, 0b01110, 0b00000, 0b00000,
        ],
        '⚠' | '!' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100, 0b00000,
        ],
        other => {
            // Deterministic hash-derived glyph for any other character so distinct
            // codepoints still render distinct patterns.
            let mut rows = [0u8; 7];
            let code = other as u32;
            for (i, row) in rows.iter_mut().enumerate() {
                let mixed = code.wrapping_mul(31).wrapping_add((i as u32 + 1) * 17);
                *row = ((mixed & 0b11111) as u8) | 0b00100;
            }
            rows
        }
    }
}

/// Encode raw RGBA pixel data as a PNG.
fn encode_rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    use std::io::Write;

    let mut buf = Vec::new();

    // PNG signature
    buf.write_all(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        .unwrap();

    // IHDR
    let mut ihdr_data = Vec::new();
    ihdr_data.extend_from_slice(&width.to_be_bytes());
    ihdr_data.extend_from_slice(&height.to_be_bytes());
    ihdr_data.push(8);
    ihdr_data.push(6);
    ihdr_data.push(0);
    ihdr_data.push(0);
    ihdr_data.push(0);
    write_png_chunk(&mut buf, b"IHDR", &ihdr_data);

    // IDAT
    let mut raw = Vec::new();
    let row_bytes = (width * 4) as usize;
    for y in 0..height as usize {
        raw.push(0); // filter: None
        let start = y * row_bytes;
        let end = start + row_bytes;
        if end <= pixels.len() {
            raw.extend_from_slice(&pixels[start..end]);
        } else {
            raw.extend(std::iter::repeat_n(0, row_bytes));
        }
    }
    let compressed = deflate_compress(&raw);
    write_png_chunk(&mut buf, b"IDAT", &compressed);

    // IEND
    write_png_chunk(&mut buf, b"IEND", &[]);

    buf
}

/// Write a PNG chunk with CRC.
fn write_png_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    use std::io::Write;
    buf.write_all(&(data.len() as u32).to_be_bytes()).unwrap();
    buf.write_all(chunk_type).unwrap();
    buf.write_all(data).unwrap();

    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    let crc = crc32(&crc_data);
    buf.write_all(&crc.to_be_bytes()).unwrap();
}

/// Minimal CRC-32 for PNG chunks.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Minimal deflate compression (stored blocks, no actual compression).
///
/// Wraps raw data in a valid zlib stream with stored blocks.
fn deflate_compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();

    // Zlib header: CM=8 (deflate), CINFO=7 (32K window)
    out.push(0x78);
    out.push(0x01); // FCHECK for CMF=0x78

    // Split into stored blocks of max 65535 bytes
    let chunks: Vec<&[u8]> = data.chunks(65535).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        out.push(if is_last { 0x01 } else { 0x00 }); // BFINAL + BTYPE=00 (stored)
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes()); // NLEN
        out.extend_from_slice(chunk);
    }

    // Adler-32 checksum
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());

    out
}

/// Adler-32 checksum for zlib.
fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Ensure a fixture directory exists and return its path.
pub fn ensure_fixture_dir(base: &Path, level: &str) -> std::io::Result<PathBuf> {
    let dir = base.join(level);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
#[path = "../../tests/unit/vlm_bench/fixtures/fixtures_test.rs"]
mod tests;
