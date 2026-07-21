use super::*;

#[test]
fn test_shell_exec_name() {
    let tool = ShellExec;
    assert_eq!(tool.name(), "shell_exec");
}

#[test]
fn test_shell_exec_description() {
    let tool = ShellExec;
    assert!(tool.description().contains("Execute"));
    assert!(tool.description().contains("command"));
}

#[test]
fn test_shell_exec_schema() {
    let tool = ShellExec;
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["command"].is_object());
    assert!(schema["properties"]["timeout_secs"].is_object());
}

#[tokio::test]
async fn test_shell_exec_echo() {
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "echo 'hello world'",
        "timeout_secs": 5
    });

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["exit_code"], 0);
    assert!(result["stdout"].as_str().unwrap().contains("hello world"));
    assert_eq!(result["timed_out"], false);
}

#[tokio::test]
#[cfg(unix)]
async fn registered_shell_exec_timeout_reaps_process_group() {
    // The REGISTERED shell tool must reap the whole tree on timeout — a
    // backgrounded grandchild (`sleep 30 &`) must be killed, not orphaned.
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("gc.pid");
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": format!("sleep 30 & echo $! > {}; wait", pidfile.display()),
        "timeout_secs": 1
    });
    let start = std::time::Instant::now();
    let result = tool.execute(args).await.unwrap();
    assert!(start.elapsed().as_secs() < 10, "should return at timeout");
    assert_eq!(result["timed_out"], true);

    let gc_pid: i32 = std::fs::read_to_string(&pidfile)
        .expect("grandchild wrote its pid")
        .trim()
        .parse()
        .expect("valid pid");
    tokio::time::sleep(Duration::from_millis(500)).await;
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        let alive = kill(Pid::from_raw(gc_pid), None).is_ok();
        assert!(!alive, "grandchild pid {gc_pid} should have been reaped");
    }
}

#[tokio::test]
async fn registered_shell_exec_large_output_completes() {
    // ~40 MiB of output must complete without hang/OOM (bounded drain).
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "head -c 41943040 /dev/zero | tr '\\0' 'a'",
        "timeout_secs": 30
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["timed_out"], false);
    assert_eq!(result["exit_code"], 0);
}

#[tokio::test]
async fn test_shell_exec_exit_code() {
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "exit 42",
        "timeout_secs": 5
    });

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["exit_code"], 42);
}

#[tokio::test]
async fn test_shell_exec_stderr() {
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "echo 'error' >&2",
        "timeout_secs": 5
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result["stderr"].as_str().unwrap().contains("error"));
}

#[tokio::test]
async fn test_shell_exec_with_env() {
    let tool = ShellExec;
    let command = if cfg!(target_os = "windows") {
        "echo %MY_VAR%"
    } else {
        "echo $MY_VAR"
    };
    let args = serde_json::json!({
        "command": command,
        "timeout_secs": 5,
        "env": {
            "MY_VAR": "custom_value"
        }
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result["stdout"].as_str().unwrap().contains("custom_value"));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_shell_exec_with_cwd() {
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "pwd",
        "cwd": "/tmp",
        "timeout_secs": 5
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result["stdout"].as_str().unwrap().contains("/tmp"));
}

#[tokio::test]
#[cfg(unix)]
async fn test_shell_exec_timeout() {
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "sleep 10",
        "timeout_secs": 1
    });

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["timed_out"], true);
    let stderr = result["stderr"].as_str().unwrap();
    assert!(stderr.contains("timed out"));
    // Regression: the timeout must be actionable so the model retries with a
    // larger timeout instead of looping (e.g. `cargo check` on big crates).
    assert!(stderr.contains("timeout_secs"), "stderr was: {stderr}");
}

#[test]
fn test_command_is_long_running_build() {
    // Build/test commands get a generous timeout floor so they aren't killed
    // at 60s with empty output (which made the model loop on `cargo check`).
    assert!(command_is_long_running_build("cargo check --all-targets"));
    assert!(command_is_long_running_build(
        "cd foo && cargo build --release"
    ));
    assert!(command_is_long_running_build("npm install"));
    assert!(!command_is_long_running_build("ls -la"));
    assert!(!command_is_long_running_build("grep -r foo src/"));
}

#[tokio::test]
async fn test_dangerous_pattern_dev_tcp() {
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "cat < /dev/tcp/127.0.0.1/8080",
        "timeout_secs": 5
    });
    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Blocked potentially dangerous shell pattern"));
}

#[tokio::test]
async fn test_cwd_relative_path_rejected() {
    let tool = ShellExec;
    let args = serde_json::json!({
        "command": "echo test",
        "cwd": "relative/path",
        "timeout_secs": 5
    });
    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cwd must be an absolute path"));
}

#[tokio::test]
async fn test_command_exceeds_max_length_rejected() {
    let tool = ShellExec;
    let long_cmd = "a".repeat(10_001);
    let args = serde_json::json!({
        "command": long_cmd,
        "timeout_secs": 5
    });
    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("exceeds maximum length"));
}

// --- sed -i interception tests ---

#[test]
fn test_parse_sed_substitution_simple() {
    let result = try_parse_sed_substitution("sed -i 's/old/new/g' file.txt");
    assert!(result.is_some());
    let sub = result.unwrap();
    assert_eq!(sub.file, "file.txt");
    assert_eq!(sub.old_str, "old");
    assert_eq!(sub.new_str, "new");
    assert!(sub.global);
    assert_eq!(sub.backup_suffix, None);
}

#[test]
fn test_parse_sed_substitution_no_global() {
    let result = try_parse_sed_substitution("sed -i 's/old/new/' file.txt");
    assert!(result.is_some());
    assert!(!result.unwrap().global);
}

#[test]
fn test_parse_sed_substitution_backup_suffix() {
    let sub = try_parse_sed_substitution("sed -i.bak 's/a/b/' f.txt").unwrap();
    assert_eq!(sub.backup_suffix.as_deref(), Some(".bak"));
    let sub = try_parse_sed_substitution("sed --in-place=.orig 's/a/b/' f.txt").unwrap();
    assert_eq!(sub.backup_suffix.as_deref(), Some(".orig"));
}

#[test]
fn test_parse_sed_substitution_falls_back_for_complex() {
    // Regex metacharacters in pattern
    assert!(try_parse_sed_substitution("sed -i 's/foo.bar/baz/g' file.txt").is_none());
    // Empty pattern (sed's "repeat last regex" semantics — real sed errors here)
    assert!(try_parse_sed_substitution("sed -i 's//x/g' file.txt").is_none());
    // Replacement escapes (& expands to the match, \1 is a backreference)
    assert!(try_parse_sed_substitution("sed -i 's/foo/[&]/' file.txt").is_none());
    assert!(try_parse_sed_substitution("sed -i 's/foo/\\\\1/' file.txt").is_none());
    // Unterminated s command — real sed must report the error
    assert!(try_parse_sed_substitution("sed -i 's/foo/bar' file.txt").is_none());
    // Non-g flags have their own semantics (numeric, p, w, i)
    assert!(try_parse_sed_substitution("sed -i 's/a/b/2' file.txt").is_none());
    assert!(try_parse_sed_substitution("sed -i 's/a/b/gp' file.txt").is_none());
    // Extra delimiters — real sed errors ("unknown option to `s'")
    assert!(try_parse_sed_substitution("sed -i 's/a/b/c/d' file.txt").is_none());
    // Extra sed flags beyond -i
    assert!(try_parse_sed_substitution("sed -n -i 's/a/b/' file.txt").is_none());
    assert!(try_parse_sed_substitution("sed -i -e 's/a/b/' file.txt").is_none());
    // No -i at all
    assert!(try_parse_sed_substitution("sed 's/a/b/' file.txt").is_none());
    // Multiple files
    assert!(try_parse_sed_substitution("sed -i 's/a/b/g' f1 f2").is_none());
    // Not sed
    assert!(try_parse_sed_substitution("echo hello").is_none());
    assert!(try_parse_sed_substitution("sediment -i 's/a/b/' f").is_none());
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn test_shell_exec_intercepts_sed() {
    let tool = ShellExec;
    // Keep the temp directory under the current working dir so the default
    // safety config (`./**`) allows the intercepted path validation.
    let temp_dir = std::env::current_dir()
        .unwrap()
        .join(format!("selfware-sed-test-{}", std::process::id()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let file_path = temp_dir.join("test.txt");
    tokio::fs::write(&file_path, "hello world\nfoo bar\n")
        .await
        .unwrap();

    let args = serde_json::json!({
        "command": format!("sed -i 's/foo/FOO/g' {}", file_path.display()),
        "timeout_secs": 5
    });

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["intercepted"], true);
    assert_eq!(result["replacements"], 1);

    let content = tokio::fs::read_to_string(&file_path).await.unwrap();
    assert!(content.contains("FOO bar"));
    assert!(!content.contains("foo bar"));

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn sed_interception_no_g_replaces_first_match_per_line() {
    // Real sed without `g` replaces the first match ON EACH LINE, not the
    // first match in the whole file — the old interception got this wrong.
    let tool = ShellExec;
    let temp_dir = std::env::current_dir()
        .unwrap()
        .join(format!("selfware-sed-test-lines-{}", std::process::id()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let file_path = temp_dir.join("lines.txt");
    tokio::fs::write(&file_path, "foo one\nnope\nfoo two foo two\n")
        .await
        .unwrap();

    let args = serde_json::json!({
        "command": format!("sed -i 's/foo/bar/' {}", file_path.display()),
        "timeout_secs": 5
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["intercepted"], true);
    assert_eq!(result["replacements"], 2);

    let content = tokio::fs::read_to_string(&file_path).await.unwrap();
    assert_eq!(content, "bar one\nnope\nbar two foo two\n");

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn sed_interception_honors_backup_suffix() {
    // `sed -i.bak` must leave the original at <file>.bak like real sed.
    let tool = ShellExec;
    let temp_dir = std::env::current_dir()
        .unwrap()
        .join(format!("selfware-sed-test-bak-{}", std::process::id()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let file_path = temp_dir.join("bak.txt");
    tokio::fs::write(&file_path, "alpha\n").await.unwrap();

    let args = serde_json::json!({
        "command": format!("sed -i.bak 's/alpha/BETA/' {}", file_path.display()),
        "timeout_secs": 5
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["intercepted"], true);

    let backup = temp_dir.join("bak.txt.bak");
    assert!(backup.exists(), "-i.bak must create a backup file");
    assert_eq!(tokio::fs::read_to_string(&backup).await.unwrap(), "alpha\n");
    assert_eq!(
        tokio::fs::read_to_string(&file_path).await.unwrap(),
        "BETA\n"
    );

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn sed_interception_resolves_relative_path_against_cwd() {
    // The interception must resolve a relative target against the call's
    // cwd, exactly like the real sed subprocess would.
    let tool = ShellExec;
    let temp_dir = std::env::current_dir()
        .unwrap()
        .join(format!("selfware-sed-test-cwd-{}", std::process::id()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    tokio::fs::write(temp_dir.join("rel.txt"), "foo\n")
        .await
        .unwrap();

    let args = serde_json::json!({
        "command": "sed -i 's/foo/bar/' rel.txt",
        "cwd": temp_dir.to_str().unwrap(),
        "timeout_secs": 5
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["intercepted"], true);
    assert_eq!(
        tokio::fs::read_to_string(temp_dir.join("rel.txt"))
            .await
            .unwrap(),
        "bar\n"
    );

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn sed_no_match_exits_zero_and_leaves_file_untouched() {
    // Real sed with no match exits 0 and does not modify the file; the
    // interception must not invent a failure.
    let tool = ShellExec;
    let temp_dir = std::env::current_dir()
        .unwrap()
        .join(format!("selfware-sed-test-nomatch-{}", std::process::id()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let file_path = temp_dir.join("nomatch.txt");
    tokio::fs::write(&file_path, "untouched\n").await.unwrap();

    let args = serde_json::json!({
        "command": format!("sed -i 's/zzz/yyy/' {}", file_path.display()),
        "timeout_secs": 5
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["intercepted"], true);
    assert_eq!(result["exit_code"], 0);
    assert_eq!(result["replacements"], 0);
    assert_eq!(
        tokio::fs::read_to_string(&file_path).await.unwrap(),
        "untouched\n"
    );

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn sed_empty_pattern_passes_through_to_real_sed_error() {
    let _g = crate::test_support::CwdGuard::hold();
    // Empty pattern is outside the safe subset: the real sed binary runs
    // and reports its own error instead of the interception corrupting the
    // file by inserting the replacement at every position.
    let tool = ShellExec;
    let temp_dir = std::env::current_dir()
        .unwrap()
        .join(format!("selfware-sed-test-empty-{}", std::process::id()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let file_path = temp_dir.join("empty.txt");
    tokio::fs::write(&file_path, "abc\n").await.unwrap();

    let args = serde_json::json!({
        "command": format!("sed -i 's//x/g' {}", file_path.display()),
        "timeout_secs": 5
    });
    let result = tool.execute(args).await.unwrap();
    assert!(
        result.get("intercepted").is_none(),
        "empty pattern must fall through to the real sed: {result}"
    );
    assert_ne!(result["exit_code"], 0, "real sed errors on empty pattern");
    // File content is unchanged.
    assert_eq!(
        tokio::fs::read_to_string(&file_path).await.unwrap(),
        "abc\n"
    );

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn sed_replacement_metachars_pass_through_to_real_sed() {
    let _g = crate::test_support::CwdGuard::hold();
    // `&` in the replacement expands to the matched text in real sed; the
    // interception must not treat it literally.
    let tool = ShellExec;
    let temp_dir = std::env::current_dir()
        .unwrap()
        .join(format!("selfware-sed-test-amp-{}", std::process::id()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let file_path = temp_dir.join("amp.txt");
    tokio::fs::write(&file_path, "foo\n").await.unwrap();

    let args = serde_json::json!({
        "command": format!("sed -i 's/foo/[&]/' {}", file_path.display()),
        "timeout_secs": 5
    });
    let result = tool.execute(args).await.unwrap();
    assert!(
        result.get("intercepted").is_none(),
        "& replacement must fall through to real sed: {result}"
    );
    // The remaining assertions exercise the *real* sed binary, whose `-i`
    // semantics are GNU-specific: BSD sed (macOS) requires an explicit
    // backup suffix, so `sed -i 's/…' file` errors there. The portable
    // subject of this test is the interception decision asserted above.
    #[cfg(not(target_os = "macos"))]
    {
        assert_eq!(result["exit_code"], 0);
        assert_eq!(
            tokio::fs::read_to_string(&file_path).await.unwrap(),
            "[foo]\n"
        );
    }

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}
