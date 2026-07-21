use super::*;

#[test]
fn test_project_type_detect_rust() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("Cargo.toml"), "").unwrap();
    assert_eq!(ProjectType::detect(tmp.path()), Some(ProjectType::Rust));
}

#[test]
fn test_project_type_detect_go() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("go.mod"), "").unwrap();
    assert_eq!(ProjectType::detect(tmp.path()), Some(ProjectType::Go));
}

#[test]
fn test_project_status_from_results() {
    assert_eq!(
        ProjectStatus::from_results(true, 10, 0, 100),
        ProjectStatus::Green
    );
    assert_eq!(
        ProjectStatus::from_results(true, 5, 2, 100),
        ProjectStatus::Partial
    );
    assert_eq!(
        ProjectStatus::from_results(true, 0, 0, 100),
        ProjectStatus::Compiles
    );
    assert_eq!(
        ProjectStatus::from_results(false, 0, 0, 50),
        ProjectStatus::Wrote
    );
    assert_eq!(
        ProjectStatus::from_results(false, 0, 0, 0),
        ProjectStatus::Fail
    );
}
