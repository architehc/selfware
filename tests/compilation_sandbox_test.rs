use selfware::cognitive::compilation_manager::CompilationSandbox;
use std::env;

#[test]
fn test_sandbox_creation_and_cleanup() {
    let cwd = env::current_dir().unwrap();

    // We can only run this test if we are inside a git repository
    if !cwd.join(".git").exists() {
        return;
    }

    let sandbox = CompilationSandbox::new(&cwd).unwrap();
    let work_dir = sandbox.work_dir().to_path_buf();

    // The sandbox should exist
    assert!(work_dir.exists());
    assert!(work_dir.join("Cargo.toml").exists());

    // Cleanup the sandbox
    sandbox.cleanup().unwrap();

    // The sandbox should be removed
    assert!(!work_dir.exists());
}
