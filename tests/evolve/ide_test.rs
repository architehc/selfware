use selfware::evolve::ide::{FileClass, IdeEngine, MISSING_DOCUMENT_SHA256};
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

#[test]
fn test_project_listing_is_recursive_deterministic_and_classified() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src/nested")).unwrap();
    std::fs::create_dir_all(dir.path().join("tests")).unwrap();
    std::fs::create_dir_all(dir.path().join("examples")).unwrap();
    std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
    std::fs::create_dir_all(dir.path().join("system_tests")).unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "pub mod nested;\n").unwrap();
    std::fs::write(dir.path().join("src/nested/mod.rs"), "pub fn run() {}\n").unwrap();
    std::fs::write(
        dir.path().join("src/nested/integration.rs"),
        "#[test] fn internal_works() {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("src/nested/mod.rs"),
        "pub fn run() {}\n#[cfg(test)]\nmod integration;\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("tests/integration.rs"),
        "#[test] fn works() {}\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("examples/demo.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.path().join("scripts/check.sh"), "echo check\n").unwrap();
    std::fs::write(dir.path().join("system_tests/flow.sh"), "echo test\n").unwrap();

    let engine = IdeEngine::for_project(dir.path());
    let first = engine.list_files().unwrap();
    let second = engine.list_files().unwrap();
    let first_paths: Vec<&str> = first.iter().map(|file| file.path.as_str()).collect();
    let second_paths: Vec<&str> = second.iter().map(|file| file.path.as_str()).collect();

    assert_eq!(first_paths, second_paths);
    assert!(first_paths.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(
        first
            .iter()
            .find(|file| file.path == "src/nested/mod.rs")
            .unwrap()
            .classification,
        FileClass::Production
    );
    assert_eq!(
        first
            .iter()
            .find(|file| file.path == "tests/integration.rs")
            .unwrap()
            .classification,
        FileClass::Test
    );
    assert_eq!(
        first
            .iter()
            .find(|file| file.path == "src/nested/integration.rs")
            .unwrap()
            .classification,
        FileClass::Test
    );
    assert_eq!(
        first
            .iter()
            .find(|file| file.path == "examples/demo.rs")
            .unwrap()
            .classification,
        FileClass::Example
    );
    assert_eq!(
        first
            .iter()
            .find(|file| file.path == "Cargo.toml")
            .unwrap()
            .classification,
        FileClass::Production
    );
    assert_eq!(
        first
            .iter()
            .find(|file| file.path == "scripts/check.sh")
            .unwrap()
            .classification,
        FileClass::Production
    );
    assert_eq!(
        first
            .iter()
            .find(|file| file.path == "system_tests/flow.sh")
            .unwrap()
            .classification,
        FileClass::Test
    );
}

#[test]
fn test_project_ide_uses_repository_source_policy_for_listing_and_edits() {
    let project = tempfile::tempdir().unwrap();
    for (path, content) in [
        ("src/lib.rs", "pub fn library() {}\n"),
        (
            "vscode-selfware/src/extension.ts",
            "export const active = true;\n",
        ),
        ("zed-extension/src/lib.rs", "pub fn extension() {}\n"),
        ("fuzz/fuzz_targets/parser.rs", "fn fuzz_target() {}\n"),
        ("workflows/review.swl", "workflow review {}\n"),
        ("rustfmt.toml", "edition = \"2021\"\n"),
        ("target/generated.rs", "pub fn generated() {}\n"),
        (
            "node_modules/pkg/index.ts",
            "export const dependency = 1;\n",
        ),
        ("vendor/copied.rs", "pub fn copied() {}\n"),
        ("selfware.toml", "api_key = \"private\"\n"),
        ("credentials.json", "{}\n"),
        ("README.md", "not an editable source document\n"),
    ] {
        let full = project.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(
            project.path().join("vscode-selfware/src/extension.ts"),
            project.path().join("src/symlink.ts"),
        )
        .unwrap();
    }

    let engine = IdeEngine::for_project(project.path());
    let files = engine.list_files().unwrap();
    let paths = files
        .iter()
        .filter(|file| !file.is_dir)
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    for included in [
        "src/lib.rs",
        "vscode-selfware/src/extension.ts",
        "zed-extension/src/lib.rs",
        "fuzz/fuzz_targets/parser.rs",
        "workflows/review.swl",
        "rustfmt.toml",
    ] {
        assert!(paths.contains(&included), "missing {included}");
    }
    for excluded in [
        "target/generated.rs",
        "node_modules/pkg/index.ts",
        "vendor/copied.rs",
        "selfware.toml",
        "credentials.json",
        "README.md",
        "src/symlink.ts",
    ] {
        assert!(!paths.contains(&excluded), "exposed {excluded}");
        assert!(engine.read_document(excluded).is_err());
    }
    assert_eq!(
        files
            .iter()
            .find(|file| file.path == "fuzz/fuzz_targets/parser.rs")
            .unwrap()
            .classification,
        FileClass::Test
    );

    let before = engine.read_document("rustfmt.toml").unwrap();
    engine
        .write_file_checked("rustfmt.toml", "edition = \"2024\"\n", &before.hash)
        .unwrap();
    assert_eq!(
        engine.read_file("rustfmt.toml").unwrap(),
        "edition = \"2024\"\n"
    );
    assert!(engine
        .write_file("node_modules/pkg/new.ts", "export const unsafe = true;\n")
        .is_err());
}

#[test]
fn test_document_snapshot_has_hash_lines_language_and_json_shape() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hello.rs"), "abc").unwrap();
    let engine = IdeEngine::new(dir.path());

    let snapshot = engine.read_document("hello.rs").unwrap();
    assert_eq!(
        snapshot.hash,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(snapshot.lines, 1);
    assert_eq!(snapshot.language, "rust");
    assert_eq!(snapshot.content, "abc");

    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["path"], "hello.rs");
    assert_eq!(json["hash"], snapshot.hash);
}

#[test]
fn test_limited_document_read_rejects_before_returning_oversized_source() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hello.rs"), "123456").unwrap();
    let engine = IdeEngine::new(dir.path());

    let error = engine.read_document_limited("hello.rs", 5).unwrap_err();

    assert!(error.to_string().contains("5-byte read limit"));
    assert_eq!(
        engine.read_document_limited("hello.rs", 6).unwrap().content,
        "123456"
    );
}

#[test]
fn test_graph_document_accepts_only_absolute_paths_inside_project() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    let source = project.path().join("src/lib.rs");
    std::fs::write(&source, "pub fn grounded() {}\n").unwrap();
    let ide = IdeEngine::for_project(project.path());

    let document = ide.read_graph_document(source.to_str().unwrap()).unwrap();
    assert_eq!(document.path, "src/lib.rs");
    assert!(ide.read_graph_document("/tmp/outside.rs").is_err());
}

#[test]
fn test_checked_write_rejects_stale_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.rs");
    std::fs::write(&path, "original\n").unwrap();
    let engine = IdeEngine::new(dir.path());
    let snapshot = engine.read_document("hello.rs").unwrap();

    std::fs::write(&path, "newer external edit\n").unwrap();
    let error = engine
        .write_file_checked("hello.rs", "stale browser edit\n", &snapshot.hash)
        .unwrap_err();

    assert!(error.to_string().contains("stale write rejected"));
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "newer external edit\n"
    );
    assert!(!dir.path().join(".selfware/evolve-checkpoints").exists());
}

#[test]
fn test_checked_write_creates_durable_before_image() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "before\n").unwrap();
    let engine = IdeEngine::for_project(dir.path());
    let snapshot = engine.read_document("src/lib.rs").unwrap();

    let result = engine
        .write_file_checked("src/lib.rs", "after\n", &snapshot.hash)
        .unwrap();

    assert!(!result.created);
    assert_eq!(
        result.previous_hash.as_deref(),
        Some(snapshot.hash.as_str())
    );
    assert_eq!(result.bytes_written, "after\n".len());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(),
        "after\n"
    );
    let checkpoint_path = PathBuf::from(&result.checkpoint_path);
    assert!(checkpoint_path.starts_with(dir.path().join(".selfware/evolve-checkpoints")));
    let checkpoint: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(checkpoint_path).unwrap()).unwrap();
    assert_eq!(checkpoint["path"], "src/lib.rs");
    assert_eq!(checkpoint["before"]["content"], "before\n");
    assert_eq!(checkpoint["before"]["hash"], snapshot.hash);

    let current = engine.read_document("src/lib.rs").unwrap();
    assert_eq!(current.hash, result.hash);
}

#[test]
fn test_checked_write_requires_missing_sentinel_for_creation() {
    let dir = tempfile::tempdir().unwrap();
    let engine = IdeEngine::new(dir.path());

    assert!(engine
        .write_file_checked("new.rs", "fn new() {}\n", "not-the-missing-sentinel")
        .is_err());
    let result = engine
        .write_file_checked("new.rs", "fn new() {}\n", MISSING_DOCUMENT_SHA256)
        .unwrap();

    assert!(result.created);
    assert!(result.previous_hash.is_none());
    assert_eq!(engine.read_file("new.rs").unwrap(), "fn new() {}\n");
}

#[cfg(unix)]
#[test]
fn test_ide_engine_rejects_final_and_parent_symlink_writes() {
    use std::os::unix::fs::symlink;

    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    let outside_file = outside.path().join("outside.rs");
    std::fs::write(&outside_file, "outside\n").unwrap();
    symlink(&outside_file, project.path().join("src/link.rs")).unwrap();
    symlink(outside.path(), project.path().join("src/link-dir")).unwrap();
    let engine = IdeEngine::for_project(project.path());

    assert!(engine.read_document("src/link.rs").is_err());
    assert!(engine
        .write_file_checked("src/link.rs", "escaped\n", MISSING_DOCUMENT_SHA256)
        .is_err());
    assert!(engine
        .write_file("src/link-dir/new.rs", "escaped\n")
        .is_err());
    assert_eq!(std::fs::read_to_string(&outside_file).unwrap(), "outside\n");
    assert!(!outside.path().join("new.rs").exists());
}
