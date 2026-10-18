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

#[test]
fn test_scaffold_template_rejects_traversal_and_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    let templates_dir = tmp.path().join("templates");
    let dst = tmp.path().join("dst");
    std::fs::create_dir_all(&templates_dir).unwrap();

    for bad in ["", "../escape", "nested/tpl", "nested\\tpl"] {
        let err = scaffold_template(bad, &templates_dir, &dst);
        assert!(err.is_err(), "template name '{bad}' must be rejected");
    }

    #[cfg(unix)]
    {
        let tpl = templates_dir.join("with_link");
        std::fs::create_dir_all(&tpl).unwrap();
        let outside = tmp.path().join("outside.txt");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, tpl.join("leak.txt")).unwrap();

        let err = scaffold_template("with_link", &templates_dir, &dst);
        assert!(
            err.is_err(),
            "scaffold_template must refuse symlink entries inside template"
        );
    }
}
