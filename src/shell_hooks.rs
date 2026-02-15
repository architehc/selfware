//! Shell Integration Hooks
//!
//! Terminal augmentation with:
//! - Pre-exec warnings for destructive commands
//! - Post-exec auto-fix suggestions
//! - Shell script generation for zsh/bash/fish
//! - Dangerous command detection

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Shell types supported
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    Sh,
}

impl ShellType {
    /// Detect shell from environment
    pub fn detect() -> Self {
        if let Ok(shell) = std::env::var("SHELL") {
            Self::from_path(&shell)
        } else {
            ShellType::Bash
        }
    }

    /// Parse from shell path
    pub fn from_path(path: &str) -> Self {
        if path.contains("zsh") {
            ShellType::Zsh
        } else if path.contains("fish") {
            ShellType::Fish
        } else if path.contains("bash") {
            ShellType::Bash
        } else {
            ShellType::Sh
        }
    }

    /// Get shell name
    pub fn name(&self) -> &'static str {
        match self {
            ShellType::Bash => "bash",
            ShellType::Zsh => "zsh",
            ShellType::Fish => "fish",
            ShellType::Sh => "sh",
        }
    }

    /// Get shell config file
    pub fn config_file(&self) -> &'static str {
        match self {
            ShellType::Bash => ".bashrc",
            ShellType::Zsh => ".zshrc",
            ShellType::Fish => "config.fish",
            ShellType::Sh => ".profile",
        }
    }

    /// Get config file path
    pub fn config_path(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|home| {
            if *self == ShellType::Fish {
                home.join(".config").join("fish").join("config.fish")
            } else {
                home.join(self.config_file())
            }
        })
    }
}

impl std::fmt::Display for ShellType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Risk level of a command
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    /// Get color for display
    pub fn color(&self) -> &'static str {
        match self {
            RiskLevel::Safe => "\x1b[32m",     // Green
            RiskLevel::Low => "\x1b[33m",      // Yellow
            RiskLevel::Medium => "\x1b[33m",   // Yellow
            RiskLevel::High => "\x1b[31m",     // Red
            RiskLevel::Critical => "\x1b[91m", // Bright red
        }
    }

    /// Get emoji indicator
    pub fn emoji(&self) -> &'static str {
        match self {
            RiskLevel::Safe => "✓",
            RiskLevel::Low => "⚡",
            RiskLevel::Medium => "⚠️",
            RiskLevel::High => "🔥",
            RiskLevel::Critical => "💀",
        }
    }

    /// Is this a dangerous level?
    pub fn is_dangerous(&self) -> bool {
        matches!(self, RiskLevel::High | RiskLevel::Critical)
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            RiskLevel::Safe => "Safe",
            RiskLevel::Low => "Low",
            RiskLevel::Medium => "Medium",
            RiskLevel::High => "High",
            RiskLevel::Critical => "Critical",
        };
        write!(f, "{}", name)
    }
}

/// A dangerous command pattern
#[derive(Debug, Clone)]
pub struct DangerousPattern {
    /// Pattern name
    pub name: String,
    /// Description of what it does
    pub description: String,
    /// Risk level
    pub risk: RiskLevel,
    /// Regex to match the command
    pub regex: Regex,
    /// Suggested safe alternative
    pub alternative: Option<String>,
}

impl DangerousPattern {
    /// Create a new pattern
    pub fn new(name: &str, description: &str, risk: RiskLevel, pattern: &str) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
            description: description.to_string(),
            risk,
            regex: Regex::new(pattern)?,
            alternative: None,
        })
    }

    /// Add safe alternative
    pub fn with_alternative(mut self, alt: &str) -> Self {
        self.alternative = Some(alt.to_string());
        self
    }

    /// Check if command matches
    pub fn matches(&self, command: &str) -> bool {
        self.regex.is_match(command)
    }
}

/// Command analyzer for detecting dangerous commands
#[derive(Debug, Default)]
pub struct CommandAnalyzer {
    patterns: Vec<DangerousPattern>,
}

impl CommandAnalyzer {
    /// Create new analyzer with default patterns
    pub fn new() -> Self {
        let mut analyzer = Self::default();
        analyzer.add_default_patterns();
        analyzer
    }

    /// Add default dangerous patterns
    fn add_default_patterns(&mut self) {
        // Critical - system destruction
        if let Ok(p) = DangerousPattern::new(
            "rm_rf_root",
            "Recursively removes root filesystem",
            RiskLevel::Critical,
            r"rm\s+(-[rfRFv\s]+)*\s*/\s*$",
        ) {
            self.patterns
                .push(p.with_alternative("Be very careful! This deletes everything"));
        }

        if let Ok(p) = DangerousPattern::new(
            "rm_rf_home",
            "Recursively removes home directory",
            RiskLevel::Critical,
            r"rm\s+(-[rfRFv\s]+)*\s*~/?\s*$",
        ) {
            self.patterns
                .push(p.with_alternative("This will delete your entire home directory!"));
        }

        if let Ok(p) = DangerousPattern::new(
            "dd_dev",
            "Writes directly to disk device",
            RiskLevel::Critical,
            r"dd\s+.*of=/dev/(sd[a-z]|nvme\d+n\d+|hd[a-z])\s*",
        ) {
            self.patterns
                .push(p.with_alternative("Double-check the target device!"));
        }

        if let Ok(p) = DangerousPattern::new(
            "mkfs",
            "Formats a disk partition",
            RiskLevel::Critical,
            r"mkfs\.?\w*\s+/dev/",
        ) {
            self.patterns
                .push(p.with_alternative("This will destroy all data on the partition!"));
        }

        // High risk - data loss
        if let Ok(p) = DangerousPattern::new(
            "rm_rf",
            "Recursive force remove",
            RiskLevel::High,
            r"rm\s+(-[rfRF]+\s+)+",
        ) {
            self.patterns
                .push(p.with_alternative("Consider using 'trash' command instead"));
        }

        if let Ok(p) = DangerousPattern::new(
            "git_reset_hard",
            "Discards all uncommitted changes",
            RiskLevel::High,
            r"git\s+reset\s+--hard",
        ) {
            self.patterns
                .push(p.with_alternative("Use 'git stash' to save changes first"));
        }

        if let Ok(p) = DangerousPattern::new(
            "git_force_push",
            "Force pushes to remote, potentially overwriting history",
            RiskLevel::High,
            r"git\s+push\s+(-f|--force)",
        ) {
            self.patterns
                .push(p.with_alternative("Use 'git push --force-with-lease' for safer force push"));
        }

        if let Ok(p) = DangerousPattern::new(
            "git_clean_force",
            "Removes untracked files permanently",
            RiskLevel::High,
            r"git\s+clean\s+(-[fdxX]+\s*)+",
        ) {
            self.patterns
                .push(p.with_alternative("Run 'git clean -n' first to preview"));
        }

        if let Ok(p) = DangerousPattern::new(
            "chmod_777",
            "Sets dangerous file permissions",
            RiskLevel::High,
            r"chmod\s+(-R\s+)?777",
        ) {
            self.patterns
                .push(p.with_alternative("Use more restrictive permissions like 755 or 644"));
        }

        // Medium risk - potentially dangerous
        if let Ok(p) = DangerousPattern::new(
            "sudo_su",
            "Switches to root user",
            RiskLevel::Medium,
            r"sudo\s+(su|bash|sh|zsh)\s*$",
        ) {
            self.patterns
                .push(p.with_alternative("Consider using 'sudo -s' for a root shell"));
        }

        if let Ok(p) = DangerousPattern::new(
            "curl_bash",
            "Pipes remote script directly to shell",
            RiskLevel::Medium,
            r"curl\s+.*\|\s*(sudo\s+)?(ba)?sh",
        ) {
            self.patterns
                .push(p.with_alternative("Download and review the script first"));
        }

        if let Ok(p) = DangerousPattern::new(
            "wget_bash",
            "Pipes remote script directly to shell",
            RiskLevel::Medium,
            r"wget\s+.*-O\s*-.*\|\s*(sudo\s+)?(ba)?sh",
        ) {
            self.patterns
                .push(p.with_alternative("Download and review the script first"));
        }

        if let Ok(p) = DangerousPattern::new(
            "docker_privileged",
            "Runs container with full host access",
            RiskLevel::Medium,
            r"docker\s+run\s+.*--privileged",
        ) {
            self.patterns
                .push(p.with_alternative("Only use --privileged when absolutely necessary"));
        }

        if let Ok(p) = DangerousPattern::new(
            "kill_9",
            "Force kills process without cleanup",
            RiskLevel::Medium,
            r"kill\s+-9",
        ) {
            self.patterns
                .push(p.with_alternative("Try 'kill -15' (SIGTERM) first for graceful shutdown"));
        }

        // Low risk - caution advised
        if let Ok(p) = DangerousPattern::new(
            "mv_overwrite",
            "May overwrite existing files",
            RiskLevel::Low,
            r"mv\s+[^|]+\s+[^|]+$",
        ) {
            self.patterns
                .push(p.with_alternative("Use 'mv -i' for interactive confirmation"));
        }

        if let Ok(p) = DangerousPattern::new(
            "cp_overwrite",
            "May overwrite existing files",
            RiskLevel::Low,
            r"cp\s+[^|]+\s+[^|]+$",
        ) {
            self.patterns
                .push(p.with_alternative("Use 'cp -i' for interactive confirmation"));
        }
    }

    /// Analyze a command and return any matches
    pub fn analyze(&self, command: &str) -> Vec<&DangerousPattern> {
        self.patterns
            .iter()
            .filter(|p| p.matches(command))
            .collect()
    }

    /// Get the highest risk level for a command
    pub fn risk_level(&self, command: &str) -> RiskLevel {
        self.analyze(command)
            .iter()
            .map(|p| p.risk)
            .max()
            .unwrap_or(RiskLevel::Safe)
    }

    /// Check if command is dangerous
    pub fn is_dangerous(&self, command: &str) -> bool {
        self.risk_level(command).is_dangerous()
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, pattern: DangerousPattern) {
        self.patterns.push(pattern);
    }

    /// Get warning message for a command
    pub fn warning(&self, command: &str) -> Option<String> {
        let matches = self.analyze(command);
        if matches.is_empty() {
            return None;
        }

        let mut warning = String::new();
        for m in matches {
            warning.push_str(&format!(
                "{} {} - {}\n",
                m.risk.emoji(),
                m.name,
                m.description
            ));
            if let Some(alt) = &m.alternative {
                warning.push_str(&format!("  Suggestion: {}\n", alt));
            }
        }
        Some(warning)
    }
}

/// Exit code analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitCodeInfo {
    /// Exit code
    pub code: i32,
    /// Meaning of the code
    pub meaning: String,
    /// Possible fix suggestions
    pub suggestions: Vec<String>,
}

impl ExitCodeInfo {
    /// Analyze an exit code
    pub fn analyze(code: i32, command: &str) -> Self {
        let (meaning, suggestions) = match code {
            0 => ("Success".to_string(), vec![]),
            1 => (
                "General error".to_string(),
                vec![
                    "Check command syntax".to_string(),
                    "Verify file permissions".to_string(),
                ],
            ),
            2 => (
                "Misuse of shell command".to_string(),
                vec![
                    "Check command arguments".to_string(),
                    "Run 'man <command>' for help".to_string(),
                ],
            ),
            126 => (
                "Command found but not executable".to_string(),
                vec![
                    "Check file permissions with 'ls -l'".to_string(),
                    "Try 'chmod +x <file>'".to_string(),
                ],
            ),
            127 => (
                "Command not found".to_string(),
                vec![
                    "Check if command is installed".to_string(),
                    "Verify PATH includes the command's directory".to_string(),
                    Self::suggest_installation(command),
                ],
            ),
            128 => ("Invalid exit code".to_string(), vec![]),
            130 => ("Terminated by Ctrl+C (SIGINT)".to_string(), vec![]),
            137 => (
                "Killed (SIGKILL, often out of memory)".to_string(),
                vec![
                    "Check system memory with 'free -h'".to_string(),
                    "Reduce resource usage or increase memory".to_string(),
                ],
            ),
            139 => (
                "Segmentation fault (SIGSEGV)".to_string(),
                vec![
                    "Check for bugs in the program".to_string(),
                    "Try with different input".to_string(),
                ],
            ),
            143 => ("Terminated (SIGTERM)".to_string(), vec![]),
            255 => ("Exit status out of range".to_string(), vec![]),
            _ if code > 128 => {
                let signal = code - 128;
                (format!("Killed by signal {}", signal), vec![])
            }
            _ => (format!("Exit code {}", code), vec![]),
        };

        Self {
            code,
            meaning,
            suggestions,
        }
    }

    /// Suggest installation for common commands
    fn suggest_installation(command: &str) -> String {
        let cmd = command.split_whitespace().next().unwrap_or("");
        let install = match cmd {
            "jq" => "apt install jq / brew install jq",
            "rg" | "ripgrep" => "apt install ripgrep / cargo install ripgrep",
            "fd" => "apt install fd-find / cargo install fd-find",
            "bat" => "apt install bat / cargo install bat",
            "exa" | "eza" => "cargo install eza",
            "htop" => "apt install htop / brew install htop",
            "tree" => "apt install tree / brew install tree",
            "wget" => "apt install wget / brew install wget",
            "curl" => "apt install curl / brew install curl",
            "git" => "apt install git / brew install git",
            "node" | "npm" => "Install Node.js from nodejs.org",
            "python3" | "pip3" => "apt install python3 python3-pip",
            "rustc" | "cargo" => "Install Rust from rustup.rs",
            "go" => "Install Go from go.dev",
            "docker" => "Install Docker from docker.com",
            _ => "Check package manager for installation",
        };
        format!("Install with: {}", install)
    }

    /// Is this a successful exit?
    pub fn is_success(&self) -> bool {
        self.code == 0
    }

    /// Display the info
    pub fn display(&self) -> String {
        let mut s = format!("Exit {}: {}", self.code, self.meaning);
        for suggestion in &self.suggestions {
            s.push_str(&format!("\n  → {}", suggestion));
        }
        s
    }
}

/// Shell hook event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    PreExec(String),       // Before command execution
    PostExec(String, i32), // After command, with exit code
    DirectoryChange(PathBuf),
    CommandNotFound(String),
}

/// Shell script generator
#[derive(Debug)]
pub struct ShellScriptGenerator {
    shell: ShellType,
    agent_path: PathBuf,
}

impl ShellScriptGenerator {
    /// Create generator for a shell
    pub fn new(shell: ShellType, agent_path: PathBuf) -> Self {
        Self { shell, agent_path }
    }

    /// Generate the hook script
    pub fn generate(&self) -> String {
        match self.shell {
            ShellType::Bash => self.generate_bash(),
            ShellType::Zsh => self.generate_zsh(),
            ShellType::Fish => self.generate_fish(),
            ShellType::Sh => self.generate_sh(),
        }
    }

    fn generate_bash(&self) -> String {
        let agent = self.agent_path.display();
        format!(
            r#"# Selfware shell hooks for Bash
# Add this to your ~/.bashrc

_selfware_preexec() {{
    local cmd="$1"
    local warning
    warning=$("{agent}" hook preexec "$cmd" 2>/dev/null)
    if [[ -n "$warning" ]]; then
        echo -e "$warning"
        read -p "Continue? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            return 1
        fi
    fi
}}

_selfware_postexec() {{
    local exit_code=$?
    local cmd="$1"
    if [[ $exit_code -ne 0 ]]; then
        "{agent}" hook postexec "$cmd" $exit_code 2>/dev/null
    fi
}}

# Enable command trapping
trap '_selfware_preexec "$BASH_COMMAND"' DEBUG
PROMPT_COMMAND="_selfware_postexec '$BASH_COMMAND'; $PROMPT_COMMAND"
"#
        )
    }

    fn generate_zsh(&self) -> String {
        let agent = self.agent_path.display();
        format!(
            r#"# Selfware shell hooks for Zsh
# Add this to your ~/.zshrc

autoload -Uz add-zsh-hook

_selfware_preexec() {{
    local cmd="$1"
    local warning
    warning=$("{agent}" hook preexec "$cmd" 2>/dev/null)
    if [[ -n "$warning" ]]; then
        echo -e "$warning"
        read -q "REPLY?Continue? [y/N] "
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            return 1
        fi
    fi
}}

_selfware_precmd() {{
    local exit_code=$?
    if [[ $exit_code -ne 0 ]]; then
        "{agent}" hook postexec "$_selfware_last_cmd" $exit_code 2>/dev/null
    fi
}}

_selfware_preexec_save() {{
    _selfware_last_cmd="$1"
}}

add-zsh-hook preexec _selfware_preexec
add-zsh-hook preexec _selfware_preexec_save
add-zsh-hook precmd _selfware_precmd
"#
        )
    }

    fn generate_fish(&self) -> String {
        let agent = self.agent_path.display();
        format!(
            r#"# Selfware shell hooks for Fish
# Add this to your ~/.config/fish/config.fish

function _selfware_preexec --on-event fish_preexec
    set -l cmd $argv[1]
    set -l warning ("{agent}" hook preexec "$cmd" 2>/dev/null)
    if test -n "$warning"
        echo -e $warning
        read -P "Continue? [y/N] " -n 1 reply
        if test "$reply" != "y" -a "$reply" != "Y"
            return 1
        end
    end
end

function _selfware_postexec --on-event fish_postexec
    set -l exit_code $status
    if test $exit_code -ne 0
        "{agent}" hook postexec "$argv[1]" $exit_code 2>/dev/null
    end
end
"#
        )
    }

    fn generate_sh(&self) -> String {
        r#"# Selfware shell hooks for POSIX sh
# Note: Limited functionality in pure sh

# Basic aliases for safer commands
alias rm='rm -i'
alias cp='cp -i'
alias mv='mv -i'
"#
        .to_string()
    }

    /// Get installation instructions
    pub fn instructions(&self) -> String {
        let config = self.shell.config_file();
        let script = self.generate();

        format!(
            r#"
== Selfware Shell Integration ==

To enable shell hooks for {shell}:

1. Add the following to your ~/{config}:

{script}

2. Reload your shell:
   source ~/{config}

The hooks will:
- Warn before dangerous commands
- Suggest fixes after command failures
- Provide helpful context
"#,
            shell = self.shell,
            config = config,
            script = script
        )
    }
}

/// Auto-fix suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoFixSuggestion {
    /// Original command that failed
    pub original: String,
    /// Suggested fix
    pub suggestion: String,
    /// Explanation
    pub explanation: String,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f32,
}

impl AutoFixSuggestion {
    /// Create a new suggestion
    pub fn new(original: String, suggestion: String, explanation: String, confidence: f32) -> Self {
        Self {
            original,
            suggestion,
            explanation,
            confidence,
        }
    }
}

/// Auto-fix suggester
#[derive(Debug, Default)]
pub struct AutoFixSuggester {
    /// Correction rules
    rules: Vec<CorrectionRule>,
}

/// A correction rule
#[derive(Debug, Clone)]
pub struct CorrectionRule {
    /// Pattern to match
    pub pattern: Regex,
    /// Replacement
    pub replacement: String,
    /// Explanation
    pub explanation: String,
}

impl CorrectionRule {
    /// Create a new rule
    pub fn new(pattern: &str, replacement: &str, explanation: &str) -> Result<Self> {
        Ok(Self {
            pattern: Regex::new(pattern)?,
            replacement: replacement.to_string(),
            explanation: explanation.to_string(),
        })
    }
}

impl AutoFixSuggester {
    /// Create suggester with default rules
    pub fn new() -> Self {
        let mut suggester = Self::default();
        suggester.add_default_rules();
        suggester
    }

    /// Add default correction rules
    fn add_default_rules(&mut self) {
        // Common typos
        if let Ok(r) = CorrectionRule::new(r"^gti\s+", "git ", "Typo: 'gti' -> 'git'") {
            self.rules.push(r);
        }
        if let Ok(r) = CorrectionRule::new(r"^sl\s*$", "ls", "Typo: 'sl' -> 'ls'") {
            self.rules.push(r);
        }
        if let Ok(r) = CorrectionRule::new(r"^gerp\s+", "grep ", "Typo: 'gerp' -> 'grep'") {
            self.rules.push(r);
        }
        if let Ok(r) = CorrectionRule::new(r"^cta\s+", "cat ", "Typo: 'cta' -> 'cat'") {
            self.rules.push(r);
        }
        if let Ok(r) = CorrectionRule::new(r"^dc\s+", "cd ", "Typo: 'dc' -> 'cd'") {
            self.rules.push(r);
        }
        if let Ok(r) = CorrectionRule::new(r"^pyhton", "python", "Typo: 'pyhton' -> 'python'") {
            self.rules.push(r);
        }
        if let Ok(r) = CorrectionRule::new(r"^pytohn", "python", "Typo: 'pytohn' -> 'python'") {
            self.rules.push(r);
        }

        // Common git mistakes
        if let Ok(r) = CorrectionRule::new(r"^git add -a\s*$", "git add -A", "Use -A for all files")
        {
            self.rules.push(r);
        }
        if let Ok(r) = CorrectionRule::new(
            r"^git stash pop --index",
            "git stash pop",
            "Remove --index for simpler pop",
        ) {
            self.rules.push(r);
        }

        // Cargo
        if let Ok(r) = CorrectionRule::new(
            r"^cargo run --relase",
            "cargo run --release",
            "Typo: 'relase' -> 'release'",
        ) {
            self.rules.push(r);
        }

        // Docker
        if let Ok(r) = CorrectionRule::new(
            r"^docker compose",
            "docker-compose",
            "Use 'docker-compose' command",
        ) {
            self.rules.push(r);
        }
    }

    /// Suggest fixes for a failed command
    pub fn suggest(&self, command: &str, _exit_code: i32) -> Vec<AutoFixSuggestion> {
        let mut suggestions = Vec::new();

        // Check correction rules
        for rule in &self.rules {
            if rule.pattern.is_match(command) {
                let fixed = rule.pattern.replace(command, &rule.replacement);
                suggestions.push(AutoFixSuggestion::new(
                    command.to_string(),
                    fixed.to_string(),
                    rule.explanation.clone(),
                    0.9,
                ));
            }
        }

        // Check for common patterns
        if command.starts_with("cd ") && !Path::new(&command[3..].trim()).exists() {
            // Suggest similar directories
            if let Some(fix) = self.find_similar_directory(command[3..].trim()) {
                suggestions.push(AutoFixSuggestion::new(
                    command.to_string(),
                    format!("cd {}", fix),
                    "Did you mean this directory?".to_string(),
                    0.7,
                ));
            }
        }

        suggestions
    }

    /// Find similar directory name
    fn find_similar_directory(&self, path: &str) -> Option<String> {
        let parent = Path::new(path).parent().unwrap_or(Path::new("."));
        let target = Path::new(path).file_name()?.to_str()?;

        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.to_lowercase() == target.to_lowercase() {
                        return Some(entry.path().display().to_string());
                    }
                    // Simple edit distance check for typos
                    if Self::edit_distance(&name_str, target) <= 2 {
                        return Some(entry.path().display().to_string());
                    }
                }
            }
        }
        None
    }

    /// Calculate edit distance between two strings
    #[allow(clippy::needless_range_loop)]
    fn edit_distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let m = a.len();
        let n = b.len();

        if m == 0 {
            return n;
        }
        if n == 0 {
            return m;
        }

        let mut dp = vec![vec![0; n + 1]; m + 1];

        for i in 0..=m {
            dp[i][0] = i;
        }
        for j in 0..=n {
            dp[0][j] = j;
        }

        for i in 1..=m {
            for j in 1..=n {
                let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
                dp[i][j] = (dp[i - 1][j] + 1)
                    .min(dp[i][j - 1] + 1)
                    .min(dp[i - 1][j - 1] + cost);
            }
        }

        dp[m][n]
    }

    /// Add a custom correction rule
    pub fn add_rule(&mut self, rule: CorrectionRule) {
        self.rules.push(rule);
    }
}

/// Shell context for displaying information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShellContext {
    /// Current working directory
    pub cwd: Option<PathBuf>,
    /// Git branch if in a repo
    pub git_branch: Option<String>,
    /// Git status (dirty/clean)
    pub git_dirty: bool,
    /// Last command exit code
    pub last_exit: Option<i32>,
    /// Active virtual environment
    pub venv: Option<String>,
    /// Docker context
    pub docker_context: Option<String>,
}

impl ShellContext {
    /// Gather current shell context
    #[allow(clippy::field_reassign_with_default)]
    pub fn gather() -> Self {
        let mut ctx = Self::default();

        // Get CWD
        ctx.cwd = std::env::current_dir().ok();

        // Get git info
        if let Ok(output) = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .output()
        {
            if output.status.success() {
                ctx.git_branch = Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
        }

        // Check if git is dirty
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
        {
            ctx.git_dirty = !output.stdout.is_empty();
        }

        // Check virtual environment
        ctx.venv = std::env::var("VIRTUAL_ENV").ok().and_then(|v| {
            Path::new(&v)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        });

        // Check Docker context
        if let Ok(output) = std::process::Command::new("docker")
            .args(["context", "show"])
            .output()
        {
            if output.status.success() {
                let context = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if context != "default" {
                    ctx.docker_context = Some(context);
                }
            }
        }

        ctx
    }

    /// Format for right prompt
    pub fn right_prompt(&self) -> String {
        let mut parts = Vec::new();

        if let Some(branch) = &self.git_branch {
            let dirty = if self.git_dirty { "*" } else { "" };
            parts.push(format!("\x1b[33m{}{}\x1b[0m", branch, dirty));
        }

        if let Some(venv) = &self.venv {
            parts.push(format!("\x1b[36m({})\x1b[0m", venv));
        }

        if let Some(ctx) = &self.docker_context {
            parts.push(format!("\x1b[35m🐳{}\x1b[0m", ctx));
        }

        parts.join(" ")
    }
}

/// Command history analysis
#[derive(Debug, Default)]
pub struct HistoryAnalyzer {
    /// Frequently used commands
    pub frequent: HashMap<String, usize>,
    /// Command sequences (what follows what)
    pub sequences: HashMap<String, Vec<String>>,
    /// Total commands analyzed
    pub total: usize,
}

impl HistoryAnalyzer {
    /// Create new analyzer
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyze a command
    pub fn record(&mut self, command: &str) {
        let base = command.split_whitespace().next().unwrap_or("");
        *self.frequent.entry(base.to_string()).or_insert(0) += 1;
        self.total += 1;
    }

    /// Record a sequence
    pub fn record_sequence(&mut self, prev: &str, current: &str) {
        let prev_base = prev.split_whitespace().next().unwrap_or("");
        let curr_base = current.split_whitespace().next().unwrap_or("");
        self.sequences
            .entry(prev_base.to_string())
            .or_default()
            .push(curr_base.to_string());
    }

    /// Get top commands
    pub fn top_commands(&self, n: usize) -> Vec<(&String, &usize)> {
        let mut sorted: Vec<_> = self.frequent.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        sorted.into_iter().take(n).collect()
    }

    /// Predict next command based on current
    pub fn predict_next(&self, current: &str) -> Option<String> {
        let base = current.split_whitespace().next()?;
        let sequences = self.sequences.get(base)?;

        // Count occurrences
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for s in sequences {
            *counts.entry(s.as_str()).or_insert(0) += 1;
        }

        // Return most common
        counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(s, _)| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_type_detect() {
        // Just verify it doesn't crash
        let _ = ShellType::detect();
    }

    #[test]
    fn test_shell_type_from_path() {
        assert_eq!(ShellType::from_path("/bin/zsh"), ShellType::Zsh);
        assert_eq!(ShellType::from_path("/bin/bash"), ShellType::Bash);
        assert_eq!(ShellType::from_path("/usr/bin/fish"), ShellType::Fish);
        assert_eq!(ShellType::from_path("/bin/sh"), ShellType::Sh);
    }

    #[test]
    fn test_shell_type_name() {
        assert_eq!(ShellType::Bash.name(), "bash");
        assert_eq!(ShellType::Zsh.name(), "zsh");
        assert_eq!(ShellType::Fish.name(), "fish");
        assert_eq!(ShellType::Sh.name(), "sh");
    }

    #[test]
    fn test_shell_type_config_file() {
        assert_eq!(ShellType::Bash.config_file(), ".bashrc");
        assert_eq!(ShellType::Zsh.config_file(), ".zshrc");
        assert_eq!(ShellType::Fish.config_file(), "config.fish");
        assert_eq!(ShellType::Sh.config_file(), ".profile");
    }

    #[test]
    fn test_shell_type_display() {
        assert_eq!(format!("{}", ShellType::Bash), "bash");
    }

    #[test]
    fn test_risk_level_color() {
        assert!(RiskLevel::Safe.color().contains("32"));
        assert!(RiskLevel::Critical.color().contains("91"));
    }

    #[test]
    fn test_risk_level_emoji() {
        assert_eq!(RiskLevel::Safe.emoji(), "✓");
        assert_eq!(RiskLevel::Critical.emoji(), "💀");
    }

    #[test]
    fn test_risk_level_is_dangerous() {
        assert!(!RiskLevel::Safe.is_dangerous());
        assert!(!RiskLevel::Low.is_dangerous());
        assert!(!RiskLevel::Medium.is_dangerous());
        assert!(RiskLevel::High.is_dangerous());
        assert!(RiskLevel::Critical.is_dangerous());
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(format!("{}", RiskLevel::High), "High");
        assert_eq!(format!("{}", RiskLevel::Critical), "Critical");
    }

    #[test]
    fn test_dangerous_pattern_creation() {
        let pattern =
            DangerousPattern::new("test", "Test pattern", RiskLevel::High, r"rm\s+-rf").unwrap();

        assert_eq!(pattern.name, "test");
        assert_eq!(pattern.risk, RiskLevel::High);
        assert!(pattern.matches("rm -rf /tmp"));
    }

    #[test]
    fn test_dangerous_pattern_alternative() {
        let pattern = DangerousPattern::new("test", "Test", RiskLevel::Medium, r"test")
            .unwrap()
            .with_alternative("use other");

        assert_eq!(pattern.alternative, Some("use other".to_string()));
    }

    #[test]
    fn test_command_analyzer_new() {
        let analyzer = CommandAnalyzer::new();
        assert!(!analyzer.patterns.is_empty());
    }

    #[test]
    fn test_command_analyzer_rm_rf() {
        let analyzer = CommandAnalyzer::new();
        assert!(analyzer.is_dangerous("rm -rf /"));
        assert!(analyzer.is_dangerous("rm -rf ~/"));
        assert!(analyzer.risk_level("rm -rf /") == RiskLevel::Critical);
    }

    #[test]
    fn test_command_analyzer_git_force_push() {
        let analyzer = CommandAnalyzer::new();
        let risk = analyzer.risk_level("git push -f origin main");
        assert_eq!(risk, RiskLevel::High);
    }

    #[test]
    fn test_command_analyzer_safe_command() {
        let analyzer = CommandAnalyzer::new();
        let risk = analyzer.risk_level("ls -la");
        assert_eq!(risk, RiskLevel::Safe);
    }

    #[test]
    fn test_command_analyzer_warning() {
        let analyzer = CommandAnalyzer::new();
        let warning = analyzer.warning("rm -rf /");
        assert!(warning.is_some());
        // Warning contains pattern name and description
        let w = warning.unwrap();
        assert!(w.contains("rm_rf") || w.contains("Recursively"));
    }

    #[test]
    fn test_command_analyzer_no_warning() {
        let analyzer = CommandAnalyzer::new();
        assert!(analyzer.warning("echo hello").is_none());
    }

    #[test]
    fn test_command_analyzer_add_pattern() {
        let mut analyzer = CommandAnalyzer::new();
        let initial = analyzer.patterns.len();
        analyzer.add_pattern(
            DangerousPattern::new("custom", "Custom", RiskLevel::Low, r"custom").unwrap(),
        );
        assert_eq!(analyzer.patterns.len(), initial + 1);
    }

    #[test]
    fn test_exit_code_info_success() {
        let info = ExitCodeInfo::analyze(0, "ls");
        assert!(info.is_success());
        assert!(info.meaning.contains("Success"));
    }

    #[test]
    fn test_exit_code_info_not_found() {
        let info = ExitCodeInfo::analyze(127, "nonexistent");
        assert!(!info.is_success());
        assert!(info.meaning.contains("not found"));
        assert!(!info.suggestions.is_empty());
    }

    #[test]
    fn test_exit_code_info_permission() {
        let info = ExitCodeInfo::analyze(126, "script.sh");
        assert!(info.meaning.contains("executable"));
    }

    #[test]
    fn test_exit_code_info_killed() {
        let info = ExitCodeInfo::analyze(137, "long_process");
        assert!(info.meaning.contains("SIGKILL"));
    }

    #[test]
    fn test_exit_code_info_display() {
        let info = ExitCodeInfo::analyze(1, "failed");
        let display = info.display();
        assert!(display.contains("Exit 1"));
    }

    #[test]
    fn test_exit_code_signal() {
        let info = ExitCodeInfo::analyze(143, "process");
        assert!(info.meaning.contains("SIGTERM"));
    }

    #[test]
    fn test_shell_script_generator_bash() {
        let gen = ShellScriptGenerator::new(ShellType::Bash, PathBuf::from("/usr/bin/selfware"));
        let script = gen.generate();
        assert!(script.contains("Bash"));
        assert!(script.contains("trap"));
        assert!(script.contains("_selfware_preexec"));
    }

    #[test]
    fn test_shell_script_generator_zsh() {
        let gen = ShellScriptGenerator::new(ShellType::Zsh, PathBuf::from("/usr/bin/selfware"));
        let script = gen.generate();
        assert!(script.contains("Zsh"));
        assert!(script.contains("add-zsh-hook"));
    }

    #[test]
    fn test_shell_script_generator_fish() {
        let gen = ShellScriptGenerator::new(ShellType::Fish, PathBuf::from("/usr/bin/selfware"));
        let script = gen.generate();
        assert!(script.contains("Fish"));
        assert!(script.contains("on-event"));
    }

    #[test]
    fn test_shell_script_generator_sh() {
        let gen = ShellScriptGenerator::new(ShellType::Sh, PathBuf::from("/usr/bin/selfware"));
        let script = gen.generate();
        assert!(script.contains("POSIX"));
    }

    #[test]
    fn test_shell_script_generator_instructions() {
        let gen = ShellScriptGenerator::new(ShellType::Bash, PathBuf::from("/usr/bin/selfware"));
        let instructions = gen.instructions();
        assert!(instructions.contains("~/.bashrc"));
        assert!(instructions.contains("Selfware Shell Integration"));
    }

    #[test]
    fn test_auto_fix_suggestion_creation() {
        let fix = AutoFixSuggestion::new(
            "gti status".to_string(),
            "git status".to_string(),
            "Typo".to_string(),
            0.9,
        );
        assert_eq!(fix.original, "gti status");
        assert_eq!(fix.suggestion, "git status");
        assert_eq!(fix.confidence, 0.9);
    }

    #[test]
    fn test_auto_fix_suggester_new() {
        let suggester = AutoFixSuggester::new();
        assert!(!suggester.rules.is_empty());
    }

    #[test]
    fn test_auto_fix_suggester_typo() {
        let suggester = AutoFixSuggester::new();
        let suggestions = suggester.suggest("gti status", 127);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].suggestion.contains("git"));
    }

    #[test]
    fn test_auto_fix_suggester_sl() {
        let suggester = AutoFixSuggester::new();
        let suggestions = suggester.suggest("sl", 127);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].suggestion.contains("ls"));
    }

    #[test]
    fn test_auto_fix_suggester_no_match() {
        let suggester = AutoFixSuggester::new();
        let suggestions = suggester.suggest("correct_command arg", 1);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_auto_fix_edit_distance() {
        assert_eq!(AutoFixSuggester::edit_distance("", "abc"), 3);
        assert_eq!(AutoFixSuggester::edit_distance("abc", ""), 3);
        assert_eq!(AutoFixSuggester::edit_distance("abc", "abc"), 0);
        assert_eq!(AutoFixSuggester::edit_distance("abc", "abd"), 1);
        assert_eq!(AutoFixSuggester::edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_correction_rule_creation() {
        let rule = CorrectionRule::new(r"^test", "replaced", "explanation").unwrap();
        assert!(rule.pattern.is_match("testing"));
        assert_eq!(rule.replacement, "replaced");
    }

    #[test]
    fn test_auto_fix_add_rule() {
        let mut suggester = AutoFixSuggester::new();
        let initial = suggester.rules.len();
        suggester.add_rule(CorrectionRule::new(r"custom", "fixed", "Custom fix").unwrap());
        assert_eq!(suggester.rules.len(), initial + 1);
    }

    #[test]
    fn test_shell_context_default() {
        let ctx = ShellContext::default();
        assert!(ctx.cwd.is_none());
        assert!(ctx.git_branch.is_none());
        assert!(!ctx.git_dirty);
    }

    #[test]
    fn test_shell_context_right_prompt() {
        let mut ctx = ShellContext::default();
        ctx.git_branch = Some("main".to_string());
        ctx.git_dirty = true;
        ctx.venv = Some("myenv".to_string());

        let prompt = ctx.right_prompt();
        assert!(prompt.contains("main*"));
        assert!(prompt.contains("myenv"));
    }

    #[test]
    fn test_shell_context_right_prompt_empty() {
        let ctx = ShellContext::default();
        assert!(ctx.right_prompt().is_empty());
    }

    #[test]
    fn test_history_analyzer_new() {
        let analyzer = HistoryAnalyzer::new();
        assert_eq!(analyzer.total, 0);
    }

    #[test]
    fn test_history_analyzer_record() {
        let mut analyzer = HistoryAnalyzer::new();
        analyzer.record("git status");
        analyzer.record("git commit");
        analyzer.record("git push");
        analyzer.record("ls");

        assert_eq!(analyzer.total, 4);
        assert_eq!(analyzer.frequent.get("git"), Some(&3));
        assert_eq!(analyzer.frequent.get("ls"), Some(&1));
    }

    #[test]
    fn test_history_analyzer_top_commands() {
        let mut analyzer = HistoryAnalyzer::new();
        for _ in 0..5 {
            analyzer.record("git status");
        }
        for _ in 0..3 {
            analyzer.record("ls");
        }
        for _ in 0..1 {
            analyzer.record("cd");
        }

        let top = analyzer.top_commands(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "git");
        assert_eq!(top[1].0, "ls");
    }

    #[test]
    fn test_history_analyzer_sequence() {
        let mut analyzer = HistoryAnalyzer::new();
        analyzer.record_sequence("git add", "git commit");
        analyzer.record_sequence("git add", "git commit");
        analyzer.record_sequence("git add", "git status");

        let next = analyzer.predict_next("git add");
        assert_eq!(next, Some("git".to_string()));
    }

    #[test]
    fn test_history_analyzer_predict_none() {
        let analyzer = HistoryAnalyzer::new();
        assert!(analyzer.predict_next("unknown").is_none());
    }

    #[test]
    fn test_hook_event_variants() {
        let _pre = HookEvent::PreExec("ls".to_string());
        let _post = HookEvent::PostExec("ls".to_string(), 0);
        let _cd = HookEvent::DirectoryChange(PathBuf::from("/tmp"));
        let _not_found = HookEvent::CommandNotFound("xyz".to_string());
    }

    #[test]
    fn test_shell_context_gather_basic() {
        // This just tests that gather doesn't panic
        let _ctx = ShellContext::gather();
    }

    #[test]
    fn test_command_analyzer_curl_bash() {
        let analyzer = CommandAnalyzer::new();
        let risk = analyzer.risk_level("curl https://example.com/install.sh | bash");
        assert!(risk >= RiskLevel::Medium);
    }

    #[test]
    fn test_command_analyzer_chmod_777() {
        let analyzer = CommandAnalyzer::new();
        let risk = analyzer.risk_level("chmod 777 /var/www");
        assert!(risk >= RiskLevel::High);
    }

    #[test]
    fn test_command_analyzer_git_clean() {
        let analyzer = CommandAnalyzer::new();
        let risk = analyzer.risk_level("git clean -fd");
        assert!(risk >= RiskLevel::High);
    }

    #[test]
    fn test_command_analyzer_dd() {
        let analyzer = CommandAnalyzer::new();
        let risk = analyzer.risk_level("dd if=/dev/zero of=/dev/sda bs=1M");
        assert_eq!(risk, RiskLevel::Critical);
    }

    #[test]
    fn test_command_analyzer_mkfs() {
        let analyzer = CommandAnalyzer::new();
        let risk = analyzer.risk_level("mkfs.ext4 /dev/sda1");
        assert_eq!(risk, RiskLevel::Critical);
    }

    #[test]
    fn test_exit_code_segfault() {
        let info = ExitCodeInfo::analyze(139, "buggy");
        assert!(info.meaning.contains("Segmentation"));
    }

    #[test]
    fn test_exit_code_unknown_signal() {
        let info = ExitCodeInfo::analyze(150, "process");
        assert!(info.meaning.contains("signal"));
    }

    #[test]
    fn test_auto_fix_gerp() {
        let suggester = AutoFixSuggester::new();
        let suggestions = suggester.suggest("gerp pattern file", 127);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].suggestion.starts_with("grep"));
    }

    #[test]
    fn test_auto_fix_dc() {
        let suggester = AutoFixSuggester::new();
        let suggestions = suggester.suggest("dc /tmp", 127);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].suggestion.starts_with("cd"));
    }

    #[test]
    fn test_shell_type_config_path() {
        let path = ShellType::Bash.config_path();
        assert!(path.is_some());
        assert!(path.unwrap().ends_with(".bashrc"));
    }

    #[test]
    fn test_shell_type_fish_config_path() {
        let path = ShellType::Fish.config_path();
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(p.ends_with("config.fish"));
        assert!(p.to_string_lossy().contains("fish"));
    }
}
