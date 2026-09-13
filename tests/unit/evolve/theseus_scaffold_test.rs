use super::*;
use tempfile::tempdir;

#[test]
fn test_generate_collection_map_extracts_symbols_and_modules() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();

    let lib_rs = r#"
pub mod math;
pub use math::add;
"#;
    fs::write(src.join("lib.rs"), lib_rs).unwrap();

    let math_rs = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub struct Calculator {
    pub precision: usize,
}

pub enum Operation {
    Add,
    Subtract,
}
"#;
    fs::write(src.join("math.rs"), math_rs).unwrap();

    let map = TheseusScaffold::generate_collection_map(tmp.path())
        .expect("should generate collection map");

    assert!(map.modules.contains(&"math".to_string()));
    assert!(map.reexports.contains(&"math::add".to_string()));

    let sym_names: Vec<_> = map.symbols.iter().map(|s| s.name.clone()).collect();
    assert!(sym_names.contains(&"add".to_string()));
    assert!(sym_names.contains(&"Calculator".to_string()));
    assert!(sym_names.contains(&"Operation".to_string()));

    let md = map.to_markdown();
    assert!(md.contains("# Theseus Collection Map"));
    assert!(md.contains("`math`"));
    assert!(md.contains("`Calculator`"));
}

#[test]
fn test_generate_event_log_fallback_on_non_git() {
    let tmp = tempdir().unwrap();
    let log = TheseusScaffold::generate_event_log(tmp.path(), 5)
        .expect("event log should produce fallback");

    assert!(!log.events.is_empty());
    assert_eq!(log.events[0].kind, "workspace_init");
    let md = log.to_markdown();
    assert!(md.contains("# Theseus Event Log"));
}

#[test]
fn test_scaffold_shadow_worktree_injects_artifacts() {
    let tmp = tempdir().unwrap();
    let base = tmp.path().join("base");
    let shadow = tmp.path().join("shadow");

    let src = base.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&shadow).unwrap();

    fs::write(src.join("lib.rs"), "pub mod worker;\n").unwrap();
    fs::write(src.join("worker.rs"), "pub fn do_work() -> bool { true }\n").unwrap();

    let report = TheseusScaffold::scaffold_shadow_worktree(&shadow, &base)
        .expect("scaffolding shadow worktree should succeed");

    assert!(report.modules_mapped >= 1);
    assert!(report.symbols_extracted >= 1);
    assert!(shadow.join(".selfware/theseus/collection_map.md").exists());
    assert!(shadow.join(".selfware/theseus/event_log.md").exists());
    assert!(shadow.join(".theseus.md").exists());

    let root_guide = fs::read_to_string(shadow.join(".theseus.md")).unwrap();
    assert!(root_guide.contains("# Theseus Environment Guide"));
    assert!(root_guide.contains("Architecture Summary"));
}

#[test]
fn test_generate_event_log_with_real_git_commits() {
    let tmp = tempdir().unwrap();
    let repo_path = tmp.path();

    let run_git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .status()
            .unwrap();
        assert!(status.success());
    };

    run_git(&["init"]);
    run_git(&["config", "user.email", "theseus@selfware.ai"]);
    run_git(&["config", "user.name", "Theseus Agent"]);
    fs::write(repo_path.join("README.md"), "# Project").unwrap();
    run_git(&["add", "."]);
    run_git(&["commit", "-m", "Initial commit from theseus"]);

    let log = TheseusScaffold::generate_event_log(repo_path, 5).expect("should read git log");
    assert!(!log.events.is_empty());
    assert!(log
        .events
        .iter()
        .any(|e| e.summary.contains("Initial commit from theseus")));
}

#[test]
fn test_empty_event_log_markdown() {
    let empty_log = EventLog::default();
    let md = empty_log.to_markdown();
    assert!(md.contains("No recent historical events recorded."));
}
