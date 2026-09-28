//! A test runner that was never started: the interpreter could not find it.
//!
//! `python3 pytest tests/x.py` asks python3 to execute a FILE named `pytest`
//! in the working directory ("python3: can't open file '…/pytest': [Errno 2]");
//! `python3 -m pytest` on an interpreter without pytest prints
//! "…/python3: No module named pytest"; a `pytest` launcher whose interpreter
//! lost the package dies with `ModuleNotFoundError: No module named 'pytest'`.
//! None of them ran a single test, yet each exits non-zero and was recorded as
//! a failing check. The failure could never be attributed (no error lines to
//! compare) and never cleared by the passing run under a different identity,
//! so completion was refused until UNATTRIBUTED_FAILURE_LOOP ended the run
//! (0.9.4 live finding on python-slugify).
//!
//! Detection keys on the RUNNER NAMED IN THE COMMAND being the thing that is
//! missing — a test that fails on its own `ModuleNotFoundError` is a real
//! failure and is not matched.

use regex::Regex;
use std::sync::OnceLock;

/// How the runner could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunnerUnavailableKind {
    /// `python3 pytest …`: the runner was passed as a script path that does
    /// not exist.
    ScriptPath,
    /// `python3 -m pytest …` / a `pytest` launcher: the interpreter has no
    /// such module installed.
    ModuleNotInstalled,
    /// `node jest …`: node was given a module path that does not exist.
    NodeModulePath,
}

/// A runner the command named but the interpreter could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunnerUnavailable {
    pub(crate) kind: RunnerUnavailableKind,
    /// The interpreter token as written (`python3`), or the launcher.
    pub(crate) interpreter: String,
    /// The runner the command named (`pytest`, `unittest`, `jest`).
    pub(crate) runner: String,
}

impl RunnerUnavailable {
    /// Identity for "tell the model once".
    pub(crate) fn key(&self) -> String {
        format!("{:?}:{}:{}", self.kind, self.interpreter, self.runner)
    }

    /// The one-time correction for the model.
    pub(crate) fn hint(&self, command: &str) -> String {
        let (interp, runner) = (&self.interpreter, &self.runner);
        let body = match self.kind {
            RunnerUnavailableKind::ScriptPath => format!(
                "`{command}` asked `{interp}` to execute a FILE named `{runner}` in the working \
                 directory, and there is none — no tests ran. Run the runner as a module: \
                 `{interp} -m {runner} …` (or `{runner} …` if it is on PATH)."
            ),
            RunnerUnavailableKind::ModuleNotInstalled => format!(
                "`{interp}` here has no `{runner}` module installed, so `{command}` ran no \
                 tests. Use an interpreter that has it: the project's virtualenv (e.g. \
                 `.venv/bin/python -m {runner} …`), another python3.X on PATH that has \
                 {runner}, or `{runner} …` if it is on PATH."
            ),
            RunnerUnavailableKind::NodeModulePath => format!(
                "`{command}` asked node to load `{runner}` as a file path and it does not \
                 exist — no tests ran. Use the package's runner: `npx {runner} …` or \
                 `npm test`."
            ),
        };
        format!("RUNNER NOT STARTED: {body} This call is not recorded as a failing check.")
    }

    /// The repeat note once the hint was given.
    pub(crate) fn repeat_note(&self, command: &str) -> String {
        format!(
            "RUNNER NOT STARTED again: `{command}` ran no tests (see the earlier note on \
             `{}`/`{}`); not recorded as a failing check.",
            self.interpreter, self.runner
        )
    }
}

fn is_python(word: &str) -> bool {
    let base = word.rsplit('/').next().unwrap_or(word);
    let rest = base.strip_prefix("python").unwrap_or("-");
    rest.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn basename(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

fn cant_open_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"can't open file '([^']+)': \[Errno 2\]").expect("static runner regex")
    })
}

/// Whether the missing module `missing` is `runner` or one of the packages
/// it lives in (`a` or `a.b` for `a.b.c`).
fn module_on_runner_path(missing: &str, runner: &str) -> bool {
    runner == missing
        || runner
            .strip_prefix(missing)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn no_module_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"No module named '?([A-Za-z_][\w.]*)'?"#).expect("static runner regex")
    })
}

fn node_missing_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Cannot find module '([^']+)'").expect("static runner regex"))
}

/// The runner's own output: stdout + stderr of a shell result, else the text.
fn output_text(result: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(result.trim()) {
        Ok(value) => {
            let field = |k: &str| value.get(k).and_then(|v| v.as_str()).unwrap_or("");
            let joined = format!("{}\n{}", field("stdout"), field("stderr"));
            if joined.trim().is_empty() {
                result.to_string()
            } else {
                joined
            }
        }
        Err(_) => result.to_string(),
    }
}

fn is_runner_word(word: &str) -> bool {
    is_python(word) || matches!(basename(word), "node" | "pytest" | "py.test")
}

/// The last `&&`/`||`/`;`/`|`-separated segment whose program is a runner,
/// as words (leading `VAR=value` assignments skipped; `2>&1`-style
/// redirections are not separators).
fn runner_words(command: &str) -> Vec<&str> {
    static SEP: OnceLock<Regex> = OnceLock::new();
    let sep = SEP.get_or_init(|| Regex::new(r"&&|\|\||;|\||\n").expect("static runner regex"));
    static REDIRECT: OnceLock<Regex> = OnceLock::new();
    let redirect = REDIRECT.get_or_init(|| Regex::new(r"\d*>&\d+").expect("static runner regex"));
    let mut found: Vec<&str> = Vec::new();
    let mut last_end = 0;
    let mut segments: Vec<&str> = Vec::new();
    // Split on separators that are not part of a `N>&M` redirection.
    let masked = redirect.replace_all(command, |c: &regex::Captures<'_>| " ".repeat(c[0].len()));
    for m in sep.find_iter(&masked) {
        segments.push(&command[last_end..m.start()]);
        last_end = m.end();
    }
    segments.push(&command[last_end..]);
    for seg in segments {
        let words: Vec<&str> = seg
            .split_whitespace()
            .skip_while(|w| w.contains('=') && !w.starts_with('-'))
            .filter(|w| !redirect.is_match(w))
            .collect();
        if words.first().is_some_and(|w| is_runner_word(w)) {
            found = words;
        }
    }
    found
}

/// Did `command` fail because the runner it names was never started?
pub(crate) fn detect(command: &str, result: &str) -> Option<RunnerUnavailable> {
    let words = runner_words(command);
    let first = *words.first()?;
    let output = output_text(result);
    if output.contains("test session starts") {
        return None;
    }
    if is_python(first) {
        let (module_form, runner) = match words.get(1) {
            Some(&"-m") => (true, *words.get(2)?),
            Some(w) if !w.starts_with('-') => (false, *w),
            _ => return None,
        };
        if !module_form {
            let missing = cant_open_re()
                .captures_iter(&output)
                .any(|c| basename(&c[1]) == basename(runner));
            return missing.then(|| RunnerUnavailable {
                kind: RunnerUnavailableKind::ScriptPath,
                interpreter: first.to_string(),
                runner: basename(runner).to_string(),
            });
        }
        // The runner module itself (or a package on its path) is missing —
        // not a module the runner imported once it had started: `python3 -m
        // tests` failing on `No module named 'tests.helpers'` ran the tests
        // and is a real failure (review 2026-09-27: only the top-level
        // package was compared).
        let missing = no_module_re()
            .captures_iter(&output)
            .any(|c| module_on_runner_path(&c[1], runner));
        return missing.then(|| RunnerUnavailable {
            kind: RunnerUnavailableKind::ModuleNotInstalled,
            interpreter: first.to_string(),
            runner: runner.to_string(),
        });
    }
    if basename(first) == "node" {
        let runner = *words.get(1)?;
        if runner.starts_with('-') {
            return None;
        }
        let missing = output.contains("MODULE_NOT_FOUND")
            && node_missing_re()
                .captures_iter(&output)
                .any(|c| basename(&c[1]) == basename(runner));
        return missing.then(|| RunnerUnavailable {
            kind: RunnerUnavailableKind::NodeModulePath,
            interpreter: "node".to_string(),
            runner: basename(runner).to_string(),
        });
    }
    // A launcher script (`pytest`, `py.test`) whose interpreter lacks its own
    // package: the traceback ends in `No module named '<launcher>'`.
    let launcher = basename(first);
    let package = match launcher {
        "pytest" | "py.test" => "pytest",
        _ => return None,
    };
    let missing = output.contains("ModuleNotFoundError")
        && no_module_re()
            .captures_iter(&output)
            .any(|c| &c[1] == package);
    missing.then(|| RunnerUnavailable {
        kind: RunnerUnavailableKind::ModuleNotInstalled,
        interpreter: launcher.to_string(),
        runner: package.to_string(),
    })
}

/// [`detect`] from a shell tool's JSON arguments.
pub(crate) fn detect_for_call(
    tool_name: &str,
    args_str: &str,
    result: &str,
) -> Option<RunnerUnavailable> {
    if !matches!(tool_name, "shell_exec" | "pty_shell") {
        return None;
    }
    let args: serde_json::Value = serde_json::from_str(args_str).ok()?;
    let command = args.get("command")?.as_str()?;
    detect(command, result)
}

#[cfg(test)]
#[path = "../../tests/unit/agent/runner_invocation/runner_invocation_test.rs"]
mod tests;
