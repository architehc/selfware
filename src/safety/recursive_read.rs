//! Recursive readers: what a `grep -r` / `rg` / `find … -exec cat` would
//! read inside its search root.
//!
//! The headless read approval and the YOLO floor vet every path OPERAND a
//! reader names (`cat .env` is refused), but a recursive reader names a
//! directory and reads every file below it: `grep -r KEY .` read `.env`,
//! `rg -uu token` read `.ssh/id_rsa`, `find . -type f -exec cat {} +` read
//! everything (0.9.5 known issue). Rewriting such a command with exclusions
//! (`rg --glob '!…'`, `grep --exclude-dir`) is not provably complete — the
//! glob dialects differ from the deny-glob dialect and from each other —
//! so this module REFUSES instead: it walks each search root the way the
//! program would and reports the first entry that is denied
//! (`safety.denied_paths`) or sensitive (the YOLO sensitive list, matched
//! per path component).
//!
//! The walk is conservative — it may see more than the program would,
//! never less:
//!
//! - `grep -r`/`-R`, `diff -r`, `ag`, `cp -r`, `rsync -r`/`-a`, `scp -r`,
//!   `tar -c`, `zip -r`, and `find` feeding `-exec`/`-ok`/`xargs`: every
//!   entry (hidden ones included);
//! - `git grep`: every entry except the `.git` directory (tracked-ness is
//!   not checked: an untracked `.env` still refuses);
//! - `rg` with no flag that widens it: non-hidden entries, pruning only
//!   directories the repository's top-level `.gitignore` names with a plain
//!   pattern (and only when that file has no `!` re-include). `-u`,
//!   `-uu`, `--hidden`, `-.`, `--no-ignore*`, `--unrestricted`, and any
//!   `-g`/`--glob`/`--iglob`/`-t`/`--type*` (ripgrep's overrides beat its
//!   ignore rules) widen it to every entry.
//!
//! Symlinks are followed only where the program follows them (`grep -R`,
//! `rg -L`, `find -L`, …), and a symlink's target text is vetted either
//! way. A root with more than [`MAX_ENTRIES`] entries is refused as
//! unvettable.

use std::path::{Path, PathBuf};

/// Entries walked per command before the command is refused as too large
/// to vet.
pub const MAX_ENTRIES: usize = 50_000;

/// How a program walks its search root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Walk {
    /// Every entry.
    All,
    /// Every entry except a `.git` directory (`git grep`).
    SkipGitDir,
    /// ripgrep's default: no hidden entries, `.gitignore`d dirs pruned.
    RgDefault,
    /// ripgrep with ignore files off but hidden entries still skipped.
    NoHidden,
}

#[derive(Debug)]
struct Plan {
    program: String,
    walk: Walk,
    follow: bool,
    roots: Vec<String>,
    /// Search "." when no operand names an existing path.
    default_dot: bool,
}

/// Leading wrappers that run the rest of the words as a command.
fn strip_wrappers(words: &[String]) -> &[String] {
    let mut rest = words;
    loop {
        let Some(first) = rest.first() else {
            return rest;
        };
        let base = first.rsplit('/').next().unwrap_or(first);
        let is_assignment = first.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty() && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
        });
        if is_assignment {
            rest = &rest[1..];
            continue;
        }
        match base {
            "time" | "command" | "nohup" | "exec" | "builtin" => rest = &rest[1..],
            "env" | "nice" | "stdbuf" | "ionice" => {
                rest = &rest[1..];
                // Options (and `-n 10`-style values) before the command.
                while let Some(w) = rest.first() {
                    if w.starts_with('-') {
                        let takes_value =
                            matches!(w.as_str(), "-n" | "-u" | "-i" | "-o" | "-e" | "-c");
                        rest = &rest[1..];
                        if takes_value && !rest.is_empty() {
                            rest = &rest[1..];
                        }
                    } else if w.contains('=') && base == "env" {
                        rest = &rest[1..];
                    } else {
                        break;
                    }
                }
            }
            "timeout" => {
                rest = &rest[1..];
                while let Some(w) = rest.first().filter(|w| w.starts_with('-')) {
                    let takes_value = matches!(w.as_str(), "-k" | "-s");
                    rest = &rest[1..];
                    if takes_value && !rest.is_empty() {
                        rest = &rest[1..];
                    }
                }
                if !rest.is_empty() {
                    rest = &rest[1..]; // the duration
                }
            }
            _ => return rest,
        }
    }
}

fn short_flags(args: &[String]) -> impl Iterator<Item = char> + '_ {
    args.iter()
        .take_while(|t| t.as_str() != "--")
        .filter(|t| t.starts_with('-') && !t.starts_with("--") && t.len() > 1)
        .flat_map(|t| t[1..].chars())
}

fn has_short(args: &[String], c: char) -> bool {
    short_flags(args).any(|f| f == c)
}

fn long_opts(args: &[String]) -> impl Iterator<Item = &str> + '_ {
    args.iter()
        .take_while(|t| t.as_str() != "--")
        .filter_map(|t| t.strip_prefix("--"))
        .map(|t| t.split('=').next().unwrap_or(t))
}

fn has_long(args: &[String], name: &str) -> bool {
    long_opts(args).any(|o| o == name || (o.len() >= 3 && name.starts_with(o)))
}

/// Non-option words (every word after `--`).
fn operands(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut after_dd = false;
    for a in args {
        if after_dd {
            out.push(a.clone());
        } else if a == "--" {
            after_dd = true;
        } else if !a.starts_with('-') || a == "-" {
            out.push(a.clone());
        }
    }
    out
}

/// The walk plan for one simple command, `None` when it reads no directory
/// recursively. `piped_xargs`: some segment of the command runs `xargs`.
fn plan_for(words: &[String], piped_xargs: bool, assignments: &[String]) -> Option<Plan> {
    let words = strip_wrappers(words);
    let (prog, args) = words.split_first()?;
    let prog = prog.rsplit('/').next().unwrap_or(prog).to_ascii_lowercase();
    let plan = |walk, follow, roots, default_dot| {
        Some(Plan {
            program: prog.clone(),
            walk,
            follow,
            roots,
            default_dot,
        })
    };
    match prog.as_str() {
        "grep" | "egrep" | "fgrep" | "zgrep" => {
            let directories_recurse = args.windows(2).any(|w| w[0] == "-d" && w[1] == "recurse")
                || args.iter().any(|a| {
                    a == "-drecurse" || a == "--directories=recurse" || a == "--directories=r"
                });
            let recursive = has_short(args, 'r')
                || has_short(args, 'R')
                || has_long(args, "recursive")
                || has_long(args, "dereference-recursive")
                || directories_recurse;
            if !recursive {
                return None;
            }
            let follow = has_short(args, 'R')
                || has_long(args, "dereference-recursive")
                || has_short(args, 'S');
            plan(Walk::All, follow, operands(args), true)
        }
        "rg" => {
            let u = short_flags(args).filter(|c| *c == 'u').count()
                + long_opts(args).filter(|o| *o == "unrestricted").count();
            let overrides = has_short(args, 'g')
                || has_short(args, 't')
                || long_opts(args).any(|o| {
                    matches!(
                        o,
                        "glob" | "iglob" | "type" | "type-add" | "glob-case-insensitive"
                    )
                })
                || assignments
                    .iter()
                    .any(|a| a.starts_with("RIPGREP_CONFIG_PATH="));
            let hidden = u >= 2 || has_long(args, "hidden") || has_short(args, '.');
            let no_ignore = u >= 1 || long_opts(args).any(|o| o.starts_with("no-ignore"));
            let walk = if overrides || hidden {
                Walk::All
            } else if no_ignore {
                Walk::NoHidden
            } else {
                Walk::RgDefault
            };
            let follow = has_short(args, 'L') || has_long(args, "follow");
            plan(walk, follow, operands(args), true)
        }
        "ag" | "ack" | "pt" | "ugrep" | "ug" => {
            let follow = has_short(args, 'f') || has_long(args, "follow");
            plan(Walk::All, follow, operands(args), true)
        }
        "diff" | "colordiff" => (has_short(args, 'r') || has_long(args, "recursive"))
            .then(|| plan(Walk::All, false, operands(args), false))
            .flatten(),
        "cp" | "rsync" | "scp" => {
            let recursive = has_short(args, 'r')
                || has_short(args, 'R')
                || (prog != "scp" && has_short(args, 'a'))
                || has_long(args, "recursive")
                || has_long(args, "archive");
            let follow = has_short(args, 'L')
                || has_long(args, "dereference")
                || has_long(args, "copy-links");
            recursive
                .then(|| plan(Walk::All, follow, operands(args), false))
                .flatten()
        }
        "tar" | "bsdtar" | "gtar" => {
            let first_bundle = args.first().filter(|a| !a.starts_with('-'));
            let create = has_short(args, 'c')
                || has_long(args, "create")
                || first_bundle.is_some_and(|b| b.contains('c'));
            let follow = has_short(args, 'h') || has_long(args, "dereference");
            create
                .then(|| plan(Walk::All, follow, operands(args), false))
                .flatten()
        }
        "zip" => (has_short(args, 'r') || has_long(args, "recurse-paths"))
            .then(|| plan(Walk::All, false, operands(args), false))
            .flatten(),
        "git" => {
            // `git [-C dir] [-c k=v] grep …`
            let mut i = 0;
            let mut chdir: Option<String> = None;
            while i < args.len() && args[i].starts_with('-') {
                if matches!(
                    args[i].as_str(),
                    "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace"
                ) {
                    if args[i] == "-C" {
                        chdir = args.get(i + 1).cloned();
                    }
                    i += 1;
                }
                i += 1;
            }
            if args.get(i).map(String::as_str) != Some("grep") {
                return None;
            }
            let rest = &args[i + 1..];
            let mut roots = operands(rest);
            if let Some(dir) = chdir {
                roots = roots.into_iter().map(|r| format!("{dir}/{r}")).collect();
                roots.push(dir);
            }
            plan(Walk::SkipGitDir, false, roots, true)
        }
        "find" => {
            let executes = args
                .iter()
                .any(|a| matches!(a.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir"));
            if !executes && !piped_xargs {
                return None;
            }
            let follow = args.iter().any(|a| a == "-L" || a == "-follow");
            let roots: Vec<String> = args
                .iter()
                .skip_while(|a| {
                    matches!(a.as_str(), "-H" | "-L" | "-P")
                        || a.starts_with("-O")
                        || a.starts_with("-D")
                })
                .take_while(|a| !a.starts_with('-') && *a != "(" && *a != "!")
                .cloned()
                .collect();
            plan(Walk::All, follow, roots, true)
        }
        _ => None,
    }
}

/// A plain directory pattern of the top-level `.gitignore` (`target`,
/// `/target`, `node_modules/`), `anchored` when it starts with `/`.
struct DirPattern {
    name: String,
    anchored: bool,
}

/// Directory patterns ripgrep's default walk certainly skips below
/// `repo`, or nothing when the file re-includes anything (`!`) or is
/// missing.
fn gitignored_dirs(repo: &Path) -> Vec<DirPattern> {
    let Ok(text) = std::fs::read_to_string(repo.join(".gitignore")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with('!') {
            return Vec::new();
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let anchored = line.starts_with('/');
        let name = line.trim_start_matches('/').trim_end_matches('/');
        if name.is_empty()
            || name.contains(['*', '?', '[', '\\', '/'])
            || name.starts_with('.') && name.len() <= 2
        {
            continue;
        }
        out.push(DirPattern {
            name: name.to_string(),
            anchored,
        });
    }
    out
}

fn repo_root_of(path: &Path) -> Option<PathBuf> {
    let start = std::fs::canonicalize(path).ok()?;
    start
        .ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Why walking `plans` would read a denied or sensitive path, if it would.
fn vet_plans(plans: &[Plan], base: &Path, denied: &[String]) -> Option<String> {
    let mut budget = MAX_ENTRIES;
    for plan in plans {
        let mut roots: Vec<String> = plan
            .roots
            .iter()
            .filter(|r| {
                let p = base.join(r);
                std::fs::symlink_metadata(&p).is_ok()
            })
            .cloned()
            .collect();
        if roots.is_empty() && plan.default_dot {
            roots.push(".".to_string());
        }
        for root in roots {
            if let Some(why) = vet_root(plan, base, &root, denied, &mut budget) {
                return Some(why);
            }
        }
    }
    None
}

fn vet_root(
    plan: &Plan,
    base: &Path,
    root: &str,
    denied: &[String],
    budget: &mut usize,
) -> Option<String> {
    let root_path = base.join(root);
    let prune = if plan.walk == Walk::RgDefault {
        repo_root_of(&root_path).map(|repo| (gitignored_dirs(&repo), repo))
    } else {
        None
    };
    let canonical_root = std::fs::canonicalize(&root_path).ok();
    let walk = walkdir::WalkDir::new(&root_path)
        .follow_links(plan.follow)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            match plan.walk {
                Walk::All => true,
                Walk::SkipGitDir => !(name == ".git" && e.file_type().is_dir()),
                Walk::NoHidden => !name.starts_with('.'),
                Walk::RgDefault => {
                    if name.starts_with('.') {
                        return false;
                    }
                    let Some((dirs, repo)) = &prune else {
                        return true;
                    };
                    if !e.file_type().is_dir() {
                        return true;
                    }
                    let rel_to_repo = canonical_root
                        .as_ref()
                        .and_then(|c| e.path().strip_prefix(&root_path).ok().map(|r| c.join(r)))
                        .and_then(|abs| abs.strip_prefix(repo).ok().map(Path::to_path_buf));
                    !dirs.iter().any(|d| {
                        if d.anchored {
                            rel_to_repo.as_deref() == Some(Path::new(&d.name))
                        } else {
                            name == d.name.as_str()
                        }
                    })
                }
            }
        });
    for entry in walk {
        if *budget == 0 {
            return Some(format!(
                "`{}` searches more than {MAX_ENTRIES} entries under `{root}`; too many to vet",
                plan.program
            ));
        }
        *budget -= 1;
        let entry = match entry {
            Ok(e) => e,
            // Unreadable entry or a symlink loop: the program would skip
            // or fail it too.
            Err(_) => continue,
        };
        let rel = display_path(root, &root_path, entry.path());
        let is_dir = entry.file_type().is_dir();
        let mut candidates = vec![rel.clone()];
        if entry.path_is_symlink() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                candidates.push(target.to_string_lossy().into_owned());
            }
        }
        for cand in &candidates {
            if let Some(token) = sensitive_component(cand, is_dir) {
                return Some(format!(
                    "`{}` would read `{rel}` inside `{root}` (sensitive: {token})",
                    plan.program
                ));
            }
            let abs = base.join(cand);
            let abs = abs.to_string_lossy();
            if let Some(glob) =
                crate::safety::yolo::denied_glob_among(&[cand.as_str(), abs.as_ref()], denied)
            {
                return Some(format!(
                    "`{}` would read `{rel}` inside `{root}` (denied: {glob})",
                    plan.program
                ));
            }
        }
    }
    None
}

/// `entry` as the command would name it: `root` + the part below it.
fn display_path(root: &str, root_path: &Path, entry: &Path) -> String {
    let below = entry.strip_prefix(root_path).unwrap_or(entry);
    let joined = if below.as_os_str().is_empty() {
        PathBuf::from(root)
    } else {
        Path::new(root).join(below)
    };
    let s = joined.to_string_lossy().into_owned();
    let s = s.strip_prefix("./").map(str::to_string).unwrap_or(s);
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

/// The sensitive token a path matches, per path component (so
/// `src/environment.rs` is not `/environ`, but `.ssh/`, `secrets/`,
/// `.env`, `id_rsa`, `*.pem`, `.git-credentials`, … are).
pub fn sensitive_component(path: &str, is_dir: bool) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase().replace('\\', "/");
    let comps: Vec<&str> = lower
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .collect();
    let last = comps.last().copied().unwrap_or("");
    let dirs: &[&str] = if is_dir {
        &comps
    } else {
        &comps[..comps.len().saturating_sub(1)]
    };
    if dirs.contains(&".ssh") || (is_dir && last == ".ssh") {
        return Some(".ssh/");
    }
    if dirs.contains(&"secrets") {
        return Some("/secrets/");
    }
    if lower.contains(".aws/credentials") {
        return Some(".aws/credentials");
    }
    if lower.contains(".selfware/skills")
        || lower.contains(".selfware/commands")
        || lower.contains(".selfware/killswitch")
    {
        return Some(".selfware");
    }
    if is_dir {
        return None;
    }
    let name = last;
    if name == ".env" || name.starts_with(".env.") || name.ends_with(".env") {
        return Some(".env");
    }
    for (token, hit) in [
        ("id_rsa", name.starts_with("id_rsa")),
        ("id_ed25519", name.starts_with("id_ed25519")),
        ("id_ecdsa", name.starts_with("id_ecdsa")),
        (".netrc", name == ".netrc"),
        (".git-credentials", name == ".git-credentials"),
        ("private_key", name.contains("private_key")),
        (".pem", name.ends_with(".pem")),
        ("/environ", name == "environ"),
        (".admitted_ledger.json", name == ".admitted_ledger.json"),
    ] {
        if hit {
            return Some(token);
        }
    }
    None
}

/// Commands whose words are known: one `Vec` per simple command, with its
/// leading `NAME=value` assignments apart.
pub struct Command {
    pub assignments: Vec<String>,
    pub words: Vec<String>,
}

impl Command {
    pub fn from_segment(seg: &crate::safety::shell_read::Segment) -> Self {
        Self {
            assignments: seg
                .assignments
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect(),
            words: seg.words.iter().map(|w| w.text.clone()).collect(),
        }
    }
}

/// Why `commands`, run in `base`, would recursively read a denied or
/// sensitive path — `None` when no recursive reader reaches one. A `cd`
/// moves the base for the commands after it.
pub fn recursive_read_violation(
    commands: &[Command],
    base: &Path,
    denied: &[String],
) -> Option<String> {
    let piped_xargs = commands.iter().any(|c| {
        strip_wrappers(&c.words)
            .first()
            .is_some_and(|p| p.rsplit('/').next() == Some("xargs"))
    });
    let mut base = base.to_path_buf();
    for cmd in commands {
        let words = strip_wrappers(&cmd.words);
        if let Some((prog, args)) = words.split_first() {
            if prog == "cd" {
                if let Some(dir) = args.first() {
                    base = base.join(dir);
                }
                continue;
            }
            // `sh -c '…'`, `bash -lc '…'`: vet the inner command too.
            let shell = prog.rsplit('/').next().unwrap_or(prog);
            if matches!(shell, "sh" | "bash" | "zsh" | "dash" | "ksh") {
                if let Some(pos) = args
                    .iter()
                    .position(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('c'))
                {
                    if let Some(inner) = args.get(pos + 1) {
                        let nested = lenient_commands(inner);
                        if let Some(why) = recursive_read_violation(&nested, &base, denied) {
                            return Some(why);
                        }
                    }
                }
            }
        }
        if let Some(plan) = plan_for(&cmd.words, piped_xargs, &cmd.assignments) {
            if let Some(why) = vet_plans(std::slice::from_ref(&plan), &base, denied) {
                return Some(why);
            }
        }
    }
    None
}

/// Best-effort split of an arbitrary shell command into simple commands
/// (for the YOLO floor, which sees commands the strict parser rejects):
/// unquoted `;`, `&`, `|`, newlines, parentheses and backticks separate
/// commands; `$(` opens a new one. Words are shell-split when possible.
pub fn lenient_commands(cmd: &str) -> Vec<Command> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                } else if c == '\\' && q == '"' {
                    if let Some(n) = chars.next() {
                        cur.push(n);
                    }
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '\\' => {
                    cur.push(c);
                    if let Some(n) = chars.next() {
                        cur.push(n);
                    }
                }
                ';' | '&' | '|' | '\n' | '(' | ')' | '`' | '{' | '}' => {
                    parts.push(std::mem::take(&mut cur));
                }
                '$' if chars.peek() == Some(&'(') => {
                    chars.next();
                    parts.push(std::mem::take(&mut cur));
                }
                _ => cur.push(c),
            },
        }
    }
    parts.push(cur);
    parts
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let words = shlex::split(&p).unwrap_or_else(|| {
                p.split_whitespace()
                    .map(|w| w.trim_matches(['\'', '"']).to_string())
                    .collect()
            });
            let split = words
                .iter()
                .position(|w| {
                    !w.split_once('=').is_some_and(|(n, _)| {
                        !n.is_empty() && n.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
                    })
                })
                .unwrap_or(words.len());
            Command {
                assignments: words[..split].to_vec(),
                words: words[split..].to_vec(),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/safety/recursive_read/recursive_read_test.rs"]
mod tests;
