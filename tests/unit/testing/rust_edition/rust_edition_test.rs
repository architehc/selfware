use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn crate_with(dir: &Path, manifest: &str) -> PathBuf {
    write(&dir.join("Cargo.toml"), manifest);
    let file = dir.join("src").join("lib.rs");
    write(&file, "pub async fn f() {}\n");
    file
}

#[test]
fn package_edition_2018_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    let file = crate_with(
        tmp.path(),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2018\"\n",
    );
    let r = resolve_rust_edition(&file);
    assert_eq!(r.edition, "2018");
    assert_eq!(
        r.source,
        EditionSource::CargoPackage(tmp.path().join("Cargo.toml"))
    );
    assert!(!r.is_fallback());
}

#[test]
fn package_edition_2024_is_read_from_nested_module() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join("Cargo.toml"),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    let file = tmp.path().join("src/deep/nested/m.rs");
    write(&file, "fn g() {}\n");
    assert_eq!(resolve_rust_edition(&file).edition, "2024");
}

#[test]
fn workspace_inherited_edition_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nedition = \"2024\"\n",
    );
    let member = tmp.path().join("crates/m");
    let file = crate_with(
        &member,
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition.workspace = true\n",
    );
    let r = resolve_rust_edition(&file);
    assert_eq!(r.edition, "2024");
    assert_eq!(
        r.source,
        EditionSource::CargoWorkspace(tmp.path().join("Cargo.toml"))
    );
}

#[test]
fn workspace_inheritance_without_workspace_edition_falls_back_with_reason() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"m\"]\n",
    );
    let file = crate_with(
        &tmp.path().join("m"),
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = { workspace = true }\n",
    );
    let r = resolve_rust_edition(&file);
    assert_eq!(r.edition, FALLBACK_EDITION);
    assert!(r.is_fallback());
    assert!(
        r.describe().contains("workspace.package.edition"),
        "{}",
        r.describe()
    );
}

#[test]
fn virtual_manifest_edition_applies_to_loose_files() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n[workspace.package]\nedition = \"2018\"\n",
    );
    let file = tmp.path().join("scripts/x.rs");
    write(&file, "fn main() {}\n");
    assert_eq!(resolve_rust_edition(&file).edition, "2018");
}

#[test]
fn rustfmt_toml_in_crate_wins_over_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let file = crate_with(
        tmp.path(),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(&tmp.path().join(".rustfmt.toml"), "edition = \"2018\"\n");
    let r = resolve_rust_edition(&file);
    assert_eq!(r.edition, "2018");
    assert!(matches!(r.source, EditionSource::RustfmtConfig(_)));
}

#[test]
fn rustfmt_toml_above_the_package_loses_to_manifest() {
    // cargo fmt passes the manifest edition on the CLI, which beats config.
    let tmp = tempfile::tempdir().unwrap();
    write(&tmp.path().join("rustfmt.toml"), "edition = \"2015\"\n");
    let file = crate_with(
        &tmp.path().join("pkg"),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    assert_eq!(resolve_rust_edition(&file).edition, "2021");
}

#[test]
fn rustfmt_toml_without_edition_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let file = crate_with(
        tmp.path(),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    write(&tmp.path().join("rustfmt.toml"), "max_width = 100\n");
    assert_eq!(resolve_rust_edition(&file).edition, "2024");
}

#[test]
fn manifest_without_edition_falls_back_to_2021_with_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let file = crate_with(tmp.path(), "[package]\nname = \"a\"\nversion = \"0.1.0\"\n");
    let r = resolve_rust_edition(&file);
    assert_eq!(r.edition, "2021");
    assert!(r.is_fallback());
    assert!(
        r.describe().contains("no package.edition"),
        "{}",
        r.describe()
    );
}

#[test]
fn no_manifest_falls_back_to_2021() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("loose.rs");
    write(&file, "fn main() {}\n");
    let r = resolve_rust_edition(&file);
    assert_eq!(r.edition, FALLBACK_EDITION);
    assert!(r.is_fallback());
}

#[test]
fn malformed_edition_values_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let file = crate_with(
        tmp.path(),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021 --config x=y\"\n",
    );
    let r = resolve_rust_edition(&file);
    assert_eq!(r.edition, FALLBACK_EDITION);
    assert!(r.is_fallback());
    assert_eq!(valid_edition("2024").as_deref(), Some("2024"));
    assert_eq!(valid_edition("latest"), None);
    assert_eq!(valid_edition("1999"), None);
}

#[test]
fn group_by_edition_splits_mixed_crates() {
    let tmp = tempfile::tempdir().unwrap();
    let a = crate_with(
        &tmp.path().join("a"),
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2018\"\n",
    );
    let b = crate_with(
        &tmp.path().join("b"),
        "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let a2 = tmp.path().join("a/src/other.rs");
    write(&a2, "fn h() {}\n");
    let groups = group_by_edition(&[a.clone(), b.clone(), a2.clone()]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].0.edition, "2018");
    assert_eq!(groups[0].1, vec![a, a2]);
    assert_eq!(groups[1].0.edition, "2021");
    assert_eq!(groups[1].1, vec![b]);
}

#[test]
fn rustfmt_check_args_pass_edition_and_skip_children() {
    let args = rustfmt_check_args("2024");
    assert_eq!(&args[..3], &["--edition", "2024", "--check"]);
    assert!(args.iter().any(|a| a == "skip_children=true"));
}
