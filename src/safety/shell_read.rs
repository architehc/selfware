//! What a `[reads]` shell command is — the one source of truth.
//!
//! A `[reads]` tag is not only a label: headless Normal and headless
//! AutoEdit approve such a command with nobody watching, and a prefix rule
//! seeded from it is reused unasked. The 0.9.4 classifier keyed on the first
//! program of each segment, skipped leading `VAR=…` words, and looked at
//! exact option words, so all of these were "reads" (0.9.5 review, C1/M1/M2):
//!
//! - `echo $(sh tools/x.sh)`, `` ls `./tools/x.sh` ``, `cat <(./tools/x.sh)`
//!   — command/process substitution runs anything;
//! - `GIT_EXTERNAL_DIFF=./tools/x.sh git diff`, `RIPGREP_CONFIG_PATH=… rg`
//!   — the environment makes a reader execute a program;
//! - `tree -ao victim.txt .` — a bundled short flag writes a file;
//! - `ps eww`, `ps -E` — print the agent's environment (API keys).
//!
//! This module parses a command with a quote-aware shell-word lexer (no
//! expansion is ever performed) and answers, fail-closed, whether it is a
//! plain read:
//!
//! - [`parse`] rejects command/process substitution, parameter expansion,
//!   ANSI-C quoting, brace expansion, subshells, heredocs, any redirection
//!   that writes somewhere other than `/dev/null`, and leading environment
//!   assignments other than [`SAFE_ENV`] with plain values;
//! - [`segment_is_read`] then requires every pipeline/list segment to run a
//!   program from the read allowlist with arguments that program's
//!   validator accepts (bundled short flags are expanded before every flag
//!   check).
//!
//! [`CONTENT_READERS`] / [`CONTENT_COPIERS`] are the single list of programs
//! that consume a file operand's contents; the checker's shell path policy
//! and the YOLO sensitive/denied-path heuristics both use it, so a reader
//! cannot be "read-only" for approval yet invisible to the path guards (M5:
//! `jq -R . .env`, `cut -c1- .env`, `git show HEAD:.env`).

/// Environment variables a read may set in front of its program. None of
/// them makes any allowlisted reader execute a program or read a file other
/// than its operands; values are further restricted to plain characters.
///
/// Everything else is refused — in particular `GIT_EXTERNAL_DIFF`,
/// `GIT_PAGER`, `PAGER`, `LESSOPEN`/`LESSCLOSE`, `GIT_SSH*`,
/// `RIPGREP_CONFIG_PATH`, `EDITOR`/`VISUAL`, `*_COMMAND`, `LD_PRELOAD`,
/// `PATH`, `GIT_CONFIG_*`, `GIT_DIR`.
pub const SAFE_ENV: &[&str] = &[
    "LANG",
    "LANGUAGE",
    "TZ",
    "NO_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
    "FORCE_COLOR",
    "COLUMNS",
    "LINES",
    "TERM",
    "CARGO_TERM_COLOR",
    "RUST_BACKTRACE",
    "PYTHONUNBUFFERED",
    "PYTHONDONTWRITEBYTECODE",
    "CI",
];

/// Programs that print a file operand's contents (or a digest/summary of
/// them). Shared by the checker's shell path policy (every operand of these
/// is a path candidate) and the YOLO sensitive/denied-path heuristics.
pub const CONTENT_READERS: &[&str] = &[
    "cat",
    "less",
    "more",
    "head",
    "tail",
    "bat",
    "nl",
    "tac",
    "xxd",
    "od",
    "hexdump",
    "strings",
    "base64",
    "base32",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "ag",
    "awk",
    "gawk",
    "mawk",
    "sed",
    "cut",
    "jq",
    "yq",
    "diff",
    "cmp",
    "comm",
    "column",
    "uniq",
    "sort",
    "paste",
    "join",
    "wc",
    "file",
    "stat",
    "md5sum",
    "sha1sum",
    "sha224sum",
    "sha256sum",
    "sha384sum",
    "sha512sum",
    "shasum",
    "md5",
    "cksum",
    "b2sum",
    "iconv",
    "fold",
    "fmt",
    "expand",
    "unexpand",
    "pr",
    "look",
    "rev",
    "zcat",
    "gzcat",
    "zless",
    "zgrep",
    "bzcat",
    "xzcat",
    "tree",
];

/// Programs that move a file operand's contents somewhere else (a copy, the
/// network, a git object read). Not reads, but a sensitive path under them
/// is exfiltration all the same, so the YOLO heuristics key on them too.
pub const CONTENT_COPIERS: &[&str] = &["cp", "rsync", "scp", "curl", "wget", "dd", "git"];

/// Whether `verb` (a program basename) consumes a file operand's contents:
/// [`CONTENT_READERS`] or [`CONTENT_COPIERS`].
pub fn consumes_file_contents(verb: &str) -> bool {
    CONTENT_READERS.contains(&verb) || CONTENT_COPIERS.contains(&verb)
}

/// One shell word after quote removal. No expansion is performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// The word with quotes and escapes removed.
    pub text: String,
    /// Contains an unquoted glob metacharacter (`*`, `?`, `[`).
    pub glob: bool,
}

/// One simple command of a list or pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Segment {
    /// Leading `NAME=value` assignments (all in [`SAFE_ENV`]).
    pub assignments: Vec<(String, String)>,
    /// The program and its arguments, redirections removed.
    pub words: Vec<Word>,
    /// Operands of `<` input redirections (files the command reads).
    pub inputs: Vec<String>,
}

impl Segment {
    /// The program word after the `time` / `command` wrappers, and the
    /// arguments after it.
    pub fn program(&self) -> Option<(&str, &[Word])> {
        let mut rest: &[Word] = &self.words;
        loop {
            let first = rest.first()?;
            match first.text.as_str() {
                "time" => rest = &rest[1..],
                "command"
                    if rest
                        .get(1)
                        .is_some_and(|w| !matches!(w.text.as_str(), "-v" | "-V")) =>
                {
                    rest = &rest[1..]
                }
                _ => return Some((first.text.as_str(), &rest[1..])),
            }
        }
    }

    /// Every word and input operand that could name a path: the arguments
    /// (without the program), `<` inputs, and for `REV:path` / `host:path`
    /// shapes the part after the last `:` as well.
    pub fn path_operands(&self) -> Vec<String> {
        let mut out = Vec::new();
        let args = self.program().map(|(_, a)| a).unwrap_or(&[]);
        for text in args
            .iter()
            .map(|w| w.text.as_str())
            .chain(self.inputs.iter().map(String::as_str))
        {
            out.push(text.to_string());
            if let Some((_, after)) = text.rsplit_once(':') {
                if !after.is_empty() && !text.contains("://") {
                    out.push(after.to_string());
                }
            }
            if let Some((_, after)) = text.split_once('=') {
                if text.starts_with('-') && !after.is_empty() {
                    out.push(after.to_string());
                }
            }
        }
        out
    }
}

#[derive(Debug)]
enum Tok {
    Word { word: Word, assign_ok: bool },
    Sep,
    Redir { op: String },
}

/// Parse `command` into segments without expanding anything. `Err` names
/// the first construct that disqualifies the command from being a plain
/// read (substitution, expansion, a writing redirection, an env
/// assignment outside [`SAFE_ENV`], …).
pub fn parse(command: &str) -> Result<Vec<Segment>, String> {
    let toks = lex(command)?;
    let mut segments = Vec::new();
    let mut cur = Segment::default();
    let mut iter = toks.into_iter().peekable();
    while let Some(tok) = iter.next() {
        match tok {
            Tok::Sep => {
                finish_segment(&mut segments, std::mem::take(&mut cur))?;
            }
            Tok::Redir { op } => {
                let target = match iter.next() {
                    Some(Tok::Word { word, .. }) => word,
                    _ => return Err(format!("redirection `{op}` without a target")),
                };
                match op.as_str() {
                    ">&" | "<&" => {
                        let t = target.text.as_str();
                        if !(t == "-" || (!t.is_empty() && t.chars().all(|c| c.is_ascii_digit()))) {
                            return Err(format!("redirection `{op}{t}` writes a file"));
                        }
                    }
                    ">" | ">>" | ">|" | "&>" | "&>>" => {
                        if target.text != "/dev/null" {
                            return Err(format!("redirection `{op}` writes `{}`", target.text));
                        }
                    }
                    "<" => cur.inputs.push(target.text),
                    other => return Err(format!("redirection `{other}`")),
                }
            }
            Tok::Word { word, assign_ok } => {
                if cur.words.is_empty() && assign_ok {
                    let (name, value) = word
                        .text
                        .split_once('=')
                        .map(|(n, v)| (n.to_string(), v.to_string()))
                        .unwrap_or_default();
                    let safe_name = SAFE_ENV.contains(&name.as_str()) || name.starts_with("LC_");
                    let safe_value = value.chars().all(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | ',')
                    });
                    if !(safe_name && safe_value) {
                        return Err(format!("sets environment variable `{name}`"));
                    }
                    cur.assignments.push((name, value));
                } else {
                    cur.words.push(word);
                }
            }
        }
    }
    finish_segment(&mut segments, cur)?;
    Ok(segments)
}

fn finish_segment(segments: &mut Vec<Segment>, seg: Segment) -> Result<(), String> {
    if seg.words.is_empty() {
        if !seg.assignments.is_empty() {
            return Err("bare variable assignment".to_string());
        }
        if !seg.inputs.is_empty() {
            return Err("redirection without a command".to_string());
        }
        return Ok(());
    }
    segments.push(seg);
    Ok(())
}

fn is_param_start(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '_' | '{' | '(' | '@' | '*' | '#' | '?' | '!' | '$' | '-')
}

// The closing `flush!()` resets state that is never read again.
#[allow(unused_assignments)]
fn lex(command: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = command.chars().collect();
    let mut toks = Vec::new();
    let mut text = String::new();
    // Characters that appeared unquoted, for assignment/brace detection.
    let mut unquoted = String::new();
    let mut in_word = false;
    let mut glob = false;
    let mut quoted_any = false;

    macro_rules! flush {
        () => {
            if in_word {
                if unquoted.contains('{') && (unquoted.contains(',') || unquoted.contains("..")) {
                    return Err("brace expansion".to_string());
                }
                let assign_ok = {
                    let name = unquoted.split('=').next().unwrap_or("");
                    unquoted.contains('=')
                        && text.starts_with(name)
                        && text[name.len()..].starts_with('=')
                        && !name.is_empty()
                        && !name.starts_with(|c: char| c.is_ascii_digit())
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                };
                toks.push(Tok::Word {
                    word: Word {
                        text: std::mem::take(&mut text),
                        glob,
                    },
                    assign_ok,
                });
                unquoted.clear();
                in_word = false;
                glob = false;
                quoted_any = false;
            }
        };
    }

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            ' ' | '\t' | '\r' => flush!(),
            '\n' => {
                flush!();
                toks.push(Tok::Sep);
            }
            '\\' => match next {
                Some('\n') => {
                    i += 2;
                    continue;
                }
                Some(n) => {
                    text.push(n);
                    in_word = true;
                    quoted_any = true;
                    i += 2;
                    continue;
                }
                None => return Err("trailing backslash".to_string()),
            },
            '\'' => {
                let Some(end) = chars[i + 1..].iter().position(|&x| x == '\'') else {
                    return Err("unterminated single quote".to_string());
                };
                text.extend(&chars[i + 1..i + 1 + end]);
                in_word = true;
                quoted_any = true;
                i += end + 2;
                continue;
            }
            '"' => {
                let mut j = i + 1;
                loop {
                    let Some(&d) = chars.get(j) else {
                        return Err("unterminated double quote".to_string());
                    };
                    match d {
                        '"' => break,
                        '\\' => match chars.get(j + 1) {
                            Some(&e) if matches!(e, '$' | '`' | '"' | '\\') => {
                                text.push(e);
                                j += 2;
                            }
                            Some('\n') => j += 2,
                            _ => {
                                text.push('\\');
                                j += 1;
                            }
                        },
                        '`' => return Err("command substitution".to_string()),
                        '$' if chars.get(j + 1).is_some_and(|&n| is_param_start(n)) => {
                            return Err("parameter expansion or substitution".to_string())
                        }
                        other => {
                            text.push(other);
                            j += 1;
                        }
                    }
                }
                in_word = true;
                quoted_any = true;
                i = j + 1;
                continue;
            }
            '`' => return Err("command substitution".to_string()),
            '$' => {
                if next.is_some_and(|n| is_param_start(n) || n == '\'' || n == '"') {
                    return Err("parameter expansion or substitution".to_string());
                }
                text.push('$');
                unquoted.push('$');
                in_word = true;
            }
            ';' => {
                flush!();
                toks.push(Tok::Sep);
            }
            '&' => {
                flush!();
                match next {
                    Some('&') => {
                        toks.push(Tok::Sep);
                        i += 2;
                        continue;
                    }
                    Some('>') => {
                        let op = if chars.get(i + 2) == Some(&'>') {
                            i += 3;
                            "&>>"
                        } else {
                            i += 2;
                            "&>"
                        };
                        toks.push(Tok::Redir { op: op.to_string() });
                        continue;
                    }
                    _ => toks.push(Tok::Sep),
                }
            }
            '|' => {
                flush!();
                toks.push(Tok::Sep);
                if matches!(next, Some('|') | Some('&')) {
                    i += 2;
                    continue;
                }
            }
            '(' | ')' => return Err("subshell or grouping".to_string()),
            '<' | '>' => {
                if next == Some('(') {
                    return Err("process substitution".to_string());
                }
                // A word made only of unquoted digits right before the
                // operator is its file descriptor (`2>`), not an argument.
                let fd_prefix = in_word
                    && !quoted_any
                    && !text.is_empty()
                    && text.chars().all(|d| d.is_ascii_digit());
                if fd_prefix {
                    text.clear();
                    unquoted.clear();
                    in_word = false;
                } else {
                    flush!();
                }
                let mut op = String::from(c);
                let mut j = i + 1;
                match (c, chars.get(j).copied()) {
                    ('>', Some('>')) | ('>', Some('|')) | ('>', Some('&')) => {
                        op.push(chars[j]);
                        j += 1;
                    }
                    ('<', Some('<')) => {
                        op.push('<');
                        j += 1;
                        if chars.get(j) == Some(&'<') {
                            op.push('<');
                            j += 1;
                        }
                    }
                    ('<', Some('>')) | ('<', Some('&')) => {
                        op.push(chars[j]);
                        j += 1;
                    }
                    _ => {}
                }
                toks.push(Tok::Redir { op });
                i = j;
                continue;
            }
            '#' if !in_word => {
                // Comment to end of line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '*' | '?' | '[' => {
                glob = true;
                text.push(c);
                unquoted.push(c);
                in_word = true;
            }
            other => {
                text.push(other);
                unquoted.push(other);
                in_word = true;
            }
        }
        i += 1;
    }
    flush!();
    Ok(toks)
}

/// Whether `command` is a plain read: it parses ([`parse`]) and every
/// segment passes [`segment_is_read`].
pub fn is_read_command(command: &str) -> bool {
    match parse(command) {
        Ok(segments) => !segments.is_empty() && segments.iter().all(segment_is_read),
        Err(_) => false,
    }
}

/// Short-option characters of `args` (bundles expanded: `-ao` → `a`, `o`),
/// stopping at `--`. Long options and operands contribute nothing.
fn short_flags(args: &[Word]) -> impl Iterator<Item = char> + '_ {
    args.iter()
        .map(|w| w.text.as_str())
        .take_while(|t| *t != "--")
        .filter(|t| t.starts_with('-') && !t.starts_with("--") && t.len() > 1)
        .flat_map(|t| t[1..].chars())
}

fn has_short(args: &[Word], flag: char) -> bool {
    short_flags(args).any(|c| c == flag)
}

/// Whether any long option in `args` is `--name`, `--name=…`, or an
/// abbreviation of it (GNU getopt and git's parse-options accept unique
/// prefixes: `--out` means `--output`). Prefixes shorter than three
/// characters are ignored (always ambiguous).
fn has_long(args: &[Word], name: &str) -> bool {
    args.iter()
        .map(|w| w.text.as_str())
        .take_while(|t| *t != "--")
        .filter_map(|t| t.strip_prefix("--"))
        .map(|t| t.split('=').next().unwrap_or(t))
        .any(|opt| opt == name || (opt.len() >= 3 && name.starts_with(opt)))
}

fn non_option_operands(args: &[Word]) -> usize {
    let mut n = 0;
    let mut after_dd = false;
    for t in args.iter().map(|w| w.text.as_str()) {
        if after_dd {
            n += 1;
        } else if t == "--" {
            after_dd = true;
        } else if !t.starts_with('-') || t == "-" {
            n += 1;
        }
    }
    n
}

/// Whether one parsed segment is a plain read: an allowlisted program whose
/// arguments cannot make it write, execute another program, or print the
/// process environment.
pub fn segment_is_read(seg: &Segment) -> bool {
    let Some((prog, args)) = seg.program() else {
        return false;
    };
    // A path to a program (`./tools/ls`, `/tmp/cat`) is whatever that file
    // is; a glob in program position is whatever it expands to.
    if prog.contains('/') || seg.words.iter().any(|w| w.glob && w.text == prog) {
        return false;
    }
    // Process introspection (`/proc/<pid>/environ`) prints environments.
    if args
        .iter()
        .map(|w| w.text.as_str())
        .chain(seg.inputs.iter().map(String::as_str))
        .any(|t| t.contains("/proc/") || t == "environ" || t.ends_with("/environ"))
    {
        return false;
    }
    let prog = prog.to_ascii_lowercase();
    match prog.as_str() {
        "cat" | "head" | "tail" | "wc" | "nl" | "tac" | "cut" | "column" | "comm" | "strings"
        | "stat" | "du" | "df" | "md5sum" | "sha1sum" | "sha256sum" | "sha512sum" | "shasum"
        | "md5" | "cksum" | "basename" | "dirname" | "readlink" | "realpath" | "pwd" | "uname"
        | "nproc" | "whoami" | "which" | "echo" | "uptime" | "ls" | "grep" | "egrep" | "fgrep"
        | "diff" | "true" => true,
        "cd" => {
            // `cd ..` / `cd /elsewhere` / bare `cd` ($HOME) move the rest of
            // the chain outside the workspace the path policy vets.
            args.len() == 1
                && !args[0].glob
                && !args[0].text.starts_with(['/', '~', '-'])
                && !args[0].text.split('/').any(|c| c == "..")
        }
        "date" => !has_short(args, 's') && !has_long(args, "set"),
        "file" => !has_short(args, 'C') && !has_long(args, "compile"),
        "rg" => {
            !args.iter().any(|w| w.text.starts_with("--pre")) && !has_long(args, "hostname-bin")
        }
        "find" => !args.iter().any(|w| {
            matches!(
                w.text.as_str(),
                "-exec"
                    | "-execdir"
                    | "-ok"
                    | "-okdir"
                    | "-delete"
                    | "-fprint"
                    | "-fprint0"
                    | "-fprintf"
                    | "-fls"
            )
        }),
        "tree" => !has_short(args, 'o') && !has_short(args, 'R') && !has_long(args, "output"),
        "uniq" => non_option_operands(args) <= 1,
        "jq" => !args
            .iter()
            .any(|w| w.text.to_ascii_lowercase().contains("env")),
        "sed" => {
            let texts: Vec<&str> = args.iter().map(|w| w.text.as_str()).collect();
            crate::safety::confirm_view::sed_script_is_plain_print(&texts)
        }
        "less" | "more" => args
            .iter()
            .all(|w| !w.text.starts_with('-') && !w.text.starts_with('+')),
        "ps" => ps_args_are_read(args),
        "top" => true,
        // `command -v x` / `-V` only looks a name up.
        "command" => args
            .first()
            .is_some_and(|w| matches!(w.text.as_str(), "-v" | "-V")),
        "git" => git_args_are_read(args),
        "xdotool" => xdotool_args_are_read(args),
        "wmctrl" => wmctrl_args_are_read(args),
        _ => false,
    }
}

/// `ps` without any environment-printing form: BSD `e` (`ps eww`, `ps
/// auxe`), `-E`, and on BSD/macOS `-e` (FreeBSD: "display the environment
/// as well"). On Linux dashed `-e` is "every process" and stays a read.
fn ps_args_are_read(args: &[Word]) -> bool {
    args.iter().all(|w| {
        let t = w.text.as_str();
        if t.to_ascii_lowercase().contains("env") {
            return false;
        }
        if let Some(long) = t.strip_prefix("--") {
            return !long.is_empty();
        }
        if let Some(short) = t.strip_prefix('-') {
            if short.contains('E') {
                return false;
            }
            return cfg!(target_os = "linux") || !short.contains('e');
        }
        // BSD-style option word (no dash): `e` prints the environment.
        !(t.chars().all(|c| c.is_ascii_alphabetic()) && t.contains(['e', 'E']))
    })
}

/// Git subcommands that only read the repository. Anything else — every
/// alias (`git st` can be `!sh …`), `difftool`, `mergetool`, `bisect run`,
/// `archive -o`, `config` (prints credential helpers and tokens) — is not a
/// read. Repository configuration can still make some of these run a
/// program (`core.fsmonitor`, textconv, filters); headless approval checks
/// the repository config separately (`crate::safety::git_exec`).
const GIT_READ_SUBCOMMANDS: &[&str] = &[
    "status",
    "diff",
    "log",
    "show",
    "ls-files",
    "ls-tree",
    "rev-parse",
    "cat-file",
    "blame",
    "annotate",
    "shortlog",
    "describe",
    "rev-list",
    "show-ref",
    "for-each-ref",
    "merge-base",
    "name-rev",
    "whatchanged",
    "count-objects",
    "diff-tree",
    "diff-files",
    "diff-index",
    "check-ignore",
    "check-attr",
    "grep",
    "branch",
    "reflog",
    "remote",
    "version",
];

/// Long git options that run a program or write a file, on any read
/// subcommand (`--output=`, `--ext-diff`, `--textconv`, `grep
/// --open-files-in-pager=`, `cat-file --filters`, …).
const GIT_DANGEROUS_LONG: &[&str] = &[
    "output",
    "output-directory",
    "ext-diff",
    "textconv",
    "open-files-in-pager",
    "exec",
    "upload-pack",
    "receive-pack",
    "filters",
];

fn git_args_are_read(args: &[Word]) -> bool {
    // Global options before the subcommand: only ones that cannot point git
    // at another repository, config, pager or exec path.
    let mut i = 0;
    while let Some(w) = args.get(i) {
        match w.text.as_str() {
            "--no-pager"
            | "-P"
            | "--no-optional-locks"
            | "--literal-pathspecs"
            | "--no-replace-objects" => i += 1,
            t if t.starts_with('-') => return false,
            _ => break,
        }
    }
    let Some(sub) = args.get(i).map(|w| w.text.as_str()) else {
        return false;
    };
    if !GIT_READ_SUBCOMMANDS.contains(&sub) {
        return false;
    }
    let rest = &args[i + 1..];
    // `--text` is a real diff option that is also a prefix of `--textconv`;
    // exact option names win over abbreviations in git.
    let dangerous = rest
        .iter()
        .map(|w| w.text.as_str())
        .take_while(|t| *t != "--")
        .filter_map(|t| t.strip_prefix("--"))
        .map(|t| t.split('=').next().unwrap_or(t))
        .any(|opt| {
            opt != "text"
                && GIT_DANGEROUS_LONG
                    .iter()
                    .any(|d| opt == *d || (opt.len() >= 3 && d.starts_with(opt)))
        });
    if dangerous {
        return false;
    }
    match sub {
        "grep" => !has_short(rest, 'O'),
        "branch" => {
            let destructive = short_flags(rest).any(|c| "dDmMcCfut".contains(c))
                || [
                    "delete",
                    "move",
                    "copy",
                    "force",
                    "set-upstream-to",
                    "unset-upstream",
                    "edit-description",
                    "track",
                    "create-reflog",
                ]
                .iter()
                .any(|o| has_long(rest, o));
            let lists = has_short(rest, 'l')
                || [
                    "list",
                    "contains",
                    "no-contains",
                    "merged",
                    "no-merged",
                    "points-at",
                ]
                .iter()
                .any(|o| has_long(rest, o));
            !destructive && (lists || non_option_operands(rest) == 0)
        }
        "reflog" => rest
            .first()
            .is_none_or(|w| w.text == "show" || w.text.starts_with('-')),
        "remote" => rest
            .iter()
            .all(|w| matches!(w.text.as_str(), "-v" | "--verbose")),
        _ => true,
    }
}

fn xdotool_args_are_read(args: &[Word]) -> bool {
    let Some(sub) = args.first().map(|w| w.text.as_str()) else {
        return false;
    };
    let query = sub.starts_with("get") || sub == "search";
    // xdotool chains commands in one invocation: `xdotool search x
    // windowkill` closes a window, `exec` runs a program.
    query
        && !args[1..].iter().any(|w| {
            let t = w.text.as_str();
            t.starts_with("window")
                || t.starts_with("key")
                || t.starts_with("mouse")
                || t.starts_with("set_")
                || t.starts_with("behave")
                || matches!(t, "type" | "click" | "exec" | "sleep")
        })
}

fn wmctrl_args_are_read(args: &[Word]) -> bool {
    let mutating = args.iter().any(|w| {
        let t = w.text.as_str();
        [
            "-r", "-c", "-s", "-k", "-a", "-e", "-b", "-o", "-n", "-R", "-t", "-T", "-N", "-I",
        ]
        .iter()
        .any(|f| t.starts_with(f))
    });
    let query = args
        .iter()
        .any(|w| w.text.starts_with("-l") || w.text == "-d" || w.text == "-m");
    query && !mutating
}

#[cfg(test)]
#[path = "../../tests/unit/safety/shell_read/shell_read_test.rs"]
mod tests;
