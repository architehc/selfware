use super::*;

fn permissive_safety() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

#[cfg(unix)]
#[tokio::test]
async fn regular_file_below_root_is_read_from_its_pinned_descriptor() {
    let temp = tempfile::Builder::new()
        .prefix(".primitive-safe-read-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), b"pub fn safe() {}\n").unwrap();

    let (text, bytes) =
        read_relative_regular(&root, Path::new("src/lib.rs"), &permissive_safety(), 1024)
            .await
            .unwrap();

    assert_eq!(text, "pub fn safe() {}\n");
    assert_eq!(bytes, b"pub fn safe() {}\n");
}

#[cfg(unix)]
#[tokio::test]
async fn leaf_symlink_outside_root_is_rejected() {
    use std::os::unix::fs::symlink;

    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let outside = outside_temp.path().join("secret.rs");
    std::fs::write(&outside, b"secret\n").unwrap();
    symlink(&outside, root.join("linked.rs")).unwrap();

    let error = read_relative_regular(&root, Path::new("linked.rs"), &permissive_safety(), 1024)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("artifact read refused linked.rs"));
}

#[cfg(unix)]
#[tokio::test]
async fn intermediate_directory_symlink_is_rejected() {
    use std::os::unix::fs::symlink;

    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    std::fs::write(outside_temp.path().join("secret.rs"), b"secret\n").unwrap();
    symlink(outside_temp.path(), root.join("linked-dir")).unwrap();

    let error = read_relative_regular(
        &root,
        Path::new("linked-dir/secret.rs"),
        &permissive_safety(),
        1024,
    )
    .await
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("artifact read refused directory component"));
}

#[cfg(unix)]
#[tokio::test]
async fn hard_link_to_an_outside_file_is_rejected() {
    let root_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-root-")
        .tempdir_in(".")
        .unwrap();
    let outside_temp = tempfile::Builder::new()
        .prefix(".primitive-safe-outside-")
        .tempdir_in(".")
        .unwrap();
    let root = std::fs::canonicalize(root_temp.path()).unwrap();
    let outside = outside_temp.path().join("secret.rs");
    std::fs::write(&outside, b"outside secret\n").unwrap();
    std::fs::hard_link(&outside, root.join("looks-local.rs")).unwrap();

    let error = read_relative_regular(
        &root,
        Path::new("looks-local.rs"),
        &permissive_safety(),
        1024,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("multiple hard links"), "{error}");
}
