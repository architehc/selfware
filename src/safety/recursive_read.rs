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
//!   entry (hidden ones included). A private discovery directory in that
//!   walk refuses the command — the binary would read it, and the vet
//!   cannot stop it;
//! - `git grep`: every entry except a `.git` directory, any case
//!   (tracked-ness is not checked: an untracked `.env` still refuses).
//!   Any other private discovery directory still refuses;
//! - `git show`, `git log -p`, `git diff`, `git cat-file -p`, …: git
//!   prints the content of every tracked path it selects — all of them
//!   without a pathspec, and a pathspec is a pattern git matches its own
//!   way (`'sec*'`, `*nv` reach `secrets/`, `.env`) — so the entries are
//!   the paths git itself lists ([`GitPaths`]): the index and every ref's
//!   tree for a work-tree diff, every path in any reachable commit for
//!   the rest (a blob id names none);
//! - `rg` with no flag that widens it: non-hidden entries, pruning only
//!   directories the repository's top-level `.gitignore` names with a plain
//!   pattern (and only when that file has no `!` re-include). `-u`,
//!   `-uu`, `--hidden`, `-.`, `--no-ignore*`, `--unrestricted`, and any
//!   `-g`/`--glob`/`--iglob`/`-t`/`--type*` (ripgrep's overrides beat its
//!   ignore rules) widen it to every entry.
//!
//! Symlinks are followed only where the program follows them (`grep -R`,
//! `rg -L`, `find -L`, `diff -r`, …), and a symlink's target text is
//! vetted either way. A followed symlink must resolve inside the workspace
//! (the path policy refuses the same file named through the link); one that
//! leads out of it is refused whatever its target is called. A root with
//! more than [`MAX_ENTRIES`] entries is refused as unvettable.

use std::path::{Path, PathBuf};

/// Entries walked per command before the command is refused as too large
/// to vet.
pub const MAX_ENTRIES: usize = 50_000;

/// How a program walks its search root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Walk {
    /// Every entry.
    All,
    /// Every entry except a `.git` directory (`git grep`), any case.
    SkipGitDir,
    /// ripgrep's default: no hidden entries, `.gitignore`d dirs pruned.
    RgDefault,
    /// ripgrep with ignore files off but hidden entries still skipped.
    NoHidden,
    /// No walk: the tracked paths git lists for the root's repository.
    Git(GitPaths),
}

/// What a content-printing git command can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPaths {
    /// The index and every ref's tree: `git diff` with no revision (or
    /// `HEAD`), `git status -v`.
    Index,
    /// Every path in the index or in any commit git can reach: `git log
    /// -p`, `git show`, `git diff <rev>`, `git cat-file -p` — an object id
    /// names no path, so nothing narrower is known.
    History,
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

fn is_two_token_long_flag(opt: &str) -> bool {
    matches!(
        opt,
        "--regexp"
            | "--file"
            | "--glob"
            | "--iglob"
            | "--type"
            | "--type-not"
            | "--type-add"
            | "--ignore-file"
            | "--ignore-file-case-insensitive"
            | "--max-count"
            | "--after-context"
            | "--before-context"
            | "--context"
            | "--threads"
            | "--pre"
            | "--pre-glob"
            | "--include"
            | "--exclude"
            | "--exclude-dir"
            | "--label"
            | "--max-depth"
            | "--max-filesize"
            | "--directories"
    )
}

fn is_two_token_short_flag(c: char) -> bool {
    matches!(
        c,
        'e' | 'f' | 'g' | 't' | 'T' | 'm' | 'A' | 'B' | 'C' | 'j' | 'd'
    )
}

fn scan_grep_tokens(args: &[String], is_rg: bool) -> (bool, Vec<String>, Option<Vec<String>>) {
    let mut has_pattern_flag = false;
    let mut before_dd = Vec::new();
    let mut after_dd: Option<Vec<String>> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(ref mut after) = after_dd {
            after.push(a.clone());
            i += 1;
            continue;
        }
        if a == "--" {
            after_dd = Some(Vec::new());
            i += 1;
            continue;
        }
        if let Some(long) = a.strip_prefix("--") {
            let (opt_name, has_eq) = match long.split_once('=') {
                Some((n, _)) => (n, true),
                None => (long, false),
            };
            let full_opt = &a[..2 + opt_name.len()];
            if matches!(full_opt, "--regexp" | "--file") || (is_rg && full_opt == "--files") {
                has_pattern_flag = true;
            }
            if !has_eq && is_two_token_long_flag(full_opt) {
                if full_opt == "--ignore-file-case-insensitive"
                    && args.get(i + 1).is_some_and(|next| next.starts_with('-'))
                {
                    i += 1;
                } else {
                    i += 2;
                }
                continue;
            }
            i += 1;
            continue;
        }
        if a.starts_with('-') && a != "-" {
            let short = &a[1..];
            let mut consumes_next = false;
            for (idx, c) in short.char_indices() {
                if c == 'e' || c == 'f' {
                    has_pattern_flag = true;
                }
                if is_two_token_short_flag(c) {
                    if idx + c.len_utf8() == short.len() {
                        consumes_next = true;
                    }
                    break;
                }
            }
            i += if consumes_next { 2 } else { 1 };
            continue;
        }
        before_dd.push(a.clone());
        i += 1;
    }
    (has_pattern_flag, before_dd, after_dd)
}

fn grep_operands(args: &[String], is_rg: bool) -> Vec<String> {
    let (has_pattern_flag, mut positionals, after_dd) = scan_grep_tokens(args, is_rg);
    if let Some(after) = after_dd {
        positionals.extend(after);
    }
    if !has_pattern_flag && !positionals.is_empty() {
        positionals.remove(0);
    }
    positionals
}

fn git_grep_operands(args: &[String]) -> Vec<String> {
    let (has_pattern_flag, mut before_dd, after_dd) = scan_grep_tokens(args, false);
    match after_dd {
        Some(mut after) => {
            if !has_pattern_flag && before_dd.is_empty() && !after.is_empty() {
                after.remove(0);
            }
            after
        }
        None => {
            if !has_pattern_flag && !before_dd.is_empty() {
                before_dd.remove(0);
            }
            if !before_dd.is_empty() && !before_dd.iter().any(|r| r == ".") {
                before_dd.push(".".to_string());
            }
            before_dd
        }
    }
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
            plan(Walk::All, follow, grep_operands(args, false), true)
        }
        "rg" => {
            let u = short_flags(args).filter(|c| *c == 'u').count()
                + long_opts(args).filter(|o| *o == "unrestricted").count();
            let overrides = has_short(args, 'g')
                || has_short(args, 't')
                || has_short(args, 'T')
                || long_opts(args).any(|o| {
                    matches!(
                        o,
                        "glob"
                            | "iglob"
                            | "type"
                            | "type-not"
                            | "type-add"
                            | "glob-case-insensitive"
                            | "ignore-file"
                            | "ignore-file-case-insensitive"
                            | "pre"
                            | "pre-glob"
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
            plan(walk, follow, grep_operands(args, true), true)
        }
        "ag" | "ack" | "pt" | "ugrep" | "ug" => {
            let follow = has_short(args, 'f') || has_long(args, "follow");
            plan(Walk::All, follow, operands(args), true)
        }
        // GNU diff compares directory operands and dereferences symlinks
        // unless told not to.
        "diff" | "colordiff" => {
            let follow = !has_long(args, "no-dereference");
            plan(Walk::All, follow, operands(args), false)
        }
        "cp" | "rsync" | "scp" => {
            let recursive = has_short(args, 'r')
                || has_short(args, 'R')
                || (prog != "scp" && has_short(args, 'a'))
                || has_long(args, "recursive")
                || has_long(args, "archive");
            // `scp -r` follows every symlink it meets.
            let follow = prog == "scp"
                || has_short(args, 'L')
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
        // Info-ZIP archives what a symlink points to unless `-y` keeps it
        // as a link.
        "zip" => (has_short(args, 'r') || has_long(args, "recurse-paths"))
            .then(|| {
                let follow = !has_short(args, 'y') && !has_long(args, "symlinks");
                plan(Walk::All, follow, operands(args), false)
            })
            .flatten(),
        "git" => {
            // `git [-C dir] [-c k=v] <subcommand> …`
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
            let sub = args.get(i)?;
            let rest = &args[i + 1..];
            // Operands are relative to a `-C dir`, which is walked too.
            let in_dir = |roots: Vec<String>| match &chdir {
                Some(dir) => {
                    let mut roots: Vec<String> =
                        roots.into_iter().map(|r| format!("{dir}/{r}")).collect();
                    roots.push(dir.clone());
                    roots
                }
                None => roots,
            };
            match sub.as_str() {
                "grep" => plan(
                    Walk::SkipGitDir,
                    false,
                    in_dir(git_grep_operands(rest)),
                    true,
                ),
                // `git diff --no-index a b` is a `diff -r` of the file system.
                "diff" if rest.iter().any(|a| a == "--no-index") => {
                    plan(Walk::All, false, in_dir(operands(rest)), false)
                }
                // Content printers: git lists the paths, the root is the
                // repository — a pathspec narrows nothing here (`'sec*'
                // README.md` would walk only `README.md`).
                _ => {
                    let scope = git_printed_paths(sub, rest)?;
                    let root = chdir.unwrap_or_else(|| ".".to_string());
                    plan(Walk::Git(scope), false, vec![root], false)
                }
            }
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

/// What a git read subcommand prints the CONTENT of — `None` when it prints
/// names, ids and messages only (`git log`, `git diff --stat`, `git
/// ls-files`). Conservative: a patch option anywhere counts, whatever a
/// later `--no-patch` undoes, and `git show` always prints something's
/// content (a commit's patch, a blob — whatever the format options say).
fn git_printed_paths(sub: &str, rest: &[String]) -> Option<GitPaths> {
    let patch = has_short(rest, 'p')
        || has_short(rest, 'u')
        || has_short(rest, 'c')
        || has_short(rest, 'U')
        || has_long(rest, "patch")
        || has_long(rest, "unified")
        || has_long(rest, "patch-with-stat")
        || has_long(rest, "patch-with-raw")
        || has_long(rest, "binary")
        || has_long(rest, "full-index")
        || long_opts(rest).any(|o| o == "cc");
    match sub {
        "show" => Some(GitPaths::History),
        "log" | "whatchanged" | "reflog" | "diff-tree" | "diff-index" | "diff-files" => {
            patch.then_some(GitPaths::History)
        }
        "diff" => {
            // A listing format replaces the patch unless an option adds it
            // back (`--stat -p`).
            let before_dd = || rest.iter().take_while(|a| *a != "--");
            let listing = before_dd().any(|a| a == "-s")
                || long_opts(rest).any(|o| {
                    matches!(
                        o,
                        "stat"
                            | "numstat"
                            | "shortstat"
                            | "dirstat"
                            | "compact-summary"
                            | "summary"
                            | "raw"
                            | "name-only"
                            | "name-status"
                            | "no-patch"
                            | "quiet"
                    )
                });
            if listing && !patch {
                return None;
            }
            // With no revision (anything before `--` that is not an option)
            // git compares the index and the work tree, `HEAD` at most.
            let revision = before_dd().any(|a| !a.starts_with('-') && a != "HEAD");
            Some(if revision {
                GitPaths::History
            } else {
                GitPaths::Index
            })
        }
        "status" => (has_short(rest, 'v') || has_long(rest, "verbose")).then_some(GitPaths::Index),
        "cat-file" => {
            let content = has_short(rest, 'p')
                || long_opts(rest).any(|o| o.starts_with("batch") && o != "batch-check")
                || rest.iter().any(|a| a == "blob");
            content.then_some(GitPaths::History)
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
/// `repo`, or nothing when `.gitignore`, `.ignore`, or `.rgignore`
/// re-includes anything (`!`) or `.gitignore` is missing.
fn gitignored_dirs(repo: &Path) -> Vec<DirPattern> {
    for extra in [".ignore", ".rgignore"] {
        match std::fs::read_to_string(repo.join(extra)) {
            Ok(extra_text) => {
                if extra_text
                    .lines()
                    .any(|line| line.trim_start().starts_with('!'))
                {
                    return Vec::new();
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Vec::new(),
        }
    }
    let Ok(text) = std::fs::read_to_string(repo.join(".gitignore")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with('!') || line.trim_start().starts_with('!') {
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

fn is_git_dir_name(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(".git"))
}

/// Private discovery state this walk cannot keep the program from reading.
///
/// Dot-skipping walks already omit every private discovery name, so only a
/// non-hidden symlink into that state is a hit. `git grep` prunes `.git`
/// before this runs; any other private directory stays in its walk because
/// tracked-ness is not checked. Full walks read every directory, including
/// `.git`.
fn walk_entry_is_private_discovery(walk: Walk, entry: &walkdir::DirEntry) -> bool {
    let named = !matches!(walk, Walk::NoHidden | Walk::RgDefault)
        && crate::safety::source_context::path_contains_private_discovery_state(entry.path());
    named || symlink_target_is_private_discovery(entry)
}

/// A symlink is not `is_dir()`, but its name was already covered by the path
/// check. The target text and, when resolution succeeds, the canonical target
/// are private discovery state the same way a sensitive link target is. A
/// nested loop or unreadable link is left alone: the program skips it, and
/// the root check is the fail-closed boundary.
fn symlink_target_is_private_discovery(entry: &walkdir::DirEntry) -> bool {
    if !entry.path_is_symlink() {
        return false;
    }
    let Ok(target) = std::fs::read_link(entry.path()) else {
        return false;
    };
    let joined = entry
        .path()
        .parent()
        .map(|parent| parent.join(&target))
        .unwrap_or_else(|| target.clone());
    let resolved = std::fs::canonicalize(entry.path());
    crate::safety::source_context::path_contains_private_discovery_state(&target)
        || crate::safety::source_context::path_contains_private_discovery_state(&joined)
        || resolved.is_ok_and(|path| {
            crate::safety::source_context::path_contains_private_discovery_state(&path)
        })
}

fn vet_root(
    plan: &Plan,
    base: &Path,
    root: &str,
    denied: &[String],
    budget: &mut usize,
) -> Option<String> {
    let root_path = base.join(root);
    if let Walk::Git(scope) = plan.walk {
        return git_paths_violation(&root_path, root, scope, denied, true, budget);
    }
    // Refuse before walking. Dropping a private root from `filter_entry`
    // yields nothing, so `grep -r KEY .claude` was allowed and a nested
    // `.env` was never seen. `is_dir()` is not consulted: a symlink root is
    // not a directory to walkdir, but its name or canonical target can still
    // be private discovery state, and a failed canonicalize is private.
    if crate::safety::source_context::discovery_root_is_private(&root_path) {
        return Some(format!(
            "`{}` would read `{root}` (private discovery state)",
            plan.program
        ));
    }
    let prune = if plan.walk == Walk::RgDefault {
        repo_root_of(&root_path).map(|repo| (gitignored_dirs(&repo), repo))
    } else {
        None
    };
    let canonical_root = std::fs::canonicalize(&root_path).ok();
    // The boundary the path policy vets operands against, symlinks
    // resolved, so a followed link is judged by where it really leads.
    let workspace = std::fs::canonicalize(crate::tools::workspace_root::current_path()).ok();
    let walk = walkdir::WalkDir::new(&root_path)
        .follow_links(plan.follow)
        .into_iter()
        // Depth 0 is the explicit root, already refused when private.
        // Prune only what the program itself skips. `git grep` skips `.git`
        // (any case, including a symlink — `is_dir()` is false for one).
        // Ripgrep's default skips dot entries. `grep -r` skips nothing, so a
        // private directory stays visible and is refused below; filtering it
        // out here would allow the command.
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name();
            match plan.walk {
                Walk::All => true,
                // Never walked: vetted above from git's own listing.
                Walk::Git(_) => true,
                Walk::SkipGitDir => !is_git_dir_name(name),
                Walk::NoHidden => !name.to_string_lossy().starts_with('.'),
                Walk::RgDefault => {
                    let name = name.to_string_lossy();
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
        // A symlink the program dereferences (every entry when it follows
        // links; the root, which the walk always enters) takes the read
        // wherever it points: the target must stay inside the workspace,
        // whatever it is called — `cat` of the same file through the link
        // is refused by the path policy. A dangling link reads nothing.
        if entry.path_is_symlink() && (plan.follow || entry.depth() == 0) {
            if let Ok(real) = std::fs::canonicalize(entry.path()) {
                if !workspace.as_ref().is_some_and(|ws| real.starts_with(ws)) {
                    return Some(format!(
                        "`{}` would follow `{rel}` inside `{root}` out of the workspace",
                        plan.program
                    ));
                }
            }
        }
        let is_dir = entry.file_type().is_dir();
        if entry.depth() > 0 && walk_entry_is_private_discovery(plan.walk, &entry) {
            return Some(format!(
                "`{}` would read `{rel}` inside `{root}` (private discovery state)",
                plan.program
            ));
        }
        // A recursively discovered hard link can give private or out-of-root
        // bytes an innocuous name. Refuse the whole recursive command when
        // the discovered file identity cannot be confined. Depth zero is an
        // explicitly selected file and retains the ordinary exact-read path.
        if entry.depth() > 0
            && entry.file_type().is_file()
            && !crate::safety::source_context::broad_discovery_file_is_safe(entry.path())
        {
            return Some(format!(
                "`{}` would read `{rel}` whose file identity is not safe for broad discovery",
                plan.program
            ));
        }
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

/// Why a content-printing git command run in `dir` (named `root` in the
/// reason) would print a denied — and, when `sensitive`, a sensitive —
/// tracked path: every path git lists for `scope` is vetted like a walked
/// entry. Git failing to list them refuses the command too.
fn git_paths_violation(
    dir: &Path,
    root: &str,
    scope: GitPaths,
    denied: &[String],
    sensitive: bool,
    budget: &mut usize,
) -> Option<String> {
    // Outside a repository git prints nothing tracked (it fails instead).
    let top = repo_root_of(dir)?;
    let paths = match git_listed_paths(dir, root, scope, budget) {
        Ok(paths) => paths,
        Err(why) => return Some(why),
    };
    for rel in paths {
        if let Some(token) = sensitive_component(&rel, false).filter(|_| sensitive) {
            return Some(format!(
                "`git` would print `{rel}`, tracked in `{root}` (sensitive: {token})"
            ));
        }
        let abs = top.join(&rel);
        let abs = abs.to_string_lossy();
        if let Some(glob) =
            crate::safety::yolo::denied_glob_among(&[rel.as_str(), abs.as_ref()], denied)
        {
            return Some(format!(
                "`git` would print `{rel}`, tracked in `{root}` (denied: {glob})"
            ));
        }
    }
    None
}

/// Why `git diff` run in `dir` (named `root` in the reason) would print a
/// path matching `denied` (`scope`: what the call compares) — the
/// `git_diff` tool's check, which like its `path` check applies the deny
/// globs only.
pub fn git_prints_denied_path(
    dir: &Path,
    root: &str,
    scope: GitPaths,
    denied: &[String],
) -> Option<String> {
    let mut budget = MAX_ENTRIES;
    git_paths_violation(dir, root, scope, denied, false, &mut budget)
}

/// The distinct paths git lists for `scope` in `dir`'s repository, as `git
/// rev-list --objects` prints them (relative to the top level; trees and
/// blobs alike), counted against `budget`. `Err` names why they cannot be
/// vetted: git failed (not a repository, …) or there are too many.
fn git_listed_paths(
    dir: &Path,
    root: &str,
    scope: GitPaths,
    budget: &mut usize,
) -> Result<Vec<String>, String> {
    use std::io::BufRead;
    let mut cmd =
        crate::safety::git_exec::git_command(dir, crate::safety::git_exec::GitScope::Internal);
    cmd.args(["rev-list", "--objects", "--indexed-objects"])
        .args(match scope {
            GitPaths::Index => ["--no-walk", "--all"],
            GitPaths::History => ["--all", "--reflog"],
        })
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let failed =
        |detail: String| format!("`git` cannot list what it would print in `{root}`: {detail}");
    let mut child = cmd.spawn().map_err(|e| failed(e.to_string()))?;
    let Some(stdout) = child.stdout.take() else {
        return Err(failed("no output".to_string()));
    };
    let mut seen = std::collections::HashSet::new();
    let mut paths = Vec::new();
    for line in std::io::BufReader::new(stdout).split(b'\n') {
        let Ok(line) = line else { break };
        // `<id> <path>`; commits and the root tree have no path.
        let Some(space) = line.iter().position(|b| *b == b' ') else {
            continue;
        };
        let path = String::from_utf8_lossy(&line[space + 1..]).into_owned();
        if path.is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        if *budget == 0 {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "`git` would print more than {MAX_ENTRIES} tracked paths in `{root}`; too many to vet"
            ));
        }
        *budget -= 1;
        paths.push(path);
    }
    match child.wait() {
        Ok(status) if status.success() => Ok(paths),
        Ok(status) => Err(failed(status.to_string())),
        Err(e) => Err(failed(e.to_string())),
    }
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
