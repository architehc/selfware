//! Environment sanitization for spawned child processes.
//!
//! The agent process holds secrets in its environment — `SELFWARE_API_KEY`,
//! and whatever the operator exported (`AWS_*`, `GITHUB_TOKEN`, …). Any child
//! process spawned with the inherited environment can read all of it, so a
//! single compromised subprocess (a hostile MCP server, an injected shell
//! command, the Playwright bridge loading attacker-controlled content) can
//! exfiltrate every credential on the box.
//!
//! `shell_exec` already clears the environment and re-adds a minimal base.
//! This helper centralizes that policy so every spawn site applies the same
//! allowlist and none silently drifts back to inheriting the full env.

/// Clear a command's inherited environment and re-populate the shared
/// non-sensitive allowlist (`DEFAULT_KEEP`: toolchain basics, git
/// identity, proxy/TLS trust, temp locations, terminal type, Rust
/// toolchain homes). Credential-bearing variables (`SELFWARE_API_KEY`,
/// `AWS_*`, tokens) are deliberately absent and never forwarded.
///
/// Call this immediately after constructing the `Command` and BEFORE adding
/// any task-specific variables, so those additions survive the clear.
pub fn sanitize_command_env(cmd: &mut tokio::process::Command) {
    sanitize_command_env_preserve(cmd, &[]);
}

/// Like [`sanitize_command_env`], but additionally re-adds the named
/// variables from the parent environment when present.
///
/// Use this for spawn sites whose child tools legitimately need session
/// state to function — e.g. computer-control tools (xdotool, wmctrl, …)
/// need `DISPLAY`/`WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR`/`XAUTHORITY`/
/// `SSH_AUTH_SOCK` to reach the user's display server. Never pass
/// credential-bearing names (`SELFWARE_API_KEY`, `AWS_*`, tokens) here.
pub fn sanitize_command_env_preserve(cmd: &mut tokio::process::Command, preserve: &[&str]) {
    cmd.env_clear();
    for (key, value) in kept_env(preserve) {
        cmd.env(key, value);
    }
}

/// [`sanitize_command_env_preserve`] for synchronous spawn sites built on
/// `std::process::Command` (e.g. backend probes that cannot `.await`).
pub fn sanitize_std_command_env_preserve(cmd: &mut std::process::Command, preserve: &[&str]) {
    cmd.env_clear();
    for (key, value) in kept_env(preserve) {
        cmd.env(key, value);
    }
}

/// The baseline kept after `env_clear`: the shared non-sensitive allowlist
/// that every sanitized spawn site forwards. Tools legitimately need these
/// to function (git authorship in clean containers, corporate proxies,
/// custom CA bundles, temp/scratch locations, terminal identity, Rust
/// toolchains in non-default homes) without exposing credentials. Keys not
/// present in the parent environment are simply skipped.
///
/// Proxy URLs may embed `user:password@` userinfo; that userinfo is stripped
/// before forwarding (host:port only) unless the operator opts in with
/// `SELFWARE_FORWARD_PROXY_CREDENTIALS=1` for deployments whose proxy
/// requires authentication.
const DEFAULT_KEEP: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "TMPDIR",
    "TEMP",
    "TMP",
    "TERM",
    "CARGO_HOME",
    "RUSTUP_HOME",
];

/// The keep-list applied after `env_clear`: [`DEFAULT_KEEP`] plus any
/// caller-preserved names, resolved from the parent environment.
fn kept_env<'a>(preserve: &'a [&'a str]) -> Vec<(&'a str, std::ffi::OsString)> {
    let proxy_credentials_opt_in = proxy_credentials_opt_in();
    DEFAULT_KEEP
        .iter()
        .copied()
        .chain(preserve.iter().copied())
        .filter_map(|key| {
            std::env::var_os(key).map(|value| {
                let value = sanitize_kept_value(key, &value, proxy_credentials_opt_in);
                (key, value)
            })
        })
        .collect()
}

/// Apply per-key sanitization to a kept value before it reaches a child.
///
/// Proxy URLs may embed `user:password@` credentials that would leak to
/// every sanitized child (build scripts included); strip that userinfo
/// unless the operator opted into forwarding credentials verbatim.
fn sanitize_kept_value(
    key: &str,
    value: &std::ffi::OsString,
    forward_proxy_credentials: bool,
) -> std::ffi::OsString {
    if !forward_proxy_credentials && matches!(key, "HTTP_PROXY" | "HTTPS_PROXY") {
        if let Some(s) = value.to_str() {
            if let Some(stripped) = strip_proxy_userinfo(s) {
                return std::ffi::OsString::from(stripped);
            }
        }
    }
    value.clone()
}

/// Strip `user:password@` userinfo from a proxy URL, keeping host:port.
///
/// Only values that carry an explicit `scheme://` marker are treated as
/// URLs; anything else (bare `host:port`, host lists) passes through
/// unchanged, as do URLs without userinfo.
fn strip_proxy_userinfo(value: &str) -> Option<String> {
    let (prefix, rest) = value.split_once("://")?;
    let host_start = rest.find('@')?;
    Some(format!("{prefix}://{}", &rest[host_start + 1..]))
}

/// Whether the operator opted into forwarding proxy URL userinfo verbatim
/// (`SELFWARE_FORWARD_PROXY_CREDENTIALS=1` / `=true`). Deployments whose
/// proxy REQUIRES authentication set this; everyone else keeps the default
/// userinfo-stripping behavior.
fn proxy_credentials_opt_in() -> bool {
    std::env::var("SELFWARE_FORWARD_PROXY_CREDENTIALS")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

#[cfg(test)]
#[path = "../../tests/unit/safety/process_env/process_env_test.rs"]
mod tests;
