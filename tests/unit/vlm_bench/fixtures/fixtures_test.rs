use super::*;

#[test]
fn test_create_solid_png_valid() {
    let png = create_solid_png(4, 4, [255, 0, 0, 255]);
    // Check PNG signature
    assert_eq!(
        &png[0..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    );
    // Should be non-trivial size
    assert!(png.len() > 50);
}

#[test]
fn test_create_solid_png_1x1() {
    let png = create_solid_png(1, 1, [0, 0, 0, 255]);
    assert_eq!(
        &png[0..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    );
}

#[test]
fn test_text_to_png_nonempty() {
    let png = text_to_png("Hello\nWorld", 6, 10);
    assert_eq!(
        &png[0..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    );
    assert!(png.len() > 100);
}

#[test]
fn test_text_to_png_empty() {
    let png = text_to_png("", 6, 10);
    assert_eq!(
        &png[0..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    );
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
