//! Skill Distillation System (Trace2Skill & Metis)
//!
//! Autonomous distillation of episodic and session traces into reusable skills,
//! SOPs, and composite tool workflows. Implements the dual-analyst pattern:
//! - **Error Analyst**: analyzes failed trajectories, extracting actionable mitigation rules.
//! - **Success Analyst**: extracts Standard Operating Procedures (SOPs) from successful tasks.
//! - **Metis Pattern Miner**: detects frequent multi-step tool call sequences and compiles them into composite macros.
//! - **Library Drift Ledger**: tracks invocation frequency and utility, enforcing a capacity cap (default 50 skills)
//!   via utility-weighted eviction to prevent retrieval dilution.
//!
//! Note on dormancy: The skill distillation engine is integrated into the memory consolidation pipeline
//! but is dormant on the primary agent loop until explicitly invoked or scheduled during sleep consolidation cycles.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::agent::session_log::{SessionEventType, SessionLogEvent};
use crate::consolidation::collector::{CollectedItem, SourceType};
use crate::skills::Skill;

/// Maximum number of skills retained in the library before eviction triggers.
pub const DEFAULT_MAX_SKILLS_CAP: usize = 50;

/// Category of distilled skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistilledSkillType {
    /// Defensive rules and corrective patterns extracted from failures.
    ErrorMitigation,
    /// Standard Operating Procedures distilled from successful task workflows.
    StandardOperatingProcedure,
    /// Composite multi-tool recipes mined by Metis pattern detector.
    ToolComposite,
}

impl DistilledSkillType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ErrorMitigation => "error_mitigation",
            Self::StandardOperatingProcedure => "sop",
            Self::ToolComposite => "tool_composite",
        }
    }
}

/// A distilled skill candidate ready for storage and registry discovery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DistilledSkill {
    pub name: String,
    pub skill_type: DistilledSkillType,
    pub description: String,
    pub tools: Vec<String>,
    pub triggers: Vec<String>,
    pub content: String,
}

fn is_empty_slice(s: &&[String]) -> bool {
    s.is_empty()
}

#[derive(Serialize)]
struct DistilledSkillFrontmatter<'a> {
    name: &'a str,
    description: &'a str,
    category: &'a str,
    verified: bool,
    #[serde(skip_serializing_if = "is_empty_slice")]
    tools: &'a [String],
    #[serde(skip_serializing_if = "is_empty_slice")]
    triggers: &'a [String],
}

impl DistilledSkill {
    /// Formats the skill into markdown with YAML frontmatter compatible with `Skill::from_markdown`.
    pub fn to_markdown(&self) -> String {
        let fm = DistilledSkillFrontmatter {
            name: &self.name,
            description: &self.description,
            category: self.skill_type.as_str(),
            verified: false,
            tools: &self.tools,
            triggers: &self.triggers,
        };
        let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
        let mut out = format!("---\n{}---\n\n{}", yaml, self.content);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// Entry in the skill library drift ledger tracking utilization and performance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillLedgerEntry {
    pub name: String,
    pub category: String,
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
    pub invocations: u32,
    pub success_count: u32,
    pub utility_score: f64,
}

impl SkillLedgerEntry {
    pub fn new(name: &str, category: &str) -> Self {
        let now = Utc::now();
        Self {
            name: name.to_string(),
            category: category.to_string(),
            created_at: now,
            last_used: now,
            invocations: 0,
            success_count: 0,
            utility_score: 1.0,
        }
    }

    /// Recalculates utility score based on success rate, usage frequency, and recency decay.
    pub fn compute_utility(&mut self, now: DateTime<Utc>, decay_rate_per_hour: f64) -> f64 {
        let hours_idle = (now - self.last_used).num_seconds().max(0) as f64 / 3600.0;
        let success_rate = (self.success_count as f64 + 1.0) / (self.invocations as f64 + 2.0);
        let frequency_weight = 1.0 + (1.0 + self.invocations as f64).ln();
        let recency_decay = (-decay_rate_per_hour * hours_idle).exp();
        let score = success_rate * frequency_weight * recency_decay;
        self.utility_score = score;
        score
    }
}

/// Summary report produced after running skill distillation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillDistillationReport {
    pub skills_created: usize,
    pub skills_updated: usize,
    pub skills_evicted: usize,
    pub created_skill_names: Vec<String>,
    pub evicted_skill_names: Vec<String>,
}

/// Skill distillation engine wiring memory consolidation to reusable skills.
pub struct SkillDistiller {
    skills_dir: PathBuf,
    ledger_file: PathBuf,
    max_skills_cap: usize,
    min_pattern_support: usize,
    decay_rate_per_hour: f64,
}

impl SkillDistiller {
    pub fn new(skills_dir: PathBuf, ledger_file: PathBuf) -> Self {
        Self {
            skills_dir,
            ledger_file,
            max_skills_cap: DEFAULT_MAX_SKILLS_CAP,
            min_pattern_support: 2,
            decay_rate_per_hour: 0.005,
        }
    }

    pub fn with_max_skills_cap(mut self, cap: usize) -> Self {
        self.max_skills_cap = cap.max(1);
        self
    }

    pub fn with_min_pattern_support(mut self, support: usize) -> Self {
        self.min_pattern_support = support.max(1);
        self
    }

    pub fn skills_dir(&self) -> &Path {
        &self.skills_dir
    }

    pub fn ledger_file(&self) -> &Path {
        &self.ledger_file
    }

    /// Load the skill drift ledger from disk or initialize an empty one.
    pub fn load_ledger(&self) -> Result<HashMap<String, SkillLedgerEntry>> {
        if !self.ledger_file.exists() {
            return Ok(HashMap::new());
        }
        let data = fs::read_to_string(&self.ledger_file)
            .with_context(|| format!("Failed to read ledger from {:?}", self.ledger_file))?;
        let ledger: HashMap<String, SkillLedgerEntry> = serde_json::from_str(&data)
            .with_context(|| format!("Failed to parse ledger json from {:?}", self.ledger_file))?;
        Ok(ledger)
    }

    /// Save the skill drift ledger to disk atomically.
    pub fn save_ledger(&self, ledger: &HashMap<String, SkillLedgerEntry>) -> Result<()> {
        if let Ok(meta) = self.ledger_file.symlink_metadata() {
            if meta.file_type().is_symlink() {
                return Err(anyhow!(
                    "Ledger destination is a symlink: {:?}",
                    self.ledger_file
                ));
            }
        }
        if let Some(parent) = self.ledger_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(ledger)?;
        let tmp_path = self.ledger_file.with_extension("tmp");
        if let Ok(meta) = tmp_path.symlink_metadata() {
            if meta.file_type().is_symlink() {
                return Err(anyhow!(
                    "Ledger temporary destination is a symlink: {:?}",
                    tmp_path
                ));
            }
        }
        fs::write(&tmp_path, json)?;
        fs::rename(tmp_path, &self.ledger_file)?;
        Ok(())
    }

    /// Error Analyst: analyzes an error trace/message and generates a mitigation skill.
    pub fn analyze_error_trace(
        &self,
        error_content: &str,
        tool_hint: Option<&str>,
    ) -> Option<DistilledSkill> {
        let content_lower = error_content.to_lowercase();

        let (name, signature, mitigation, tools) = if content_lower.contains("cannot borrow")
            || content_lower.contains("borrowed as mutable")
            || content_lower.contains("e0382")
            || content_lower.contains("e0502")
        {
            (
                "mitigate_rust_borrow_checker".to_string(),
                "Rust borrow checker conflict (aliasing / mutable borrow across scope)",
                "Refactor borrow lifetime: clone data if small, limit mutable borrow scopes, or use interior mutability (Arc/Mutex/RefCell). Check code with `cargo check` after minimal refactoring.",
                vec!["cargo_check".to_string(), "file_edit".to_string()],
            )
        } else if content_lower.contains("cannot find value")
            || content_lower.contains("unresolved import")
            || content_lower.contains("e0425")
            || content_lower.contains("e0432")
        {
            (
                "mitigate_rust_unresolved_symbol".to_string(),
                "Unresolved symbol, missing module import, or crate dependency missing",
                "Verify module path and visibility (`pub(crate)` vs `pub`). Verify `Cargo.toml` dependency existence and import in `lib.rs` / `mod.rs`.",
                vec!["cargo_check".to_string(), "file_edit".to_string()],
            )
        } else if content_lower.contains("tool_validation_failed")
            || content_lower.contains("validation failed")
            || content_lower.contains("schema mismatch")
        {
            let raw_tool = tool_hint.unwrap_or("tool");
            let safe_tool: String = raw_tool
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let safe_tool = if safe_tool.is_empty() {
                "tool".to_string()
            } else {
                safe_tool
            };
            (
                format!("mitigate_{}_args_validation", safe_tool),
                "Tool call argument validation failure / schema violation",
                "Inspect tool contract and required JSON parameters before calling. Ensure all mandatory parameters are supplied with strict typing.",
                vec![safe_tool],
            )
        } else if content_lower.contains("timeout")
            || content_lower.contains("timed out")
            || content_lower.contains("deadline exceeded")
        {
            (
                "mitigate_operation_timeout".to_string(),
                "Execution timeout in subprocess or external network call",
                "Check command batch size or remote endpoint responsiveness. Increase timeout threshold or break operation into smaller chunks.",
                vec!["bash".to_string()],
            )
        } else if content_lower.contains("test failed")
            || content_lower.contains("panicked at")
            || content_lower.contains("assertion `left == right` failed")
        {
            (
                "mitigate_test_assertion_regression".to_string(),
                "Unit/integration test failure or assertion regression",
                "Inspect test failure diff. Sweeping bug fix: ensure no side effects mutate invariants; re-run specific test with `cargo test <test_name>` before full commit.",
                vec!["cargo_test".to_string(), "file_edit".to_string()],
            )
        } else {
            return None;
        };

        let skill_content = format!(
            "# Error Mitigation Procedure: {}\n\n\
             ## Signature\n{}\n\n\
             ## Recommended Mitigation\n{}\n\n\
             ## Action Checklist\n\
             1. Isolate the failure site and identify offending inputs.\n\
             2. Apply targeted fix without altering peripheral logic.\n\
             3. Validate fix locally before continuing.\n",
            name, signature, mitigation
        );

        Some(DistilledSkill {
            name,
            skill_type: DistilledSkillType::ErrorMitigation,
            description: format!("Automatic mitigation rule for {}", signature),
            tools,
            triggers: vec![signature.to_string()],
            content: skill_content,
        })
    }

    /// Success Analyst: analyzes successful multi-turn workflows and synthesizes SOPs.
    pub fn analyze_success_trace(
        &self,
        goal: &str,
        steps: &[String],
        tools_used: &[String],
    ) -> Option<DistilledSkill> {
        if steps.is_empty() || goal.trim().is_empty() {
            return None;
        }

        let clean_goal = goal
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '_')
            .collect::<String>();
        let words: Vec<&str> = clean_goal.split_whitespace().collect();
        if words.is_empty() {
            return None;
        }
        let short_name = words
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join("_")
            .to_lowercase();
        let name = format!("sop_{}", short_name);

        let mut procedure_steps = String::new();
        for (i, step) in steps.iter().enumerate() {
            procedure_steps.push_str(&format!("{}. {}\n", i + 1, step));
        }

        let body = format!(
            "# Standard Operating Procedure: {}\n\n\
             ## Objective\n{}\n\n\
             ## Recommended Execution Sequence\n{}\n\
             ## Verification\n\
             - Verify that each step outputs zero error code.\n\
             - Inspect final outputs against required invariants.\n",
            short_name, goal, procedure_steps
        );

        Some(DistilledSkill {
            name,
            skill_type: DistilledSkillType::StandardOperatingProcedure,
            description: format!("Standard operating procedure for {}", goal),
            tools: tools_used.to_vec(),
            triggers: vec![goal.to_string()],
            content: body,
        })
    }

    /// Metis Pattern Miner: extracts repeated multi-tool sequences across sessions.
    pub fn mine_metis_patterns(&self, tool_sequences: &[Vec<String>]) -> Vec<DistilledSkill> {
        let mut ngram_counts: HashMap<Vec<String>, usize> = HashMap::new();

        for seq in tool_sequences {
            if seq.len() < 2 {
                continue;
            }
            for n in 2..=3 {
                if seq.len() >= n {
                    for window in seq.windows(n) {
                        *ngram_counts.entry(window.to_vec()).or_insert(0) += 1;
                    }
                }
            }
        }

        let mut mined_skills = Vec::new();
        for (ngram, count) in ngram_counts {
            if count >= self.min_pattern_support {
                let safe_tools: Vec<String> = ngram
                    .iter()
                    .map(|t| {
                        let clean: String = t
                            .chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        if clean.is_empty() {
                            "tool".to_string()
                        } else {
                            clean
                        }
                    })
                    .collect();
                let name = format!("composite_{}", safe_tools.join("_and_"));
                let description = format!(
                    "Chained tool workflow executing {} in sequence (observed {} times)",
                    safe_tools.join(" -> "),
                    count
                );
                let mut content = format!(
                    "# Composite Tool Workflow: {}\n\n\
                     ## Pipeline\n",
                    name
                );
                for (idx, tool) in safe_tools.iter().enumerate() {
                    content.push_str(&format!(
                        "{}. Call `{}` with piped inputs.\n",
                        idx + 1,
                        tool
                    ));
                }
                content.push_str("\n## Validation Gate\nEnsure all sub-tool invocations succeed before finalizing.\n");

                mined_skills.push(DistilledSkill {
                    name,
                    skill_type: DistilledSkillType::ToolComposite,
                    description,
                    tools: safe_tools.clone(),
                    triggers: vec![format!("run {}", safe_tools.join(" then "))],
                    content,
                });
            }
        }

        mined_skills.sort_by(|a, b| a.name.cmp(&b.name));
        mined_skills
    }

    /// Distills skills from session log events, updates ledger, and persists new skills.
    pub fn distill_from_session_events(
        &mut self,
        events: &[SessionLogEvent],
    ) -> Result<SkillDistillationReport> {
        let mut candidates = Vec::new();
        let mut tool_sequences: Vec<Vec<String>> = Vec::new();
        let mut current_seq: Vec<String> = Vec::new();

        for event in events {
            match event.event_type {
                SessionEventType::ToolCall => {
                    if let Some(ref t) = event.tool_name {
                        current_seq.push(t.clone());
                    }
                }
                SessionEventType::TurnEnd | SessionEventType::TaskEnd => {
                    if !current_seq.is_empty() {
                        tool_sequences.push(std::mem::take(&mut current_seq));
                    }
                }
                SessionEventType::ToolValidationFailed => {
                    if let Some(skill) = self.analyze_error_trace(
                        event.result.as_deref().unwrap_or("validation failed"),
                        event.tool_name.as_deref(),
                    ) {
                        candidates.push(skill);
                    }
                }
                _ => {}
            }

            // Deduplicate: ToolValidationFailed is already handled in its own match arm above.
            if event.success == Some(false)
                && event.event_type != SessionEventType::ToolValidationFailed
            {
                if let Some(res) = event.result.as_deref() {
                    if let Some(skill) = self.analyze_error_trace(res, event.tool_name.as_deref()) {
                        candidates.push(skill);
                    }
                }
            }
        }
        if !current_seq.is_empty() {
            tool_sequences.push(current_seq);
        }

        let composite_skills = self.mine_metis_patterns(&tool_sequences);
        candidates.extend(composite_skills);

        self.commit_distilled_skills(candidates)
    }

    /// Distills skills from collected consolidation items.
    pub fn distill_from_collected_items(
        &mut self,
        items: &[CollectedItem],
    ) -> Result<SkillDistillationReport> {
        let mut candidates = Vec::new();

        for item in items {
            match item.source_type {
                SourceType::ToolResult => {
                    if item.content.contains("error") || item.content.contains("failed") {
                        if let Some(skill) = self.analyze_error_trace(&item.content, None) {
                            candidates.push(skill);
                        }
                    }
                }
                SourceType::Episode if !item.content.contains("failed") && item.importance >= 2 => {
                    let lines: Vec<String> = item
                        .content
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(ToString::to_string)
                        .collect();
                    if let Some(first) = lines.first() {
                        let steps = if lines.len() > 1 {
                            lines[1..].to_vec()
                        } else {
                            vec![first.clone()]
                        };
                        if let Some(sop) = self.analyze_success_trace(first, &steps, &item.tags) {
                            candidates.push(sop);
                        }
                    }
                }
                _ => {}
            }
        }

        self.commit_distilled_skills(candidates)
    }

    /// Commit candidates to disk, update ledger, and enforce capacity cap.
    pub fn commit_distilled_skills(
        &mut self,
        candidates: Vec<DistilledSkill>,
    ) -> Result<SkillDistillationReport> {
        fs::create_dir_all(&self.skills_dir)?;
        let mut ledger = self.load_ledger()?;
        let now = Utc::now();

        let mut report = SkillDistillationReport::default();

        for candidate in candidates {
            let md = candidate.to_markdown();
            Skill::from_markdown(&md).map_err(|e| anyhow!("Invalid skill generated: {e}"))?;

            let safe_name: String = candidate
                .name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            let safe_name = if safe_name.is_empty() {
                "unnamed_skill".to_string()
            } else {
                safe_name
            };

            let skill_path = self.skills_dir.join(format!("{}.md", safe_name));
            if !skill_path.starts_with(&self.skills_dir) {
                return Err(anyhow!(
                    "Skill path escapes skills directory: {}",
                    candidate.name
                ));
            }
            if let Ok(meta) = skill_path.symlink_metadata() {
                if meta.file_type().is_symlink() {
                    return Err(anyhow!(
                        "Destination skill path is a symlink: {:?}",
                        skill_path
                    ));
                }
            }
            let is_new = !skill_path.exists();

            fs::write(&skill_path, md)?;

            let entry = ledger.entry(safe_name.clone()).or_insert_with(|| {
                SkillLedgerEntry::new(&safe_name, candidate.skill_type.as_str())
            });
            entry.last_used = now;
            entry.compute_utility(now, self.decay_rate_per_hour);

            if is_new {
                report.skills_created += 1;
                report.created_skill_names.push(safe_name);
            } else {
                report.skills_updated += 1;
            }
        }

        let evicted = self.enforce_capacity_cap_internal(&mut ledger, now)?;
        report.skills_evicted = evicted.len();
        report.evicted_skill_names = evicted;

        self.save_ledger(&ledger)?;
        Ok(report)
    }

    /// Record a skill invocation event (hit or miss) in the drift ledger.
    pub fn record_skill_usage(&mut self, skill_name: &str, success: bool) -> Result<()> {
        let mut ledger = self.load_ledger()?;
        let now = Utc::now();

        if let Some(entry) = ledger.get_mut(skill_name) {
            entry.invocations += 1;
            if success {
                entry.success_count += 1;
            }
            entry.last_used = now;
            entry.compute_utility(now, self.decay_rate_per_hour);
            self.save_ledger(&ledger)?;
            Ok(())
        } else {
            Err(anyhow!("Skill '{}' not found in ledger", skill_name))
        }
    }

    fn enforce_capacity_cap_internal(
        &self,
        ledger: &mut HashMap<String, SkillLedgerEntry>,
        now: DateTime<Utc>,
    ) -> Result<Vec<String>> {
        let mut evicted = Vec::new();
        if ledger.len() <= self.max_skills_cap {
            return Ok(evicted);
        }

        for entry in ledger.values_mut() {
            entry.compute_utility(now, self.decay_rate_per_hour);
        }

        let mut sorted_entries: Vec<_> = ledger.values().cloned().collect();
        sorted_entries.sort_by(|a, b| {
            a.utility_score
                .partial_cmp(&b.utility_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.last_used.cmp(&b.last_used))
                .then_with(|| a.name.cmp(&b.name))
        });

        let excess = ledger.len() - self.max_skills_cap;
        for entry in sorted_entries.iter().take(excess) {
            let safe_name: String = entry
                .name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            let skill_file = self.skills_dir.join(format!("{}.md", safe_name));
            if skill_file.starts_with(&self.skills_dir) && skill_file.exists() {
                if let Err(e) = fs::remove_file(&skill_file) {
                    tracing::warn!(
                        "Failed to remove evicted skill file {:?}: {}",
                        skill_file,
                        e
                    );
                    continue; // Skip removing from ledger if disk removal failed
                }
            }
            ledger.remove(&entry.name);
            evicted.push(entry.name.clone());
            info!(
                "Evicted skill '{}' (utility: {:.4}) under capacity cap {}",
                entry.name, entry.utility_score, self.max_skills_cap
            );
        }

        Ok(evicted)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/consolidation/skill_distiller_test.rs"]
mod tests;
