use selfware::evolve::ide::IdeEngine;
use std::path::PathBuf;

#[test]
fn test_ide_engine_lists_src_files() {
    let engine = IdeEngine::new("src");
    let files = engine.list_files().unwrap();
    assert!(files.iter().any(|f| f.path == "src/lib.rs"));
}

#[test]
fn test_ide_engine_reads_file_with_and_without_prefix() {
    let engine = IdeEngine::new("src");
    let with_prefix = engine.read_file("src/lib.rs").unwrap();
    let without_prefix = engine.read_file("lib.rs").unwrap();
    assert_eq!(with_prefix, without_prefix);
    assert!(with_prefix.contains("pub mod evolve;"));
}

#[test]
fn test_ide_engine_read_missing_file_errors() {
    let engine = IdeEngine::new("src");
    assert!(engine.read_file("src/definitely-not-here.rs").is_err());
}

#[test]
fn test_ide_engine_rejects_path_traversal() {
    let engine = IdeEngine::new("src");
    assert!(engine.read_file("../Cargo.toml").is_err());
    assert!(engine.read_file("src/../Cargo.toml").is_err());
    assert!(engine.read_file("../../etc/hosts").is_err());
    // Legitimate paths still work.
    assert!(engine.read_file("src/lib.rs").is_ok());
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("selfware-ide-test-{}", name));
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn test_ide_engine_write_file_round_trip() {
    let root = temp_root("write-round-trip");
    let engine = IdeEngine::new(&root);
    engine.write_file("hello.rs", "fn main() {}\n").unwrap();
    assert_eq!(engine.read_file("hello.rs").unwrap(), "fn main() {}\n");
    // Overwrite works too.
    engine
        .write_file("hello.rs", "fn main() { todo!() }\n")
        .unwrap();
    assert_eq!(
        engine.read_file("hello.rs").unwrap(),
        "fn main() { todo!() }\n"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn test_ide_engine_write_file_rejects_path_traversal() {
    let root = temp_root("write-traversal");
    let engine = IdeEngine::new(&root);
    assert!(engine.write_file("../escape.txt", "nope").is_err());
    assert!(engine.write_file("../../escape.txt", "nope").is_err());
    // `..` after a non-existent intermediate component must not bypass
    // the ancestor-canonicalization guard.
    assert!(engine.write_file("foo/../../escaped.txt", "nope").is_err());
    assert!(!root.parent().unwrap().join("escaped.txt").exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn test_ide_engine_write_file_creates_parent_dirs() {
    let root = temp_root("write-parents");
    let engine = IdeEngine::new(&root);
    engine
        .write_file("sub/dir/new.rs", "pub fn x() {}\n")
        .unwrap();
    assert_eq!(
        engine.read_file("sub/dir/new.rs").unwrap(),
        "pub fn x() {}\n"
    );
    std::fs::remove_dir_all(&root).ok();
}
