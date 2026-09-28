use super::*;
use crate::safety::confirm_view::{classify_shell_risk, RiskTag};

/// The 0.9.5 review's probe commands: every one was `[reads]` in 0.9.4 and
/// ran unasked under headless Normal/AutoEdit. None may be a read.
const PROBE_NOT_READS: &[&str] = &[
    // C1: substitution runs anything
    "echo $(sh tools/x.sh)",
    "ls `./tools/x.sh`",
    "cat <(./tools/x.sh)",
    "diff <(ls) >(sh)",
    "echo \"$(sh tools/x.sh)\"",
    "echo \"`id`\"",
    "cat ${HOME}/x",
    "cat $F",
    "echo $'\\x41'",
    // C1: environment makes a reader execute a program
    "GIT_EXTERNAL_DIFF=./tools/x.sh git diff HEAD~1",
    "RIPGREP_CONFIG_PATH=./tools/rgrc rg foo",
    "GIT_PAGER=./x git log",
    "PAGER=./x git log",
    "LESSOPEN='|./x %s' less README.md",
    "GIT_SSH_COMMAND=./x git log",
    "EDITOR=./x git log",
    "LD_PRELOAD=./evil.so ls",
    "PATH=./tools:$PATH ls",
    "env GIT_EXTERNAL_DIFF=./x git diff",
    "env",
    "F=.env",
    // M1: bundled / abbreviated writing flags
    "tree -ao victim.txt .",
    "tree -o victim.txt .",
    "tree -R -H . .",
    "tree --output=victim.txt",
    "uniq in.txt out.txt",
    "git log --output=victim.txt",
    "git log --outp=victim.txt",
    "git diff --ext-diff",
    "git diff --ext",
    "git show --textconv HEAD:x",
    "git grep -O foo",
    "git grep -nO foo",
    "git grep --open-files-in-pager=./x foo",
    "git cat-file --filters HEAD:x",
    "file -C -m magic",
    "date -s 2020-01-01",
    "rg --pre ./x foo",
    "rg --pre=./x foo",
    "rg --hostname-bin=./x foo",
    "find . -name x -exec sh {} ;",
    "find . -okdir rm {} ;",
    "find . -fprint out.txt",
    "sed -n 'w out' x",
    "sed -n -e '1p' -f script.sed x",
    "less -o log.txt README.md",
    "less +!sh README.md",
    // M2: environment printers
    "ps eww",
    "ps auxe",
    "ps -E",
    "ps -o command,env",
    "cat /proc/self/environ",
    "strings /proc/1/environ",
    "set",
    "declare -p",
    "export -p",
    "printenv",
    "git config --list --show-origin",
    "launchctl getenv PATH",
    "defaults read",
    "jq -n env",
    "jq -n '$ENV'",
    // cargo can write Cargo.lock, hit the network, and run rustc wrappers
    "cargo tree",
    "cargo metadata",
    "cargo locate-project",
    "cargo fmt --check",
    // git: aliases, tools, globals that redirect config/pager/repo
    "git st",
    "git difftool",
    "git mergetool",
    "git bisect run ./x",
    "git archive -o x.tar HEAD",
    "git -c core.pager=./x log",
    "git -C /tmp log",
    "git --git-dir=/tmp/x log",
    "git -p log",
    "git --exec-path=./x log",
    "git branch evil",
    "git branch -D main",
    "git branch -fm x y",
    "git reflog expire --all",
    "git remote show origin",
    // redirections, subshells, programs by path, heredocs
    "cat README.md > out.txt",
    "cat README.md >> out.txt",
    "cat README.md 2> err.txt",
    "cat README.md &> both.txt",
    "cat README.md >& both.txt",
    "cat <> rw.txt",
    "cat <<EOF\nx\nEOF",
    "(ls)",
    "{ ls; }",
    "./tools/ls",
    "/tmp/cat x",
    "cd .. && cat secret",
    "cd /etc && cat passwd",
    "cd",
    "cat .{env,x}",
    "ls; sh x.sh",
    "ls && python3 x.py",
    "ls | sh",
    "ls & ./x",
    "xdotool search foo windowkill",
    "xdotool getactivewindow exec ./x",
    "wmctrl -l -c foo",
];

/// Plain reads that must stay reads.
const LEGIT_READS: &[&str] = &[
    "ls -la src",
    "wc -l a b",
    "git log --oneline -5",
    "rg -n foo src",
    "cat README.md",
    "head -20 x",
    "sed -n '1,20p' x",
    "git show HEAD:README.md",
    "git status",
    "git diff --stat",
    "git diff --no-ext-diff --text",
    "git show HEAD@{1}",
    "grep -rn 'fn main' src 2>/dev/null",
    "cat a | grep b | head -5",
    "LC_ALL=C grep -c x file",
    "cat README.md 2>&1 | head",
    "cat README.md >/dev/null",
    "find src -name '*.rs'",
    "tree -a -L 2",
    "uniq -c sorted.txt",
    "jq '.a | length' data.json",
    "cd sub && ls",
    "ps aux",
    "ls src/*.rs",
    "echo done",
    "echo 'a$b'",
    "grep -E 'x$' f",
];

#[test]
fn review_probe_commands_are_not_reads() {
    for cmd in PROBE_NOT_READS {
        assert!(!is_read_command(cmd), "`{cmd}` must not be a plain read");
        assert_ne!(
            classify_shell_risk(cmd),
            RiskTag::Reads,
            "`{cmd}` must not be tagged [reads]"
        );
    }
}

#[test]
fn plain_reads_stay_reads() {
    for cmd in LEGIT_READS {
        assert!(is_read_command(cmd), "`{cmd}` should parse as a plain read");
        assert_eq!(classify_shell_risk(cmd), RiskTag::Reads, "`{cmd}`");
    }
}

#[test]
fn parse_reports_why_a_command_is_not_a_read() {
    assert!(parse("echo $(id)").unwrap_err().contains("substitution"));
    assert!(parse("cat <(id)")
        .unwrap_err()
        .contains("process substitution"));
    assert!(parse("GIT_PAGER=x git log")
        .unwrap_err()
        .contains("GIT_PAGER"));
    assert!(parse("cat x > y").unwrap_err().contains("writes"));
    assert!(parse("TERM=../../x ls").is_err(), "values must be plain");
    // Name lookups are reads for the parser (the label stays conservative).
    assert!(is_read_command("command -v rg"));
    assert!(!is_read_command("command ./x"));
    assert!(is_read_command("time ls"));
}

#[test]
fn parse_splits_segments_and_keeps_inputs() {
    let segs = parse("LC_ALL=C sort < in.txt | uniq -c; ls 2>&1").unwrap();
    assert_eq!(segs.len(), 3);
    assert_eq!(segs[0].assignments, vec![("LC_ALL".into(), "C".into())]);
    assert_eq!(segs[0].inputs, vec!["in.txt".to_string()]);
    assert_eq!(segs[1].words[0].text, "uniq");
    assert_eq!(segs[2].words.len(), 1, "2>&1 is not an argument");
}

#[test]
fn quoted_operators_are_arguments() {
    let segs = parse("grep 'a | b; c > d' file").unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].words[1].text, "a | b; c > d");
    assert!(is_read_command("grep 'a | b; c > d' file"));
}

#[test]
fn path_operands_include_rev_path_suffix() {
    let segs = parse("git show HEAD:.env").unwrap();
    let ops = segs[0].path_operands();
    assert!(ops.contains(&".env".to_string()), "{ops:?}");
    let segs = parse("jq --rawfile x .env . f.json").unwrap();
    assert!(segs[0].path_operands().contains(&".env".to_string()));
}

#[test]
fn git_branch_listing_is_a_plain_read_but_keeps_its_label() {
    // The confirmation label for `git branch` stays `[git history]` (it
    // cannot tell listing from creating by the word alone); the read parser
    // accepts only the listing forms.
    for cmd in [
        "git branch -a",
        "git branch --list 'feat/*'",
        "git branch -vv",
    ] {
        assert!(is_read_command(cmd), "{cmd}");
    }
    for cmd in ["git branch new", "git branch -d old", "git branch -fu x"] {
        assert!(!is_read_command(cmd), "{cmd}");
    }
}

#[test]
fn ps_environment_forms_by_platform() {
    assert!(!is_read_command("ps -E"));
    assert!(!is_read_command("ps eww"));
    assert!(is_read_command("ps aux"));
    // FreeBSD/macOS `-e` shows the environment; Linux `-e` is every process.
    assert_eq!(is_read_command("ps -ef"), cfg!(target_os = "linux"));
}

#[test]
fn reader_list_is_shared() {
    for r in ["cat", "jq", "cut", "comm", "column", "sed", "awk", "head"] {
        assert!(consumes_file_contents(r), "{r}");
    }
    assert!(consumes_file_contents("git"));
    assert!(!consumes_file_contents("ls"));
}
