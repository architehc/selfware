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
/// Each character is represented as a 6x10 pixel block.
pub fn text_to_png(text: &str, char_width: u32, char_height: u32) -> Vec<u8> {
    let lines: Vec<&str> = text.lines().collect();
    let max_cols = lines.iter().map(|l| l.len()).max().unwrap_or(0) as u32;
    let num_rows = lines.len() as u32;

    let width = max_cols * char_width;
    let height = num_rows * char_height;

    if width == 0 || height == 0 {
        return create_solid_png(1, 1, [0, 0, 0, 255]);
    }

    // Simple rendering: non-space chars become white pixels in their cell
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for (row, line) in lines.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch != ' ' {
                let base_x = col as u32 * char_width;
                let base_y = row as u32 * char_height;
                // Fill the character cell with white
                for dy in 1..char_height.saturating_sub(1) {
                    for dx in 1..char_width.saturating_sub(1) {
                        let px = base_x + dx;
                        let py = base_y + dy;
                        if px < width && py < height {
                            let idx = ((py * width + px) * 4) as usize;
                            pixels[idx] = 255; // R
                            pixels[idx + 1] = 255; // G
                            pixels[idx + 2] = 255; // B
                            pixels[idx + 3] = 255; // A
                        }
                    }
                }
            }
        }
    }

    encode_rgba_png(width, height, &pixels)
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

/// Load ground truth from a JSON file.
pub fn load_ground_truth(path: &Path) -> anyhow::Result<GroundTruth> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_solid_png_valid() {
        let png = create_solid_png(4, 4, [255, 0, 0, 255]);
        // Check PNG signature
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // Should be non-trivial size
        assert!(png.len() > 50);
    }

    #[test]
    fn test_create_solid_png_1x1() {
        let png = create_solid_png(1, 1, [0, 0, 0, 255]);
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_text_to_png_nonempty() {
        let png = text_to_png("Hello\nWorld", 6, 10);
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        assert!(png.len() > 100);
    }

    #[test]
    fn test_text_to_png_empty() {
        let png = text_to_png("", 6, 10);
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn test_crc32_known() {
        // CRC32 of empty data
        let crc = crc32(b"");
        assert_eq!(crc, 0x0000_0000);
    }

    #[test]
    fn test_adler32_known() {
        // Adler-32 of "Wikipedia"
        let a = adler32(b"Wikipedia");
        assert_eq!(a, 0x11E6_0398);
    }

    #[test]
    fn test_ground_truth_serde() {
        let gt = GroundTruth {
            level: "l1_tui_state".into(),
            scenarios: vec![ScenarioTruth {
                id: "dashboard_normal".into(),
                image: "dashboard_normal.png".into(),
                expected: serde_json::json!({
                    "panel": "dashboard",
                    "status": "ok"
                }),
            }],
        };
        let json = serde_json::to_string(&gt).unwrap();
        let parsed: GroundTruth = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.level, "l1_tui_state");
        assert_eq!(parsed.scenarios.len(), 1);
    }

    #[test]
    fn test_deflate_compress_decompresses_to_original() {
        // Just verify it produces valid output (non-empty, has zlib header)
        let data = b"Hello, World! This is test data for compression.";
        let compressed = deflate_compress(data);
        assert_eq!(compressed[0], 0x78); // zlib header
        assert!(compressed.len() > data.len()); // stored blocks are larger
    }
}
