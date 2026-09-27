use super::*;

fn shell(stdout: &str, stderr: &str, code: i64) -> String {
    serde_json::json!({"exit_code": code, "stdout": stdout, "stderr": stderr}).to_string()
}

#[test]
fn python_given_a_runner_as_a_script_path_is_detected() {
    let result = shell(
        "",
        "/usr/bin/python3: can't open file '/work/ws/pytest': [Errno 2] No such file or directory\n",
        2,
    );
    let found = detect("python3 pytest tests/test_release.py", &result).unwrap();
    assert_eq!(found.kind, RunnerUnavailableKind::ScriptPath);
    assert_eq!(found.runner, "pytest");
    let hint = found.hint("python3 pytest tests/test_release.py");
    assert!(hint.contains("`python3 -m pytest …`"), "{hint}");
    assert!(hint.contains("not recorded as a failing check"), "{hint}");
    // Through a `cd … &&` prefix and a `2>&1` redirection.
    assert!(detect("cd ws && python3 pytest -q 2>&1", &result).is_some());
}

#[test]
fn interpreter_without_the_runner_module_is_detected() {
    // The live output (CommandLineTools python3 without pytest).
    let result = shell(
        "/Library/Developer/CommandLineTools/usr/bin/python3: No module named pytest\n",
        "",
        1,
    );
    let found = detect(
        "python3 -m pytest tests/test_release.py -x -q 2>&1",
        &result,
    )
    .unwrap();
    assert_eq!(found.kind, RunnerUnavailableKind::ModuleNotInstalled);
    assert_eq!(found.interpreter, "python3");
    assert!(found
        .hint("python3 -m pytest")
        .contains(".venv/bin/python -m pytest"));

    // A `pytest` launcher whose interpreter lost the package.
    let launcher = shell(
        "",
        "Traceback (most recent call last):\n  File \"/usr/local/bin/pytest\", line 5, in <module>\n    \
         from pytest import console_main\nModuleNotFoundError: No module named 'pytest'\n",
        1,
    );
    assert!(detect("pytest tests", &launcher).is_some());
}

#[test]
fn node_given_a_missing_runner_path_is_detected() {
    let result = shell(
        "",
        "node:internal/modules/cjs/loader:1080\n  throw err;\n\nError: Cannot find module '/app/jest'\n    \
         at Module._resolveFilename\n  code: 'MODULE_NOT_FOUND',\n",
        1,
    );
    let found = detect("node jest --ci", &result).unwrap();
    assert_eq!(found.kind, RunnerUnavailableKind::NodeModulePath);
}

#[test]
fn real_test_failures_are_not_runner_failures() {
    // A test that fails on its OWN missing import ran: it is a real failure.
    let ran = shell(
        "============================= test session starts ==============================\n\
         E   ModuleNotFoundError: No module named 'requests'\n1 error\n",
        "",
        2,
    );
    assert!(detect("python3 -m pytest", &ran).is_none());
    // Missing module that is not the runner.
    let other = shell("", "ModuleNotFoundError: No module named 'slugify'\n", 1);
    assert!(detect("python3 -m pytest", &other).is_none());
    assert!(detect("pytest", &other).is_none());
    // A script that exists and fails.
    let failed = shell("FAILED tests/test_x.py::test_a\n", "", 1);
    assert!(detect("python3 run_tests.py", &failed).is_none());
    // `can't open` for a DIFFERENT file than the one invoked.
    let unrelated = shell("", "python3: can't open file '/x/other.py': [Errno 2]\n", 2);
    assert!(detect("python3 run_tests.py", &unrelated).is_none());
    // Not a runner command at all.
    assert!(detect("cargo test", &shell("", "No module named pytest", 1)).is_none());
}

#[test]
fn detect_for_call_reads_shell_arguments_only() {
    let result = shell("", "python3: can't open file '/w/pytest': [Errno 2]\n", 2);
    let args = serde_json::json!({"command": "python3 pytest"}).to_string();
    assert!(detect_for_call("shell_exec", &args, &result).is_some());
    assert!(detect_for_call("pty_shell", &args, &result).is_some());
    assert!(detect_for_call("cargo_test", &args, &result).is_none());
}
