use std::fs;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// 1. Path Traversal
// ---------------------------------------------------------------------------

/// VULNERABLE: Naively joins base_dir and filename without canonicalization.
/// An attacker can supply "../../../etc/passwd" to escape the base directory.
pub fn read_user_file(base_dir: &str, filename: &str) -> Result<String, String> {
    let path = format!("{}/{}", base_dir, filename);
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.
pub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {
    todo!("Implement secure version with path canonicalization and prefix validation")
}

// ---------------------------------------------------------------------------
// 2. Command Injection
// ---------------------------------------------------------------------------

/// VULNERABLE: Interpolates `query` directly into a shell command string.
/// An attacker can supply `"; echo INJECTED"` to execute arbitrary commands.
pub fn run_search(query: &str) -> Result<String, String> {
    let cmd = format!("grep {} file.txt", query);
    let output = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr))
}

/// SECURE: Must sanitize input so shell metacharacters cannot break out.
pub fn run_search_secure(query: &str) -> Result<String, String> {
    todo!("Implement secure version with input sanitization (no shell metacharacters)")
}

// ---------------------------------------------------------------------------
// 3. Open Redirect
// ---------------------------------------------------------------------------

/// VULNERABLE: Only checks that the URL starts with "/", which is bypassed
/// by protocol-relative URLs like "//evil.com".
pub fn validate_redirect(url: &str) -> bool {
    url.starts_with("/")
}

/// SECURE: Must reject protocol-relative URLs and external hosts.
pub fn validate_redirect_secure(url: &str) -> bool {
    todo!("Implement secure version that rejects protocol-relative URLs and external hosts")
}

// ---------------------------------------------------------------------------
// 4. XSS (Cross-Site Scripting)
// ---------------------------------------------------------------------------

/// VULNERABLE: Interpolates `name` directly into HTML without escaping.
/// An attacker can supply `<script>alert(1)</script>` to inject scripts.
pub fn render_greeting(name: &str) -> String {
    format!("<h1>Hello, {}</h1>", name)
}

/// SECURE: Must escape HTML entities in `name` before embedding in markup.
pub fn render_greeting_secure(name: &str) -> String {
    todo!("Implement secure version with proper HTML entity escaping")
}

// ---------------------------------------------------------------------------
// 5. Information Leak
// ---------------------------------------------------------------------------

/// VULNERABLE: Returns the full internal file path in error messages,
/// exposing server directory structure to the caller.
pub fn handle_error(path: &str) -> String {
    format!("Error: file not found at {}", path)
}

/// SECURE: Must return a generic error message without revealing internal paths.
pub fn handle_error_secure(path: &str) -> String {
    todo!("Implement secure version that returns generic error messages without internal paths")
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Helper: create a temp directory with a known file inside it.
    fn setup_temp_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let file_path = dir.path().join("allowed.txt");
        fs::write(&file_path, "safe content").expect("failed to write test file");
        let canonical = dir.path().canonicalize().unwrap();
        (dir, canonical)
    }

    // -----------------------------------------------------------------------
    // Path Traversal
    // -----------------------------------------------------------------------

    #[test]
    fn test_path_traversal_is_vulnerable() {
        // The insecure version happily accepts traversal sequences.
        // It will try to read the path; we just verify it doesn't reject the input.
        let (_dir, base) = setup_temp_dir();
        let base_str = base.to_str().unwrap();
        // The insecure function builds the path without checking traversal.
        // It may fail with a file-not-found error, but it does NOT reject the
        // traversal attempt — it simply forwards the raw path to the filesystem.
        let result = read_user_file(base_str, "../../../etc/passwd");
        // The key point: the function attempted the traversal rather than blocking it.
        // It either succeeded (on systems with /etc/passwd) or returned a normal
        // IO error — it never returned an "invalid path" / "traversal blocked" error.
        match result {
            Ok(_) => {} // read succeeded — clearly vulnerable
            Err(e) => {
                assert!(
                    !e.to_lowercase().contains("traversal")
                        && !e.to_lowercase().contains("outside"),
                    "Insecure version should not have traversal protection, but got: {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_path_traversal_blocked() {
        let (_dir, base) = setup_temp_dir();
        let base_str = base.to_str().unwrap();
        let result = read_user_file_secure(base_str, "../../../etc/passwd");
        assert!(
            result.is_err(),
            "Secure read_user_file must reject path traversal attempts"
        );

        // A legitimate file inside the base dir should still work.
        let ok = read_user_file_secure(base_str, "allowed.txt");
        assert!(
            ok.is_ok(),
            "Secure read_user_file must allow legitimate files inside base_dir"
        );
        assert_eq!(ok.unwrap(), "safe content");
    }

    // -----------------------------------------------------------------------
    // Command Injection
    // -----------------------------------------------------------------------

    #[test]
    fn test_command_injection_is_vulnerable() {
        // The insecure version passes the query straight to sh -c.
        let result = run_search("; echo INJECTED").unwrap_or_default();
        assert!(
            result.contains("INJECTED"),
            "Insecure run_search should allow injection, got: {}",
            result
        );
    }

    #[test]
    fn test_command_injection_blocked() {
        let result = run_search_secure("; echo INJECTED");
        match result {
            Ok(output) => {
                assert!(
                    !output.contains("INJECTED"),
                    "Secure run_search must not execute injected commands, got: {}",
                    output
                );
            }
            Err(_) => {
                // Returning an error for dangerous input is also acceptable.
            }
        }
    }

    // -----------------------------------------------------------------------
    // Open Redirect
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_redirect_is_vulnerable() {
        // Protocol-relative URL bypasses the naive starts_with("/") check.
        assert!(
            validate_redirect("//evil.com"),
            "Insecure validate_redirect should accept //evil.com"
        );
    }

    #[test]
    fn test_open_redirect_blocked() {
        assert!(
            !validate_redirect_secure("//evil.com"),
            "Secure validate_redirect must reject protocol-relative URLs"
        );
        assert!(
            !validate_redirect_secure("https://evil.com"),
            "Secure validate_redirect must reject absolute external URLs"
        );
        assert!(
            validate_redirect_secure("/dashboard"),
            "Secure validate_redirect must accept safe relative paths"
        );
        assert!(
            validate_redirect_secure("/settings/profile"),
            "Secure validate_redirect must accept safe relative paths with segments"
        );
    }

    // -----------------------------------------------------------------------
    // XSS
    // -----------------------------------------------------------------------

    #[test]
    fn test_xss_is_vulnerable() {
        let output = render_greeting("<script>alert(1)</script>");
        assert!(
            output.contains("<script>"),
            "Insecure render_greeting should contain raw script tags, got: {}",
            output
        );
    }

    #[test]
    fn test_xss_blocked() {
        let output = render_greeting_secure("<script>alert(1)</script>");
        assert!(
            !output.contains("<script>"),
            "Secure render_greeting must escape HTML, got: {}",
            output
        );
        assert!(
            output.contains("&lt;") || output.contains("&#"),
            "Secure render_greeting must use HTML entities for angle brackets, got: {}",
            output
        );
    }

    // -----------------------------------------------------------------------
    // Information Leak
    // -----------------------------------------------------------------------

    #[test]
    fn test_info_leak_is_vulnerable() {
        let msg = handle_error("/var/app/secret/data.db");
        assert!(
            msg.contains("/var/app/secret/data.db"),
            "Insecure handle_error should leak the full path, got: {}",
            msg
        );
    }

    #[test]
    fn test_info_leak_blocked() {
        let msg = handle_error_secure("/var/app/secret/data.db");
        assert!(
            !msg.contains("/var/app"),
            "Secure handle_error must not reveal internal paths, got: {}",
            msg
        );
        assert!(
            !msg.contains("secret"),
            "Secure handle_error must not reveal directory names, got: {}",
            msg
        );
        // It should still indicate an error occurred.
        let lower = msg.to_lowercase();
        assert!(
            lower.contains("error") || lower.contains("not found"),
            "Secure handle_error should still indicate an error, got: {}",
            msg
        );
    }
}
