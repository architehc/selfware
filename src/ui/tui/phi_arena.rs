//! Phi Fox Arena & Debt Slayer TUI Pane (`Ctrl+F` / `PaneType::PhiArena`).
//!
//! Brings Phi the Fox (Φ) into the native Rust `ratatui` TUI with:
//! - Live posture & emotion transitions driven by `.selfware/phi/activity`
//! - 10-viseme terminal mouth animation (`rest`, `mbp`, `etc`, `ai`, `e`, `o`, `u`, `fv`, `l_th`, `wq`)
//! - Gamified Verification Debt Boss HP bar, Grounded XP, `φ` streak multiplier, and quests
//! - Interactive Grounded Boss Battle Drills (`[b]` + `[1]`/`[2]`/`[3]`) built from real workspace symbols
//! - Persistent `.selfware/phi/arena_profile.json` progression & achievements with symlink/hardlink guards
//!
//! Honest status contract (`AGENTS.md` Rule 3):
//! - Activity captures are always labeled `TRUST: OBSERVED_SIGNALS`.
//! - Missing or stale captures never claim that unrun tests passed.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_PROFILE_BYTES: u64 = 64 * 1024;
const MAX_SOURCE_SCAN_BYTES: u64 = 128 * 1024;
const BRAILLE_SPARKS: [&str; 11] = ["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈", "✦", "✧", "φ"];

/// 10-viseme mouth shapes matching the Web `PhiVisemeEngine` and `scripts/phi_tmux_game.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viseme {
    Rest,
    Mbp,
    Etc,
    Ai,
    E,
    O,
    U,
    Fv,
    LTh,
    Wq,
}

impl Viseme {
    pub fn code(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::Mbp => "mbp",
            Self::Etc => "etc",
            Self::Ai => "ai",
            Self::E => "e",
            Self::O => "o",
            Self::U => "u",
            Self::Fv => "fv",
            Self::LTh => "l_th",
            Self::Wq => "wq",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::Rest => "───",
            Self::Mbp => "═══",
            Self::Etc => "─▫─",
            Self::Ai => "◁◯▷",
            Self::E => "◁═▷",
            Self::O => "(◯)",
            Self::U => "(▫)",
            Self::Fv => "─▿─",
            Self::LTh => "─△─",
            Self::Wq => "∘▫∘",
        }
    }
}

/// Map a single word to its primary viseme mouth shape.
pub fn word_to_viseme(word: &str) -> Viseme {
    let cleaned: String = word
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if cleaned.is_empty() {
        return Viseme::Rest;
    }
    if cleaned.starts_with('m') || cleaned.starts_with('b') || cleaned.starts_with('p') {
        return Viseme::Mbp;
    }
    if cleaned.starts_with('f') || cleaned.starts_with('v') {
        return Viseme::Fv;
    }
    if cleaned.starts_with('l') || cleaned.starts_with("th") {
        return Viseme::LTh;
    }
    if cleaned.starts_with('w') || cleaned.starts_with("qu") || cleaned.starts_with('q') {
        return Viseme::Wq;
    }
    for ch in cleaned.chars() {
        match ch {
            'a' => return Viseme::Ai,
            'e' => return Viseme::E,
            'o' => return Viseme::O,
            'u' => return Viseme::U,
            _ => {}
        }
    }
    Viseme::Etc
}

/// Convert spoken text into a bounded sequence of viseme frames ending at `Viseme::Rest`.
pub fn text_to_viseme_frames(text: &str, max_frames: usize) -> Vec<Viseme> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![Viseme::Rest];
    }
    let cap = max_frames.max(2) - 1;
    let mut frames: Vec<Viseme> = words.into_iter().take(cap).map(word_to_viseme).collect();
    frames.push(Viseme::Rest);
    frames
}

/// Persistent Grounded XP & Achievements profile stored at `.selfware/phi/arena_profile.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhiArenaProfile {
    pub schema_version: u32,
    pub xp: u64,
    pub level: u32,
    pub best_streak: f64,
    pub bosses_slain: u32,
    pub drills_completed: u32,
    pub citations_inspected: u32,
    pub achievements: Vec<String>,
}

impl Default for PhiArenaProfile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            xp: 0,
            level: 1,
            best_streak: 1.0,
            bosses_slain: 0,
            drills_completed: 0,
            citations_inspected: 0,
            achievements: Vec::new(),
        }
    }
}

impl PhiArenaProfile {
    pub fn recompute_level_and_achievements(
        &mut self,
        current_streak: f64,
        drained_hp: u64,
        cleared_failing_run: bool,
    ) {
        if current_streak > self.best_streak {
            self.best_streak = (current_streak * 100.0).round() / 100.0;
        }
        self.level = 1 + (self.xp / 100) as u32;

        if self.citations_inspected >= 1 {
            self.unlock("first_laser_lock");
        }
        if drained_hp >= 100 || self.xp >= 100 {
            self.unlock("debt_slayer_i");
        }
        if cleared_failing_run {
            self.unlock("hydra_hunter");
        }
        if self.best_streak >= 1.61 {
            self.unlock("golden_spiral");
        }
        if self.drills_completed >= 3 {
            self.unlock("drill_master");
        }
    }

    pub fn unlock(&mut self, id: &str) -> bool {
        if !self.achievements.iter().any(|a| a == id) {
            self.achievements.push(id.to_string());
            true
        } else {
            false
        }
    }
}

/// Human-readable title for each achievement ID.
pub fn achievement_badge_title(id: &str) -> &'static str {
    match id {
        "first_laser_lock" => "🏆 Optical Lock",
        "debt_slayer_i" => "🏆 Debt Slayer I",
        "hydra_hunter" => "🏆 Hydra Hunter",
        "golden_spiral" => "🏆 Golden Spiral (1.61x φ)",
        "drill_master" => "🏆 Drill Master",
        _ => "🏆 Grounded Sentinel",
    }
}

#[cfg(unix)]
fn open_child_nofollow(
    parent: &File,
    name: &std::ffi::OsStr,
    directory: bool,
) -> std::io::Result<File> {
    use nix::libc;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    let c_name = std::ffi::CString::new(name.as_bytes())?;
    let flags = libc::O_RDONLY
        | libc::O_CLOEXEC
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    // SAFETY: `parent` owns a live directory fd and `c_name` is NUL-terminated.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), c_name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// Safely load `.selfware/phi/arena_profile.json` rejecting symlinks, hardlinks, and oversized files.
pub fn load_arena_profile(workspace_root: &Path) -> PhiArenaProfile {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(canon) = workspace_root.canonicalize() else {
            return PhiArenaProfile::default();
        };
        let Ok(root_dir) = File::open(&canon) else {
            return PhiArenaProfile::default();
        };
        let Ok(selfware_dir) = open_child_nofollow(&root_dir, ".selfware".as_ref(), true) else {
            return PhiArenaProfile::default();
        };
        let Ok(phi_dir) = open_child_nofollow(&selfware_dir, "phi".as_ref(), true) else {
            return PhiArenaProfile::default();
        };
        let Ok(file) = open_child_nofollow(&phi_dir, "arena_profile.json".as_ref(), false) else {
            return PhiArenaProfile::default();
        };
        let Ok(meta) = file.metadata() else {
            return PhiArenaProfile::default();
        };
        if !meta.is_file() || meta.nlink() != 1 || meta.len() > MAX_PROFILE_BYTES {
            return PhiArenaProfile::default();
        }
        let mut buf = Vec::new();
        if file
            .take(MAX_PROFILE_BYTES + 1)
            .read_to_end(&mut buf)
            .is_err()
            || (buf.len() as u64) > MAX_PROFILE_BYTES
        {
            return PhiArenaProfile::default();
        }
        let Ok(parsed) = serde_json::from_slice::<PhiArenaProfile>(&buf) else {
            return PhiArenaProfile::default();
        };
        if parsed.schema_version != 1 {
            return PhiArenaProfile::default();
        }
        parsed
    }
    #[cfg(not(unix))]
    {
        let _ = workspace_root;
        PhiArenaProfile::default()
    }
}

/// Atomically persist `.selfware/phi/arena_profile.json` when `.selfware/phi` is free of symlinks.
pub fn save_arena_profile(workspace_root: &Path, profile: &PhiArenaProfile) -> std::io::Result<()> {
    let canon = workspace_root.canonicalize()?;
    let selfware_path = canon.join(".selfware");
    let phi_path = selfware_path.join("phi");

    for dir in [&selfware_path, &phi_path] {
        match std::fs::symlink_metadata(dir) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "refusing to write arena profile through symlink or non-directory",
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(dir)?;
            }
            Err(e) => return Err(e),
        }
    }

    let target = phi_path.join("arena_profile.json");
    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "refusing to overwrite symlinked arena_profile.json",
            ));
        }
    }

    let tmp_path = phi_path.join(format!(".tmp-arena-profile-{}.json", std::process::id()));
    let payload = serde_json::to_vec_pretty(profile)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&tmp_path)?;
        f.write_all(&payload)?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let mut f = File::create(&tmp_path)?;
        f.write_all(&payload)?;
        f.sync_all()?;
    }

    std::fs::rename(&tmp_path, &target)?;
    Ok(())
}

/// Gamified Debt Boss state derived from `.selfware/phi/activity`.
#[derive(Debug, Clone)]
pub struct DebtBossState {
    pub boss_name: String,
    pub current_hp: u64,
    pub max_hp: u64,
    pub debt_ratio: f64,
    pub status_label: String,
    pub trust_state: &'static str,
    pub quests: Vec<String>,
    pub emotion: String,
    pub failed_runs: u64,
}

/// Compute Debt Boss state from the activity JSON returned by `read_workspace_activity`.
pub fn compute_debt_boss_state(activity: &Value, peak_hp: u64, drill_damage: u64) -> DebtBossState {
    let status = activity["status"].as_str().unwrap_or("unavailable");
    let agents: Vec<&Value> = activity["agents"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|a| a["status"].as_str() != Some("stale"))
                .collect()
        })
        .unwrap_or_default();

    if status == "unavailable" || agents.is_empty() {
        let reason = activity["reason"].as_str().unwrap_or("no_capture");
        return DebtBossState {
            boss_name: "Dormant Fog (No Live Agent Captures)".to_string(),
            current_hp: 0,
            max_hp: peak_hp.max(100),
            debt_ratio: 0.0,
            status_label: format!("OBSERVATION ABSENT ({reason}) — NOT VERIFIED"),
            trust_state: "observed_signals",
            quests: vec![
                "Press [r] to probe workspace gates & status".to_string(),
                "Press [b] to launch a grounded Boss Battle code drill".to_string(),
                "Press [h] to lock Phi's optical laser onto a workspace symbol".to_string(),
            ],
            emotion: "curious".to_string(),
            failed_runs: 0,
        };
    }

    let mut unreviewed = 0u64;
    let mut untested = 0u64;
    let mut outstanding = 0u64;
    let mut failed_runs = 0u64;
    let mut unknown_runs = 0u64;
    let mut unknown_obl = 0u64;
    let mut running = 0u64;

    for agent in &agents {
        if agent["phase"].as_str() == Some("running") {
            running += 1;
        }
        if let Some(ev) = agent["evidence"].as_object() {
            unreviewed += ev
                .get("unreviewed_lines")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            untested += ev
                .get("untested_lines")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            outstanding += ev.get("outstanding").and_then(Value::as_u64).unwrap_or(0);
            failed_runs += ev.get("failed_runs").and_then(Value::as_u64).unwrap_or(0);
            unknown_runs += ev.get("unknown_runs").and_then(Value::as_u64).unwrap_or(0);
            unknown_obl += ev
                .get("unknown_size_obligations")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + ev.get("unattributed_mutations")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
        } else {
            unknown_obl += 1;
        }
    }

    let raw_hp = unreviewed
        + untested * 2
        + outstanding * 25
        + failed_runs * 50
        + unknown_runs * 20
        + unknown_obl * 40;
    let effective_hp = raw_hp.saturating_sub(drill_damage);
    let max_hp = peak_hp.max(raw_hp).max(100);

    let mut quests = Vec::new();
    let boss_name = if failed_runs > 0 {
        quests.push(format!(
            "Slay {failed_runs} failing command run(s) (+{} XP)",
            failed_runs * 50
        ));
        "Regression Hydra (Failing Command Runs)"
    } else if unknown_obl > 0 || unknown_runs > 0 {
        quests.push(format!(
            "Audit {} uncertain mutation(s) (+40 XP)",
            unknown_obl + unknown_runs
        ));
        "Phantom Specter (Unbounded / Uncertain Mutations)"
    } else if unreviewed > 0 || untested > 0 {
        if unreviewed > 0 {
            quests.push(format!(
                "Review {unreviewed} unreviewed line(s) (+{unreviewed} XP)"
            ));
        }
        if untested > 0 {
            quests.push(format!(
                "Cover {untested} untested line(s) (+{} XP)",
                untested * 2
            ));
        }
        "Boilerplate Golem (Unreviewed & Untested Diff)"
    } else if outstanding > 0 {
        quests.push(format!(
            "Resolve {outstanding} verification obligation(s) (+{} XP)",
            outstanding * 25
        ));
        "Sentinel Ward (Outstanding Obligations)"
    } else {
        quests.push(
            "All observed agent obligations cleared — press [b] for a code drill".to_string(),
        );
        "Arena Clear (0 Observed Debt Obligations)"
    };

    let emotion = agents
        .first()
        .and_then(|a| a["fox_posture"]["emotion"].as_str())
        .unwrap_or(if failed_runs > 0 {
            "error"
        } else if effective_hp > 0 {
            "skeptical"
        } else {
            "curious"
        })
        .to_string();

    let status_label = format!(
        "{} agent(s) observed ({running} running) · failed={failed_runs} · unreviewed={unreviewed} · untested={untested}",
        agents.len()
    );

    DebtBossState {
        boss_name: boss_name.to_string(),
        current_hp: effective_hp,
        max_hp,
        debt_ratio: (effective_hp as f64 / (max_hp.max(1) as f64)).clamp(0.0, 1.0),
        status_label,
        trust_state: "observed_signals",
        quests,
        emotion,
        failed_runs,
    }
}

/// A grounded multiple-choice codebase verification drill built from real workspace source files.
#[derive(Debug, Clone)]
pub struct BossBattleDrill {
    pub question: String,
    pub symbol_name: String,
    pub citation: String,
    pub code_snippet: String,
    pub options: [String; 3],
    pub correct_index: usize,
}

#[derive(Debug, Clone)]
struct LandmarkCandidate {
    rel_path: String,
    line_no: usize,
    symbol: String,
    line_text: String,
}

/// Scan `src/` for concrete Rust landmarks to power optical laser locks and Boss Battle drills.
fn collect_workspace_landmarks(workspace_root: &Path, limit: usize) -> Vec<LandmarkCandidate> {
    let mut out = Vec::new();
    let src_dir = workspace_root.join("src");
    let mut stack = vec![src_dir];

    while let Some(dir) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        let Ok(meta) = std::fs::symlink_metadata(&dir) else {
            continue;
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();

        for path in paths {
            if out.len() >= limit {
                break;
            }
            let Ok(m) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if m.file_type().is_symlink() {
                continue;
            }
            if m.is_dir() {
                stack.push(path);
                continue;
            }
            if !m.is_file() || m.len() > MAX_SOURCE_SCAN_BYTES {
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            let Ok(rel) = path.strip_prefix(workspace_root) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (idx, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                for prefix in ["pub fn ", "pub async fn ", "pub struct ", "pub enum "] {
                    if let Some(rest) = trimmed.strip_prefix(prefix) {
                        let sym: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        if sym.len() >= 4 {
                            out.push(LandmarkCandidate {
                                rel_path: rel_str.clone(),
                                line_no: idx + 1,
                                symbol: sym,
                                line_text: trimmed.chars().take(72).collect(),
                            });
                            break;
                        }
                    }
                }
                if out.len() >= limit {
                    break;
                }
            }
        }
    }
    out
}

/// Generate a grounded 3-option Boss Battle drill from real symbols in `workspace_root/src`.
pub fn generate_workspace_drill(workspace_root: &Path, seed: usize) -> BossBattleDrill {
    let landmarks = collect_workspace_landmarks(workspace_root, 24);
    if landmarks.len() >= 3 {
        let target_idx = seed % landmarks.len();
        let target = &landmarks[target_idx];
        let mut distractor_paths = Vec::new();
        for cand in &landmarks {
            if cand.rel_path != target.rel_path && !distractor_paths.contains(&cand.rel_path) {
                distractor_paths.push(cand.rel_path.clone());
                if distractor_paths.len() == 2 {
                    break;
                }
            }
        }
        while distractor_paths.len() < 2 {
            let fallback = if distractor_paths.is_empty() {
                "src/safety/redact.rs".to_string()
            } else {
                "src/evolve/ontology.rs".to_string()
            };
            if fallback != target.rel_path && !distractor_paths.contains(&fallback) {
                distractor_paths.push(fallback);
            } else {
                distractor_paths.push("src/main.rs".to_string());
            }
        }

        let correct_slot = seed % 3;
        let mut options = [String::new(), String::new(), String::new()];
        options[correct_slot] = format!("{}:{}", target.rel_path, target.line_no);
        options[(correct_slot + 1) % 3] = format!("{}:1", distractor_paths[0]);
        options[(correct_slot + 2) % 3] = format!("{}:1", distractor_paths[1]);

        return BossBattleDrill {
            question: format!(
                "BOSS DRILL: Which workspace file & line defines `{}`?",
                target.symbol
            ),
            symbol_name: target.symbol.clone(),
            citation: format!("{}:{}", target.rel_path, target.line_no),
            code_snippet: target.line_text.clone(),
            options,
            correct_index: correct_slot,
        };
    }

    BossBattleDrill {
        question: "BOSS DRILL: What does `trust_state: \"observed_signals\"` guarantee?"
            .to_string(),
        symbol_name: "observed_signals".to_string(),
        citation: "src/evolve/server/phi_activity.rs:1".to_string(),
        code_snippet: "Honest status over optimistic success (AGENTS.md Rule 3)".to_string(),
        options: [
            "Observed telemetry only — never fabricates green for unrun checks".to_string(),
            "All unit and integration tests were executed and passed".to_string(),
            "Static types were skipped to save tokens".to_string(),
        ],
        correct_index: 0,
    }
}

/// Interactive state for the Phi Fox Arena & Debt Slayer TUI pane.
pub struct PhiArenaPane {
    pub workspace_root: PathBuf,
    pub frame_tick: usize,
    pub god_mode: bool,
    pub emotion: String,
    pub laser_target: Option<String>,
    pub viseme_queue: Vec<Viseme>,
    pub peak_hp: u64,
    pub drill_damage: u64,
    pub streak: f64,
    pub activity: Value,
    pub boss: DebtBossState,
    pub profile: PhiArenaProfile,
    pub active_drill: Option<BossBattleDrill>,
    pub bot_log: Vec<(String, String)>,
    pub drill_seed: usize,
}

/// Action returned to the outer TUI event loop when a key is pressed in `PhiArenaPane`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhiArenaAction {
    Close,
    Log(String),
}

impl PhiArenaPane {
    pub fn new(workspace_root: PathBuf) -> Self {
        let profile = load_arena_profile(&workspace_root);
        let activity = crate::evolve::server::read_workspace_activity(&workspace_root);
        let boss = compute_debt_boss_state(&activity, 100, 0);
        let emotion = boss.emotion.clone();
        let mut pane = Self {
            workspace_root,
            frame_tick: 0,
            god_mode: false,
            emotion,
            laser_target: None,
            viseme_queue: Vec::new(),
            peak_hp: boss.max_hp,
            drill_damage: 0,
            streak: profile.best_streak.max(1.0),
            activity,
            boss,
            profile,
            active_drill: None,
            bot_log: vec![(
                "Phi".to_string(),
                "Ready on perch. Press [r] Red?, [d] Debt, [h] Hunt, [b] Boss Drill, or [g] GodMode."
                    .to_string(),
            )],
            drill_seed: 0,
        };
        pane.refresh();
        pane
    }

    pub fn refresh(&mut self) {
        let prev_hp = self.boss.current_hp;
        let prev_failed = self.boss.failed_runs;
        self.activity = crate::evolve::server::read_workspace_activity(&self.workspace_root);
        self.boss = compute_debt_boss_state(&self.activity, self.peak_hp, self.drill_damage);
        self.peak_hp = self.peak_hp.max(self.boss.max_hp);

        if prev_hp > self.boss.current_hp {
            let drained = prev_hp - self.boss.current_hp;
            self.profile.xp += drained;
            if self.boss.current_hp == 0 {
                self.profile.bosses_slain += 1;
            }
            let cleared_failing = prev_failed > 0 && self.boss.failed_runs == 0;
            self.profile
                .recompute_level_and_achievements(self.streak, drained, cleared_failing);
            let _ = save_arena_profile(&self.workspace_root, &self.profile);
        }

        if self.viseme_queue.is_empty() && !self.god_mode {
            self.emotion = self.boss.emotion.clone();
        }
    }

    pub fn current_viseme(&mut self) -> Viseme {
        self.frame_tick = self.frame_tick.wrapping_add(1);
        if !self.viseme_queue.is_empty() {
            self.viseme_queue.remove(0)
        } else {
            Viseme::Rest
        }
    }

    pub fn speak(&mut self, text: &str, emotion: &str, citation: Option<String>) {
        self.emotion = emotion.to_string();
        if let Some(cite) = citation {
            self.laser_target = Some(cite);
            self.profile.citations_inspected += 1;
            self.profile
                .recompute_level_and_achievements(self.streak, 0, false);
            let _ = save_arena_profile(&self.workspace_root, &self.profile);
        }
        self.viseme_queue = text_to_viseme_frames(text, 20);
        self.bot_log.push(("Phi".to_string(), text.to_string()));
        if self.bot_log.len() > 6 {
            self.bot_log.drain(0..self.bot_log.len() - 6);
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Option<PhiArenaAction> {
        if key.code == KeyCode::Esc
            || key.code == KeyCode::Char('q')
            || key.code == KeyCode::Char('Q')
            || (key.code == KeyCode::Char('f') && key.modifiers == KeyModifiers::CONTROL)
        {
            return Some(PhiArenaAction::Close);
        }

        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh();
                let msg = if self.boss.current_hp > 0 {
                    format!(
                        "Observed signals: {}. Debt Boss HP: {}/{}.",
                        self.boss.status_label, self.boss.current_hp, self.boss.max_hp
                    )
                } else {
                    format!(
                        "{}. Honest status: absence of failed captures does not claim unrun tests passed.",
                        self.boss.status_label
                    )
                };
                let emo = if self.boss.failed_runs > 0 {
                    "error"
                } else if self.boss.current_hp > 0 {
                    "skeptical"
                } else {
                    "curious"
                };
                self.speak(&msg, emo, None);
                Some(PhiArenaAction::Log(
                    "Phi Arena: probed red signals".to_string(),
                ))
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.refresh();
                let quests = self.boss.quests.join(" | ");
                let msg = format!(
                    "Boss: {} ({}/{} HP) · Quests: {}",
                    self.boss.boss_name, self.boss.current_hp, self.boss.max_hp, quests
                );
                let emo = if self.boss.current_hp > 0 {
                    "skeptical"
                } else {
                    "analytical"
                };
                self.speak(&msg, emo, None);
                Some(PhiArenaAction::Log(
                    "Phi Arena: checked verification debt".to_string(),
                ))
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                let landmarks = collect_workspace_landmarks(&self.workspace_root, 12);
                let target = landmarks
                    .get(self.drill_seed % landmarks.len().max(1))
                    .map(|l| format!("{}:{}", l.rel_path, l.line_no))
                    .unwrap_or_else(|| "src/lib.rs:1".to_string());
                self.drill_seed = self.drill_seed.wrapping_add(1);
                let msg =
                    format!("Optical laser locked onto `{target}`. Press [b] for a Boss Drill!");
                self.speak(&msg, "analytical", Some(target.clone()));
                Some(PhiArenaAction::Log(format!(
                    "Phi Laser locked onto {target}"
                )))
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                let drill = generate_workspace_drill(&self.workspace_root, self.drill_seed);
                self.drill_seed = self.drill_seed.wrapping_add(1);
                let msg = format!(
                    "{} Press [1], [2], or [3] to strike the Debt Boss!",
                    drill.question
                );
                self.active_drill = Some(drill);
                self.speak(&msg, "working", None);
                Some(PhiArenaAction::Log(
                    "Phi Arena: started Boss Battle drill".to_string(),
                ))
            }
            KeyCode::Char(c @ ('1' | '2' | '3')) => {
                let drill = self.active_drill.take()?;
                let choice = (c as usize) - ('1' as usize);
                if choice == drill.correct_index {
                    self.drill_damage += 50;
                    self.profile.xp += 35;
                    self.profile.drills_completed += 1;
                    self.streak = ((self.streak + 0.25).min(3.0) * 100.0).round() / 100.0;
                    self.profile
                        .recompute_level_and_achievements(self.streak, 50, false);
                    let _ = save_arena_profile(&self.workspace_root, &self.profile);
                    self.refresh();
                    let msg = format!(
                        "CRITICAL HIT! `{}` verified at `{}` (+35 XP, -50 Boss HP, {:.2}x φ streak)!",
                        drill.symbol_name, drill.citation, self.streak
                    );
                    self.speak(&msg, "analytical", Some(drill.citation));
                    Some(PhiArenaAction::Log(
                        "Phi Arena: Boss Battle drill critical hit!".to_string(),
                    ))
                } else {
                    self.streak = 1.0;
                    let msg = format!(
                        "Glancing blow! `{}` actually lives at `{}` (`{}`).",
                        drill.symbol_name, drill.citation, drill.code_snippet
                    );
                    self.speak(&msg, "skeptical", Some(drill.citation));
                    None
                }
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                self.god_mode = !self.god_mode;
                let msg = if self.god_mode {
                    "✧ Celestial God Mode engaged — golden spiral telemetry active."
                } else {
                    "Returning to quiet amber study glow."
                };
                let emo = if self.god_mode { "god_mode" } else { "curious" };
                self.speak(msg, emo, None);
                None
            }
            _ => None,
        }
    }
}

/// Render the animated Phi Fox sprite lines for `ratatui`.
pub fn render_fox_sprite_lines(
    emotion: &str,
    frame_tick: usize,
    viseme: Viseme,
    god_mode: bool,
    laser_target: Option<&str>,
) -> Vec<Line<'static>> {
    let effective = if god_mode { "god_mode" } else { emotion };
    let (color, badge, eyes, ears) = match effective {
        "working" => (
            Color::Rgb(250, 204, 21),
            "WORKING · TRACKING RUN",
            "◉ . ◉",
            "/\\_/\\",
        ),
        "analytical" => (
            Color::Rgb(56, 189, 248),
            "ANALYTICAL · LASER LOCK",
            "◈ _ ◉",
            "/\\_/\\",
        ),
        "skeptical" => (
            Color::Rgb(251, 146, 60),
            "SKEPTICAL · DEBT RISING",
            "¬ . o",
            "/\\_/˜",
        ),
        "guard" => (
            Color::Rgb(217, 119, 6),
            "GUARD · UNCERTAIN EVIDENCE",
            "Ѻ . Ѻ",
            "/\\_/\\",
        ),
        "error" => (
            Color::Rgb(248, 113, 113),
            "ALERT · RED CHECKS OBSERVED",
            "> . <",
            "\\_/_/",
        ),
        "god_mode" => (
            Color::Rgb(34, 211, 238),
            "✧ GOD MODE · GOLDEN SPIRAL ✧",
            "✦ . ✦",
            "/\\φ/\\",
        ),
        _ => (
            Color::Rgb(245, 158, 11),
            "CURIOUS · EARS PERKED",
            "o . o",
            "/\\_/\\",
        ),
    };

    let spark_a = BRAILLE_SPARKS[frame_tick % BRAILLE_SPARKS.len()];
    let spark_b = BRAILLE_SPARKS[(frame_tick + 3) % BRAILLE_SPARKS.len()];
    let blink = frame_tick.is_multiple_of(19) && effective != "error";
    let shown_eyes = if blink { "- . -" } else { eyes };
    let mouth = viseme.glyph();
    let tail = ["╭─φ─╮", "╭─✦─╮", "╰─φ─╯", "╭─φ─╰"][frame_tick % 4];
    let body_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    let gold_style = Style::default().fg(Color::Rgb(245, 158, 11));
    let cyan_style = Style::default().fg(Color::Rgb(56, 189, 248));

    let laser_span = if let Some(target) = laser_target {
        Span::styled(format!("   ═══╬══► 🎯 {target}"), cyan_style)
    } else {
        Span::styled(format!("   {badge}"), body_style)
    };

    vec![
        Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("{spark_a}   φ   {spark_b}"), gold_style),
        ]),
        Line::from(vec![
            Span::raw("     "),
            Span::styled(ears.to_string(), body_style),
            laser_span,
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("( {shown_eyes} )"), body_style),
            Span::styled(
                format!("   Viseme: [{:^4}]", viseme.code().to_uppercase()),
                gold_style,
            ),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("(  {mouth}  )"), body_style),
            Span::styled(format!("   Tail:   {tail}"), gold_style),
        ]),
        Line::from(vec![
            Span::raw("   "),
            Span::styled("╭─┴─────┴─╮".to_string(), body_style),
        ]),
        Line::from(vec![
            Span::raw("   "),
            Span::styled(format!("│  ╭───╮  │ {tail}"), body_style),
        ]),
        Line::from(vec![
            Span::raw("   "),
            Span::styled("╰──┴───┴──╯".to_string(), body_style),
        ]),
    ]
}

fn format_hp_bar_string(current_hp: u64, max_hp: u64, width: usize) -> (String, Color) {
    let safe_max = max_hp.max(1);
    let ratio = (current_hp as f64 / safe_max as f64).clamp(0.0, 1.0);
    let filled = ((ratio * width as f64).round() as usize).min(width);
    let empty = width.saturating_sub(filled);
    let color = if ratio > 0.6 {
        Color::Rgb(248, 113, 113)
    } else if ratio > 0.2 {
        Color::Rgb(245, 158, 11)
    } else {
        Color::Rgb(74, 222, 128)
    };
    (
        format!(
            "[{}{}] {current_hp}/{safe_max} HP",
            "█".repeat(filled),
            "░".repeat(empty)
        ),
        color,
    )
}

/// Render the Phi Fox Arena inside `area` (used for both `PaneType::PhiArena` and `Ctrl+F` modal).
pub fn render_phi_arena(frame: &mut Frame, area: Rect, pane: &mut PhiArenaPane, focused: bool) {
    if area.width < 16 || area.height < 6 {
        let tiny = Paragraph::new("🦊 φ Arena").alignment(Alignment::Center);
        frame.render_widget(tiny, area);
        return;
    }

    let viseme = pane.current_viseme();
    let border_color = if focused {
        Color::Rgb(245, 158, 11)
    } else {
        Color::Rgb(120, 113, 108)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(
            " 🦊 φ PHI FOX ARENA & DEBT SLAYER · [TRUST: {}] ",
            pane.boss.trust_state.to_uppercase()
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(inner);

    // 1. Animated Fox Sprite
    let sprite_lines = render_fox_sprite_lines(
        &pane.emotion,
        pane.frame_tick,
        viseme,
        pane.god_mode,
        pane.laser_target.as_deref(),
    );
    frame.render_widget(Paragraph::new(sprite_lines), chunks[0]);

    // 2. Debt Boss HP & Grounded XP / Achievements
    let (hp_bar, hp_color) = format_hp_bar_string(pane.boss.current_hp, pane.boss.max_hp, 22);
    let badges: String = if pane.profile.achievements.is_empty() {
        "None yet — press [h] or [b] to unlock".to_string()
    } else {
        pane.profile
            .achievements
            .iter()
            .map(|id| achievement_badge_title(id))
            .collect::<Vec<_>>()
            .join(" · ")
    };

    let boss_lines = vec![
        Line::from(vec![
            Span::styled(" ⚔ BOSS: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(pane.boss.boss_name.clone()),
            Span::styled(
                format!(
                    "   Lv.{} · {} XP ({:.2}x φ)",
                    pane.profile.level, pane.profile.xp, pane.streak
                ),
                Style::default()
                    .fg(Color::Rgb(74, 222, 128))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(" ♥ DEBT: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(hp_bar, Style::default().fg(hp_color)),
        ]),
        Line::from(vec![
            Span::styled(" 📡 ", Style::default().fg(Color::Rgb(168, 162, 158))),
            Span::styled(
                pane.boss.status_label.clone(),
                Style::default().fg(Color::Rgb(168, 162, 158)),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                " 🏅 BADGES: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(badges, Style::default().fg(Color::Rgb(250, 204, 21))),
        ]),
    ];
    frame.render_widget(Paragraph::new(boss_lines), chunks[1]);

    // 3. Quests / Active Boss Battle Drill & Bot Channel
    let mut body_lines: Vec<Line> = Vec::new();
    if let Some(drill) = &pane.active_drill {
        body_lines.push(Line::from(Span::styled(
            format!(" ⚡ {}", drill.question),
            Style::default()
                .fg(Color::Rgb(56, 189, 248))
                .add_modifier(Modifier::BOLD),
        )));
        for (idx, opt) in drill.options.iter().enumerate() {
            body_lines.push(Line::from(format!("    [{}] {}", idx + 1, opt)));
        }
    } else {
        body_lines.push(Line::from(Span::styled(
            " 📜 ACTIVE QUESTS & VERIFICATION OBLIGATIONS:",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for q in pane.boss.quests.iter().take(2) {
            body_lines.push(Line::from(format!("    • {q}")));
        }
    }
    if let Some((speaker, msg)) = pane.bot_log.last() {
        body_lines.push(Line::from(vec![
            Span::styled(
                format!(" 💬 {speaker}: "),
                Style::default()
                    .fg(Color::Rgb(56, 189, 248))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(msg.clone()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(body_lines).wrap(Wrap { trim: false }),
        chunks[2],
    );

    // 4. Hotkey Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" Keys: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled("[r]", Style::default().fg(Color::Rgb(245, 158, 11))),
        Span::raw(" Red?  "),
        Span::styled("[d]", Style::default().fg(Color::Rgb(245, 158, 11))),
        Span::raw(" Debt  "),
        Span::styled("[h]", Style::default().fg(Color::Rgb(245, 158, 11))),
        Span::raw(" Hunt  "),
        Span::styled("[b]", Style::default().fg(Color::Rgb(245, 158, 11))),
        Span::raw(" Boss Drill  "),
        Span::styled("[1-3]", Style::default().fg(Color::Rgb(56, 189, 248))),
        Span::raw(" Strike  "),
        Span::styled("[g]", Style::default().fg(Color::Rgb(245, 158, 11))),
        Span::raw(" GodMode  "),
        Span::styled("[Esc/q]", Style::default().fg(Color::Rgb(245, 158, 11))),
        Span::raw(" Close"),
    ]));
    frame.render_widget(footer, chunks[3]);
}

/// Render `PhiArenaPane` as a centered modal overlay when opened via `Ctrl+F` or `/phi`.
pub fn render_phi_arena_modal(frame: &mut Frame, area: Rect, pane: &mut PhiArenaPane) {
    let modal_w = area.width.saturating_sub(4).max(area.width.min(20));
    let modal_h = area.height.saturating_sub(2).max(area.height.min(10));
    let x = area.x + (area.width.saturating_sub(modal_w)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_h)) / 2;
    let modal_rect = Rect::new(x, y, modal_w, modal_h);
    frame.render_widget(Clear, modal_rect);
    render_phi_arena(frame, modal_rect, pane, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    use serde_json::json;

    #[test]
    fn all_ten_visemes_are_reachable_in_rust_engine() {
        assert_eq!(word_to_viseme("..."), Viseme::Rest);
        assert_eq!(word_to_viseme("module"), Viseme::Mbp);
        assert_eq!(word_to_viseme("fox"), Viseme::Fv);
        assert_eq!(word_to_viseme("laser"), Viseme::LTh);
        assert_eq!(word_to_viseme("quick"), Viseme::Wq);
        assert_eq!(word_to_viseme("cairn"), Viseme::Ai);
        assert_eq!(word_to_viseme("test"), Viseme::E);
        assert_eq!(word_to_viseme("code"), Viseme::O);
        assert_eq!(word_to_viseme("rust"), Viseme::U);
        assert_eq!(word_to_viseme("sync"), Viseme::Etc);

        let frames = text_to_viseme_frames("module fox laser quick cairn test code rust sync", 16);
        assert_eq!(frames.last(), Some(&Viseme::Rest));
        assert!(frames.len() >= 10);
    }

    #[test]
    fn debt_boss_hp_drains_with_drill_damage_and_preserves_honest_trust_state() {
        let activity = json!({
            "status": "available",
            "reason": null,
            "agents": [{
                "status": "available",
                "phase": "running",
                "evidence": {
                    "unreviewed_lines": 60,
                    "untested_lines": 20,
                    "outstanding": 2,
                    "failed_runs": 1,
                    "unknown_runs": 0,
                    "unknown_size_obligations": 0,
                    "unattributed_mutations": 0
                }
            }]
        });
        // 60 + 20*2 + 2*25 + 1*50 = 200 HP
        let boss = compute_debt_boss_state(&activity, 100, 0);
        assert_eq!(boss.current_hp, 200);
        assert_eq!(boss.trust_state, "observed_signals");
        assert!(boss.boss_name.contains("Regression Hydra"));

        let after_strike = compute_debt_boss_state(&activity, boss.max_hp, 50);
        assert_eq!(after_strike.current_hp, 150);
    }

    #[test]
    fn arena_profile_roundtrip_and_rejects_symlinks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path();

        let mut profile = PhiArenaProfile {
            xp: 140,
            citations_inspected: 1,
            drills_completed: 3,
            ..Default::default()
        };
        profile.recompute_level_and_achievements(1.75, 120, true);

        save_arena_profile(ws, &profile).expect("save profile");
        let loaded = load_arena_profile(ws);
        assert_eq!(loaded.level, 2);
        assert!(loaded
            .achievements
            .contains(&"first_laser_lock".to_string()));
        assert!(loaded.achievements.contains(&"debt_slayer_i".to_string()));
        assert!(loaded.achievements.contains(&"hydra_hunter".to_string()));
        assert!(loaded.achievements.contains(&"golden_spiral".to_string()));
        assert!(loaded.achievements.contains(&"drill_master".to_string()));

        #[cfg(unix)]
        {
            let target = ws.join(".selfware/phi/arena_profile.json");
            std::fs::remove_file(&target).unwrap();
            let outside = ws.join("outside.json");
            std::fs::write(&outside, "{}").unwrap();
            std::os::unix::fs::symlink(&outside, &target).unwrap();

            assert!(save_arena_profile(ws, &profile).is_err());
            let fallback = load_arena_profile(ws);
            assert_eq!(fallback.xp, 0);
        }
    }

    #[test]
    fn interactive_drill_and_render_survive_normal_and_tiny_terminals() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path();
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(
            ws.join("src/alpha.rs"),
            "pub fn alpha_sentinel_gate() -> bool { true }\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/beta.rs"),
            "pub struct BetaVerificationLedger { pub count: usize }\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/gamma.rs"),
            "pub enum GammaTrustOutcome { Observed, Structural }\n",
        )
        .unwrap();

        let mut pane = PhiArenaPane::new(ws.to_path_buf());
        pane.on_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        let drill = pane.active_drill.clone().expect("drill spawned");
        let correct_char = char::from_digit((drill.correct_index + 1) as u32, 10).unwrap();
        pane.on_key(KeyEvent::new(
            KeyCode::Char(correct_char),
            KeyModifiers::NONE,
        ));
        assert_eq!(pane.profile.drills_completed, 1);
        assert!(pane.profile.xp >= 35);

        // Normal terminal render
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_phi_arena_modal(frame, frame.area(), &mut pane))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let rendered: String = buffer.content().iter().map(|c| c.symbol()).collect();
        assert!(rendered.contains("PHI FOX ARENA"));
        assert!(rendered.contains("OBSERVED_SIGNALS"));

        // Tiny terminal survival (10x4)
        let tiny_backend = TestBackend::new(10, 4);
        let mut tiny_term = Terminal::new(tiny_backend).unwrap();
        tiny_term
            .draw(|frame| render_phi_arena_modal(frame, frame.area(), &mut pane))
            .unwrap();
    }
}
