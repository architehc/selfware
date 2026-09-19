//! RSI Active Investigative Search & Review Interface
//!
//! Provides deep structural auditing of autonomous mutations generated during
//! Recursive Self-Improvement (RSI). Identifies opaque or under-specified code
//! modifications, establishes 6 degrees of grounded connection, attaches
//! maximum-power cryptographic citations, and models 10,000:1 reviewer governance.

use super::tree_log::{compute_sha256, AttemptNode, AttemptStatus, FailureClass};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Maximum-power citation linking directly to exact file, line range, and content hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroundedCitation {
    pub citation_id: String,
    pub file_path: String,
    pub line_range: (usize, usize),
    pub symbol: Option<String>,
    pub content_hash: String,
    pub git_commit: Option<String>,
    pub attempt_id: Option<String>,
    pub exact_excerpt: String,
    pub hyperlink: String,
    #[serde(default)]
    pub before_excerpt: Option<String>,
    #[serde(default)]
    pub after_excerpt: Option<String>,
    /// SHA256 digest of the baseline before-excerpt grounded in git revision.
    #[serde(default)]
    pub before_content_hash: Option<String>,
}

/// Category of opaque, under-specified, or hazard-prone structure detected during RSI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpaqueCategory {
    /// Literal magic constants introduced into logic without named constant or config binding.
    ImplicitConstant,
    /// Unchecked panic vectors (.unwrap(), .expect()) added in production code paths.
    UncheckedUnwrap,
    /// Modifications touching files or interfaces outside the declared mutation target.
    BlastRadiusLeak,
    /// New public structures or functions lacking docstrings and invariants.
    UndocumentedPublicApi,
    /// Modified return types or error conditions risking contract erosion.
    ContractBreakage,
    /// Silenced error conditions or unlogged fallback defaults.
    SilentFallback,
}

impl std::fmt::Display for OpaqueCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImplicitConstant => write!(f, "ImplicitConstant"),
            Self::UncheckedUnwrap => write!(f, "UncheckedUnwrap"),
            Self::BlastRadiusLeak => write!(f, "BlastRadiusLeak"),
            Self::UndocumentedPublicApi => write!(f, "UndocumentedPublicApi"),
            Self::ContractBreakage => write!(f, "ContractBreakage"),
            Self::SilentFallback => write!(f, "SilentFallback"),
        }
    }
}

/// Finding severity level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for FindingSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// An identified opaque structure finding accompanied by a grounded citation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpaqueStructureFinding {
    pub category: OpaqueCategory,
    pub severity: FindingSeverity,
    pub title: String,
    pub explanation: String,
    pub citation: GroundedCitation,
    pub remediation: String,
}

/// Degree 1: Origin & Hypothesis Intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Degree1Intent {
    pub hypothesis_id: String,
    pub description: String,
    pub parent_attempt_id: Option<String>,
    pub parent_failure_reason: Option<String>,
    pub parent_failure_class: Option<FailureClass>,
}

/// Degree 2: Syntactic & AST Mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Degree2Syntax {
    pub diff_sha256: String,
    pub files_touched: Vec<String>,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub is_json_search_replace: bool,
}

/// Degree 3: Ontological Neighborhood & Concepts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Degree3Ontology {
    pub primary_symbols: Vec<String>,
    pub callers_affected: Vec<String>,
    pub cooccurring_concepts: Vec<String>,
}

/// Degree 4: Empirical & Benchmark Verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Degree4Empirical {
    pub status: AttemptStatus,
    pub composite_score: Option<f64>,
    pub sab_score: Option<f64>,
    pub tests_passed: Option<usize>,
    pub tests_total: Option<usize>,
    pub wall_time_ms: u64,
    pub tokens_used: Option<u64>,
}

/// Degree 5: Safety & Invariant Envelopes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Degree5Safety {
    pub protected_paths_clean: bool,
    pub rule1_verified: bool,
    pub merkle_tree_equality: Option<bool>,
    pub has_killswitch_bypass: bool,
}

/// Degree 6: Lineage, Provenance & Ecosystem Inheritance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Degree6Lineage {
    pub generation: usize,
    pub branch_id: String,
    pub base_commit: Option<String>,
    pub committed_commit: Option<String>,
    pub lineage_depth: usize,
    pub promotes_to_main: bool,
}

/// Full 6 Degrees of Grounded Connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SixDegreesOfConnection {
    pub degree_1_intent: Degree1Intent,
    pub degree_2_syntax: Degree2Syntax,
    pub degree_3_ontology: Degree3Ontology,
    pub degree_4_empirical: Degree4Empirical,
    pub degree_5_safety: Degree5Safety,
    pub degree_6_lineage: Degree6Lineage,
}

/// Governance decision issued by the reviewer panel.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GovernanceDecision {
    ApproveForPromotion,
    ConditionalClarification,
    QuarantineForExperimentation,
    HardRejectVeto,
}

impl std::fmt::Display for GovernanceDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApproveForPromotion => write!(f, "APPROVE FOR PROMOTION"),
            Self::ConditionalClarification => write!(f, "CONDITIONAL: CLARIFICATION REQUIRED"),
            Self::QuarantineForExperimentation => {
                write!(f, "QUARANTINE: FURTHER BENCHMARKING REQUIRED")
            }
            Self::HardRejectVeto => write!(f, "HARD REJECT & VETO"),
        }
    }
}

/// Specialized evaluation from one of the 4 evaluator personas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerPerspective {
    pub name: String,
    pub total_reviewers: usize,
    pub votes_approve: usize,
    pub votes_clarification: usize,
    pub votes_quarantine: usize,
    pub votes_veto: usize,
    pub perspective_score: f64,
    pub assessment: String,
}

/// Synthesized consensus across 10,000 independent synthetic reviewers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerConsensus {
    pub total_reviewers: usize,
    pub votes_approve: usize,
    pub votes_clarification: usize,
    pub votes_quarantine: usize,
    pub votes_veto: usize,
    pub consensus_score: f64,
    pub decision: GovernanceDecision,
    pub has_safety_veto: bool,
    pub perspectives: Vec<ReviewerPerspective>,
    pub deliberation_summary: String,
}

/// The complete investigative dossier for an RSI attempt or commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestigativeDossier {
    pub attempt_id: String,
    pub timestamp: String,
    pub degrees: SixDegreesOfConnection,
    pub findings: Vec<OpaqueStructureFinding>,
    pub citations: Vec<GroundedCitation>,
    pub consensus: ReviewerConsensus,
}

/// Extracts touched file paths across both JSON search-replace and unified diff formats.
pub fn extract_touched_files(patch: &str) -> Vec<String> {
    let mut files = Vec::new();

    // 1. JSON search-replace format
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch) {
        for edit in edits {
            if let Some(f) = edit
                .get("file")
                .or_else(|| edit.get("path"))
                .and_then(|v| v.as_str())
            {
                let trimmed = f.trim();
                if !trimmed.is_empty() {
                    files.push(trimmed.to_string());
                }
            }
        }
    }

    // 2. Unified diff format
    for line in patch.lines() {
        let (header, prefix) = if let Some(rest) = line.strip_prefix("+++ ") {
            (rest, "b/")
        } else if let Some(rest) = line.strip_prefix("--- ") {
            (rest, "a/")
        } else {
            continue;
        };
        let p = header.trim().trim_start_matches(prefix).trim();
        if !p.is_empty() && p != "/dev/null" {
            files.push(p.to_string());
        }
    }

    files.sort();
    files.dedup();
    files
}

/// Extracts declared Rust symbols (fn, struct, enum, trait, type, const) directly from patch text.
pub fn extract_symbols_from_patch(patch: &str) -> Vec<String> {
    let mut symbols = Vec::new();

    let extract_ident = |line: &str, keyword: &str| -> Option<String> {
        let idx = line.find(keyword)?;
        let rest = &line[idx + keyword.len()..];
        let ident: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if ident.is_empty() {
            None
        } else {
            Some(ident)
        }
    };

    let keywords = ["fn ", "struct ", "enum ", "trait ", "type ", "const "];

    for line in patch.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("+++") || trimmed.starts_with("---") || trimmed.starts_with("//") {
            continue;
        }
        for kw in keywords {
            if let Some(ident) = extract_ident(trimmed, kw) {
                if !matches!(
                    ident.as_str(),
                    "if" | "while"
                        | "match"
                        | "return"
                        | "true"
                        | "false"
                        | "mut"
                        | "pub"
                        | "ref"
                        | "self"
                        | "Self"
                        | "where"
                        | "for"
                        | "in"
                ) {
                    symbols.push(ident);
                }
            }
        }
    }

    symbols.sort();
    symbols.dedup();
    symbols
}

/// Reads file content at an explicit git revision, falling back to repo_root working tree
/// only when revision is None.
pub fn read_file_content_at_revision(
    repo_root: &Path,
    file_path: &str,
    revision: Option<&str>,
) -> Option<String> {
    if let Some(rev) = revision {
        let trimmed_rev = rev.trim();
        if !trimmed_rev.is_empty() {
            let spec = format!("{trimmed_rev}:{file_path}");
            let out = Command::new("git")
                .env_remove("GIT_INDEX_FILE")
                .args(["show", &spec])
                .current_dir(repo_root)
                .output()
                .ok()?;
            if out.status.success() {
                return Some(String::from_utf8_lossy(&out.stdout).to_string());
            } else {
                return None;
            }
        }
    }
    std::fs::read_to_string(repo_root.join(file_path)).ok()
}

/// Searches the resolved target file content (from explicit revision or working tree)
/// for the exact complete target excerpt.
/// Returns 1-indexed (start_line, end_line) if the entire excerpt matches, or (0, 0) if unmeasured/unavailable.
///
/// NOTE: Partial or first-line matches are strictly disallowed to ensure that citations never
/// point to line ranges whose content diverges from the citation excerpt and cryptographic content hash.
pub fn find_line_range_in_revision(
    repo_root: &Path,
    file_path: &str,
    target_excerpt: &str,
    revision: Option<&str>,
) -> (usize, usize) {
    let trimmed_target = target_excerpt.trim();
    if trimmed_target.is_empty() {
        return (0, 0);
    }

    let Some(content) = read_file_content_at_revision(repo_root, file_path, revision) else {
        return (0, 0);
    };

    let count_line =
        |pos: usize| -> usize { content[..pos].chars().filter(|&c| c == '\n').count() + 1 };

    // Complete exact match of target_excerpt
    if let Some(pos) = content.find(target_excerpt) {
        let start_line = count_line(pos);
        let match_lines = target_excerpt.lines().count().max(1);
        return (start_line, start_line + match_lines - 1);
    }

    // Complete exact match of trimmed_target
    if let Some(pos) = content.find(trimmed_target) {
        let start_line = count_line(pos);
        let match_lines = trimmed_target.lines().count().max(1);
        return (start_line, start_line + match_lines - 1);
    }

    (0, 0)
}

/// Searches the target file in repo_root for complete exact excerpt match.
/// Returns 1-indexed (start_line, end_line) if located, or (0, 0) if unmeasured/unavailable.
pub fn find_line_range_in_file(
    repo_root: &Path,
    file_path: &str,
    target_excerpt: &str,
    _fallback_search: Option<&str>,
) -> (usize, usize) {
    find_line_range_in_revision(repo_root, file_path, target_excerpt, None)
}

/// Scans raw patch content and surrounding repo context for opaque structures.
pub fn scan_patch_for_opaque_structures(
    patch_str: &str,
    repo_root: &Path,
    attempt_id: &str,
    base_commit: Option<&str>,
) -> (Vec<OpaqueStructureFinding>, Vec<GroundedCitation>) {
    let mut findings = Vec::new();
    let mut citations = Vec::new();
    let mut citation_idx = 0;

    let mut make_citation = |file_path: &str,
                             offending_evidence: &str,
                             anchor_search: Option<&str>,
                             before_excerpt: Option<String>,
                             after_excerpt: Option<String>,
                             symbol: Option<String>| {
        citation_idx += 1;
        let anchor = anchor_search.unwrap_or(offending_evidence);
        let (start_line, end_line) =
            find_line_range_in_revision(repo_root, file_path, anchor, base_commit);

        let resolved_before = if start_line > 0 && end_line > 0 {
            read_file_content_at_revision(repo_root, file_path, base_commit)
                .map(|content| {
                    let range_lines: Vec<&str> = content
                        .lines()
                        .skip(start_line - 1)
                        .take(end_line - start_line + 1)
                        .collect();
                    range_lines.join("\n")
                })
                .or(before_excerpt)
        } else {
            before_excerpt
        };

        let exact_excerpt = offending_evidence.to_string();
        let content_hash = compute_sha256(exact_excerpt.as_bytes());
        let before_content_hash = resolved_before
            .as_ref()
            .map(|b| compute_sha256(b.as_bytes()));

        // If base_commit is present, export an immutable snapshot of the file at that revision
        // so file:// links open immutable revision snapshots instead of mutable working copy.
        let target_display_path = if let Some(commit) = base_commit {
            let rel_p = std::path::Path::new(file_path);
            let is_contained = !rel_p.is_absolute()
                && !rel_p.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::Prefix(_)
                    )
                });

            if is_contained {
                let snapshot_dir = repo_root.join(".selfware").join("snapshots").join(commit);
                let snapshot_file = snapshot_dir.join(file_path);
                if !snapshot_file.exists() {
                    let mut cmd = Command::new("git");
                    cmd.env_remove("GIT_INDEX_FILE");
                    cmd.args(["show", &format!("{commit}:{file_path}")]);
                    cmd.current_dir(repo_root);
                    if let Ok(out) = cmd.output() {
                        if out.status.success() {
                            if let Some(parent) = snapshot_file.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            let _ = std::fs::write(&snapshot_file, out.stdout);
                        }
                    }
                }
                if snapshot_file.exists() {
                    snapshot_file
                } else {
                    repo_root.join(file_path)
                }
            } else {
                repo_root.join(file_path)
            }
        } else {
            repo_root.join(file_path)
        };

        let hyperlink = if start_line > 0 && end_line > 0 {
            format!(
                "file://{}#L{start_line}-L{end_line}",
                target_display_path.display()
            )
        } else {
            format!("file://{}", target_display_path.display())
        };

        GroundedCitation {
            citation_id: format!("cite-{attempt_id}-{citation_idx}"),
            file_path: file_path.to_string(),
            line_range: (start_line, end_line),
            symbol,
            content_hash,
            git_commit: base_commit.map(str::to_string),
            attempt_id: Some(attempt_id.to_string()),
            exact_excerpt,
            hyperlink,
            before_excerpt: resolved_before,
            after_excerpt,
            before_content_hash,
        }
    };

    // 1. JSON search-replace format
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch_str) {
        for edit in &edits {
            let file_path = edit
                .get("file")
                .or_else(|| edit.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown_file");
            let replace_text = edit.get("replace").and_then(|v| v.as_str()).unwrap_or("");
            let search_text = edit.get("search").and_then(|v| v.as_str());

            // Finding 0: Blast radius leak into protected paths
            if crate::evolution::is_protected(Path::new(file_path)) {
                let offending = if !replace_text.is_empty() {
                    replace_text
                } else {
                    file_path
                };
                let cite = make_citation(
                    file_path,
                    offending,
                    search_text,
                    search_text.map(str::to_string),
                    Some(replace_text.to_string()),
                    None,
                );
                findings.push(OpaqueStructureFinding {
                    category: OpaqueCategory::BlastRadiusLeak,
                    severity: FindingSeverity::Critical,
                    title: "Protected path targeted by mutation".to_string(),
                    explanation: format!(
                        "Modification targets protected path '{file_path}', violating evolutionary safety invariants."
                    ),
                    citation: cite.clone(),
                    remediation: "Re-target mutation strictly to unconstrained code files.".to_string(),
                });
                citations.push(cite);
            }

            // Finding 0b: Contract breakage via interface erasure
            if let Some(st) = search_text {
                let had_pub = st.contains("pub fn ")
                    || st.contains("pub trait ")
                    || st.contains("pub struct ")
                    || st.contains("pub enum ");
                let has_pub = replace_text.contains("pub fn ")
                    || replace_text.contains("pub trait ")
                    || replace_text.contains("pub struct ")
                    || replace_text.contains("pub enum ");
                if had_pub && !has_pub {
                    let cite = make_citation(
                        file_path,
                        st,
                        Some(st),
                        Some(st.to_string()),
                        Some(replace_text.to_string()),
                        None,
                    );
                    findings.push(OpaqueStructureFinding {
                        category: OpaqueCategory::ContractBreakage,
                        severity: FindingSeverity::Critical,
                        title: "Public API contract erased or converted to private visibility".to_string(),
                        explanation: "Publicly visible symbol in search context was deleted or made private, breaking API contract.".to_string(),
                        citation: cite.clone(),
                        remediation: "Maintain public interface stability and backwards compatibility.".to_string(),
                    });
                    citations.push(cite);
                }
            }

            // Finding 1: Unchecked unwrap in production code
            if !file_path.contains("test")
                && (replace_text.contains(".unwrap()") || replace_text.contains(".expect("))
            {
                let offending_lines: Vec<&str> = replace_text
                    .lines()
                    .map(str::trim)
                    .filter(|l| l.contains(".unwrap()") || l.contains(".expect("))
                    .collect();
                let offending = if offending_lines.is_empty() {
                    replace_text.to_string()
                } else {
                    offending_lines.join("\n")
                };
                let cite = make_citation(
                    file_path,
                    &offending,
                    search_text,
                    search_text.map(str::to_string),
                    Some(replace_text.to_string()),
                    None,
                );
                findings.push(OpaqueStructureFinding {
                    category: OpaqueCategory::UncheckedUnwrap,
                    severity: FindingSeverity::Critical,
                    title: "Unchecked panic vector introduced in production path".to_string(),
                    explanation: "Direct invocation of .unwrap() or .expect() introduces an unhandled panic risk in production code.".to_string(),
                    citation: cite.clone(),
                    remediation: "Propagate typed errors using `Result<T, E>` with `?` operator.".to_string(),
                });
                citations.push(cite);
            }

            // Finding 2: Implicit constant
            for line in replace_text.lines() {
                let trimmed = line.trim();
                if (trimmed.contains(" = ") || trimmed.contains(": "))
                    && (trimmed.contains("1000")
                        || trimmed.contains("5000")
                        || trimmed.contains("0.05")
                        || trimmed.contains("0.1"))
                    && !trimmed.starts_with("const ")
                    && !trimmed.starts_with("//")
                {
                    let cite = make_citation(
                        file_path,
                        trimmed,
                        search_text,
                        search_text.map(str::to_string),
                        Some(replace_text.to_string()),
                        None,
                    );
                    findings.push(OpaqueStructureFinding {
                        category: OpaqueCategory::ImplicitConstant,
                        severity: FindingSeverity::Warning,
                        title: "Implicit numerical constant embedded without named binding".to_string(),
                        explanation: format!("Literal value in '{trimmed}' lacks an explicit named constant or configuration binding."),
                        citation: cite.clone(),
                        remediation: "Define a documented `const` or bind to an evolutionary configuration setting.".to_string(),
                    });
                    citations.push(cite);
                    break;
                }
            }

            // Finding 3: Undocumented public API
            if replace_text.contains("pub fn ") && !replace_text.contains("///") {
                let pub_lines: Vec<&str> = replace_text
                    .lines()
                    .map(str::trim)
                    .filter(|l| l.contains("pub fn "))
                    .collect();
                let offending = if pub_lines.is_empty() {
                    replace_text.to_string()
                } else {
                    pub_lines.join("\n")
                };
                let cite = make_citation(
                    file_path,
                    &offending,
                    search_text,
                    search_text.map(str::to_string),
                    Some(replace_text.to_string()),
                    None,
                );
                findings.push(OpaqueStructureFinding {
                    category: OpaqueCategory::UndocumentedPublicApi,
                    severity: FindingSeverity::Warning,
                    title: "Public function declared without rustdoc contract".to_string(),
                    explanation: "Exported `pub fn` missing `///` documentation specifying invariants, arguments, and error conditions.".to_string(),
                    citation: cite.clone(),
                    remediation: "Add comprehensive `///` docstrings detailing pre-conditions, error variants, and examples.".to_string(),
                });
                citations.push(cite);
            }

            // Finding 4: Silent fallback pattern
            if replace_text.contains("unwrap_or_default()")
                || replace_text.contains(".unwrap_or_else(|_|")
            {
                let fb_lines: Vec<&str> = replace_text
                    .lines()
                    .map(str::trim)
                    .filter(|l| {
                        l.contains("unwrap_or_default()") || l.contains(".unwrap_or_else(|_|")
                    })
                    .collect();
                let offending = if fb_lines.is_empty() {
                    replace_text.to_string()
                } else {
                    fb_lines.join("\n")
                };
                let cite = make_citation(
                    file_path,
                    &offending,
                    search_text,
                    search_text.map(str::to_string),
                    Some(replace_text.to_string()),
                    None,
                );
                findings.push(OpaqueStructureFinding {
                    category: OpaqueCategory::SilentFallback,
                    severity: FindingSeverity::Info,
                    title: "Silent fallback obscures underlying failure mode".to_string(),
                    explanation: "Defaulting silently upon error prevents observability and masks degradation during recursive evolution.".to_string(),
                    citation: cite.clone(),
                    remediation: "Emit structured tracing (`tracing::warn!`) or record a typed failure class.".to_string(),
                });
                citations.push(cite);
            }
        }
    } else {
        // 2. Unified diff format
        let mut current_file = "unknown_file".to_string();
        for line in patch_str.lines() {
            if let Some(stripped) = line.strip_prefix("+++ ") {
                let p = stripped.trim().trim_start_matches("b/").trim();
                current_file = p.to_string();
                if crate::evolution::is_protected(Path::new(&current_file)) {
                    let cite = make_citation(
                        &current_file,
                        &current_file,
                        None,
                        None,
                        Some(current_file.clone()),
                        None,
                    );
                    findings.push(OpaqueStructureFinding {
                        category: OpaqueCategory::BlastRadiusLeak,
                        severity: FindingSeverity::Critical,
                        title: "Protected path targeted in unified diff".to_string(),
                        explanation: format!(
                            "Unified diff modifies protected path '{current_file}', violating evolutionary safety invariants."
                        ),
                        citation: cite.clone(),
                        remediation: "Re-target mutation strictly to unconstrained code files.".to_string(),
                    });
                    citations.push(cite);
                }
            } else if let Some(removed) = line.strip_prefix('-') {
                if !removed.starts_with('-') {
                    let trimmed_rem = removed.trim();
                    if trimmed_rem.starts_with("pub fn ")
                        || trimmed_rem.starts_with("pub trait ")
                        || trimmed_rem.starts_with("pub struct ")
                        || trimmed_rem.starts_with("pub enum ")
                    {
                        let cite = make_citation(
                            &current_file,
                            trimmed_rem,
                            Some(trimmed_rem),
                            Some(trimmed_rem.to_string()),
                            None,
                            None,
                        );
                        findings.push(OpaqueStructureFinding {
                            category: OpaqueCategory::ContractBreakage,
                            severity: FindingSeverity::Critical,
                            title: "Public API definition deleted in unified diff".to_string(),
                            explanation: format!(
                                "Removed public contract symbol definition: '{trimmed_rem}'."
                            ),
                            citation: cite.clone(),
                            remediation: "Preserve exported public interface contracts."
                                .to_string(),
                        });
                        citations.push(cite);
                    }
                }
            } else if let Some(added) = line.strip_prefix('+') {
                if added.starts_with('+') {
                    continue;
                }
                let trimmed = added.trim();

                // Unchecked unwrap in production
                if !current_file.contains("test")
                    && (added.contains(".unwrap()") || added.contains(".expect("))
                {
                    let cite = make_citation(
                        &current_file,
                        trimmed,
                        None,
                        None,
                        Some(trimmed.to_string()),
                        None,
                    );
                    findings.push(OpaqueStructureFinding {
                        category: OpaqueCategory::UncheckedUnwrap,
                        severity: FindingSeverity::Critical,
                        title: "Unchecked panic vector in unified diff".to_string(),
                        explanation: "Added line contains unwrap/expect in production module."
                            .to_string(),
                        citation: cite.clone(),
                        remediation: "Replace with typed error handling.".to_string(),
                    });
                    citations.push(cite);
                }

                // Implicit constant
                if (trimmed.contains(" = ") || trimmed.contains(": "))
                    && (trimmed.contains("1000")
                        || trimmed.contains("5000")
                        || trimmed.contains("0.05")
                        || trimmed.contains("0.1"))
                    && !trimmed.starts_with("const ")
                    && !trimmed.starts_with("//")
                {
                    let cite = make_citation(
                        &current_file,
                        trimmed,
                        None,
                        None,
                        Some(trimmed.to_string()),
                        None,
                    );
                    findings.push(OpaqueStructureFinding {
                        category: OpaqueCategory::ImplicitConstant,
                        severity: FindingSeverity::Warning,
                        title: "Implicit numerical constant embedded in unified diff".to_string(),
                        explanation: format!("Literal value in '{trimmed}' lacks an explicit named constant or configuration binding."),
                        citation: cite.clone(),
                        remediation: "Define a documented `const` or bind to an evolutionary configuration setting.".to_string(),
                    });
                    citations.push(cite);
                }

                // Undocumented public API
                if trimmed.contains("pub fn ") && !trimmed.contains("///") {
                    let cite = make_citation(
                        &current_file,
                        trimmed,
                        None,
                        None,
                        Some(trimmed.to_string()),
                        None,
                    );
                    findings.push(OpaqueStructureFinding {
                        category: OpaqueCategory::UndocumentedPublicApi,
                        severity: FindingSeverity::Warning,
                        title: "Public function declared without rustdoc in unified diff"
                            .to_string(),
                        explanation: "Exported `pub fn` missing `///` documentation.".to_string(),
                        citation: cite.clone(),
                        remediation: "Add comprehensive `///` docstrings.".to_string(),
                    });
                    citations.push(cite);
                }

                // Silent fallback
                if trimmed.contains("unwrap_or_default()")
                    || trimmed.contains(".unwrap_or_else(|_|")
                {
                    let cite = make_citation(
                        &current_file,
                        trimmed,
                        None,
                        None,
                        Some(trimmed.to_string()),
                        None,
                    );
                    findings.push(OpaqueStructureFinding {
                        category: OpaqueCategory::SilentFallback,
                        severity: FindingSeverity::Info,
                        title: "Silent fallback obscures failure mode in unified diff".to_string(),
                        explanation: "Defaulting silently upon error masks degradation."
                            .to_string(),
                        citation: cite.clone(),
                        remediation: "Emit structured tracing or record a typed failure class."
                            .to_string(),
                    });
                    citations.push(cite);
                }
            }
        }
    }

    (findings, citations)
}

/// Validates benchmark report artifact presence, regular file type, non-zero size,
/// valid JSON schema (`sab-report/1`), and candidate metric bindings (binary SHA256 and scores).
/// Resolves relative paths against `repo_root` if provided to prevent CWD dependency.
pub fn validate_benchmark_report_resolved(
    path: &Path,
    repo_root: Option<&Path>,
    node: &AttemptNode,
) -> Result<(), String> {
    let resolved_buf;
    let resolved = if path.is_absolute() {
        path
    } else if let Some(root) = repo_root {
        resolved_buf = root.join(path);
        &resolved_buf
    } else {
        path
    };

    if !resolved.exists() {
        return Err(format!(
            "Benchmark report artifact '{}' does not exist on disk",
            resolved.display()
        ));
    }
    if !resolved.is_file() {
        return Err(format!(
            "Benchmark report artifact '{}' is not a regular file",
            resolved.display()
        ));
    }

    let canonical = resolved.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize benchmark report path '{}': {e}",
            resolved.display()
        )
    })?;

    if let Some(root) = repo_root {
        if let Ok(canonical_root) = root.canonicalize() {
            if !canonical.starts_with(&canonical_root) {
                return Err(format!(
                    "Benchmark report path '{}' escapes repository root '{}' (traversal denied)",
                    resolved.display(),
                    root.display()
                ));
            }
        }
    }
    let metadata = std::fs::metadata(resolved).map_err(|e| {
        format!(
            "Failed to read metadata for benchmark report '{}': {e}",
            resolved.display()
        )
    })?;
    if metadata.len() == 0 {
        return Err(format!(
            "Benchmark report artifact '{}' is empty (0 bytes)",
            resolved.display()
        ));
    }

    let content = std::fs::read_to_string(resolved).map_err(|e| {
        format!(
            "Benchmark report artifact '{}' corrupted or unreadable: {e}",
            resolved.display()
        )
    })?;

    let sab_result = crate::evolution::fitness::parse_sab_report_content(
        &content,
        node.binary_sha256.as_deref(),
    )
    .map_err(|e| match e {
        crate::evolution::fitness::FitnessError::WrongBinaryEvaluated { requested, evaluated } => {
            format!("Benchmark report binary_sha256 mismatch (reported '{evaluated}', expected '{requested}')")
        }
        crate::evolution::fitness::FitnessError::ReportParseFailed(ref msg) if msg.contains("schema mismatch") => {
            format!("Benchmark report artifact '{}' {msg}", resolved.display())
        }
        crate::evolution::fitness::FitnessError::ReportParseFailed(ref msg) if msg.contains("aggregate score mismatch") => {
            format!("Benchmark report {msg}")
        }
        crate::evolution::fitness::FitnessError::ReportParseFailed(ref msg) if msg.contains("corrupted JSON") => {
            format!("Benchmark report artifact '{}' {msg}", resolved.display())
        }
        other => format!("Benchmark report artifact '{}' invalid: {other}", resolved.display()),
    })?;

    // Match reported aggregate metrics against node.metrics
    if let Some(ref expected_metrics) = node.metrics {
        if (sab_result.aggregate_score - expected_metrics.sab_score).abs() > 0.05 {
            return Err(format!(
                "Benchmark report aggregate score mismatch (reported {:.2}, expected {:.2})",
                sab_result.aggregate_score, expected_metrics.sab_score
            ));
        }
    }

    Ok(())
}

/// Backward-compatible wrapper validating benchmark report relative to current working directory.
pub fn validate_benchmark_report(path: &Path, node: &AttemptNode) -> Result<(), String> {
    validate_benchmark_report_resolved(path, None, node)
}

/// Simulates synthetic heuristic scoring modeled as a 10,000-reviewer governance panel
/// across 4 specialized evaluator perspectives.
///
/// NOTE: Governance scores and vote distributions are synthetic heuristic projections
/// derived from automated policy rules and verification gates, not independent human peer review.
pub fn simulate_10000_reviewer_governance(
    node: &AttemptNode,
    findings: &[OpaqueStructureFinding],
    safety: &Degree5Safety,
) -> ReviewerConsensus {
    simulate_10000_reviewer_governance_resolved(node, findings, safety, None)
}

/// Simulates synthetic heuristic scoring modeled as a 10,000-reviewer governance panel
/// across 4 specialized evaluator perspectives, resolving benchmark paths against repo_root.
pub fn simulate_10000_reviewer_governance_resolved(
    node: &AttemptNode,
    findings: &[OpaqueStructureFinding],
    safety: &Degree5Safety,
    repo_root: Option<&Path>,
) -> ReviewerConsensus {
    let has_critical = findings
        .iter()
        .any(|f| f.severity == FindingSeverity::Critical);
    let has_warning = findings
        .iter()
        .any(|f| f.severity == FindingSeverity::Warning);

    let is_verified_success =
        node.is_successful_evaluation() || node.status == AttemptStatus::Baseline;

    // 1. Frontier Safety & Boundary Compliance (3,500 reviewers / 35% weight)
    let safety_veto = !safety.protected_paths_clean
        || safety.has_killswitch_bypass
        || safety.merkle_tree_equality == Some(false)
        || node.status == AttemptStatus::SafetyRejected;

    let (s_app, s_cla, s_qua, s_vet) = if safety_veto {
        (0, 0, 0, 3500)
    } else if !is_verified_success {
        // AGENTS.md Rule 1: CI red means stop. Unverified or failing code cannot be approved by Safety.
        (0, 500, 1000, 2000)
    } else if has_critical {
        (500, 1000, 1500, 500)
    } else if has_warning {
        (2800, 700, 0, 0)
    } else {
        (3500, 0, 0, 0)
    };

    let safety_score = s_app as f64 / 3500.0;
    let safety_assessment = if safety_veto {
        "CRITICAL VETO: Protected path or Merkle tree invariant breach detected.".to_string()
    } else if !is_verified_success {
        format!(
            "VETO/QUARANTINE: Attempt failed verification (status: {:?}).",
            node.status
        )
    } else if has_critical {
        "CONCERN: Critical unhandled panic or contract risk requires quarantine.".to_string()
    } else {
        "PASS: Execution and boundary safety invariants fully preserved.".to_string()
    };

    let p_safety = ReviewerPerspective {
        name: "Frontier Safety & Boundary Compliance".to_string(),
        total_reviewers: 3500,
        votes_approve: s_app,
        votes_clarification: s_cla,
        votes_quarantine: s_qua,
        votes_veto: s_vet,
        perspective_score: safety_score,
        assessment: safety_assessment,
    };

    // 2. Systems & Performance Robustness (2,500 reviewers / 25% weight)
    let (sys_app, sys_cla, sys_qua, sys_vet) = if node.status == AttemptStatus::Timeout {
        (0, 0, 500, 2000)
    } else if node.status == AttemptStatus::BuildFailed
        || node.status == AttemptStatus::InternalError
    {
        (0, 0, 1000, 1500)
    } else if !is_verified_success {
        (0, 500, 1500, 500)
    } else if has_critical {
        (1000, 800, 700, 0)
    } else if has_warning {
        (2100, 400, 0, 0)
    } else {
        (2500, 0, 0, 0)
    };

    let sys_score = sys_app as f64 / 2500.0;
    let sys_assessment = if node.status == AttemptStatus::Timeout {
        "VETO: Mutation exceeded latency threshold or timed out.".to_string()
    } else if node.status == AttemptStatus::BuildFailed {
        "VETO: Release build failed during system compilation.".to_string()
    } else if !is_verified_success {
        "NOTICE: System performance could not be verified due to gate failure.".to_string()
    } else {
        "PASS: Resource bounds and runtime efficiency confirmed.".to_string()
    };

    let p_systems = ReviewerPerspective {
        name: "Systems & Performance Robustness".to_string(),
        total_reviewers: 2500,
        votes_approve: sys_app,
        votes_clarification: sys_cla,
        votes_quarantine: sys_qua,
        votes_veto: sys_vet,
        perspective_score: sys_score,
        assessment: sys_assessment,
    };

    // 3. Software Architecture & Code Quality (2,000 reviewers / 20% weight)
    let swe_failed = matches!(
        node.status,
        AttemptStatus::CompileFailed
            | AttemptStatus::BuildFailed
            | AttemptStatus::TestFailed
            | AttemptStatus::ClippyFailed
            | AttemptStatus::FormatFailed
            | AttemptStatus::PatchFailed
    );

    let (swe_app, swe_cla, swe_qua, swe_vet) = if swe_failed {
        (0, 0, 200, 1800)
    } else if !is_verified_success {
        (0, 500, 500, 1000)
    } else if has_critical {
        (600, 1000, 400, 0)
    } else if has_warning {
        (1600, 400, 0, 0)
    } else {
        (2000, 0, 0, 0)
    };

    let swe_score = swe_app as f64 / 2000.0;
    let swe_assessment = if swe_failed {
        format!(
            "VETO: Regression detected in compilation, lint, or tests ({:?}).",
            node.status
        )
    } else if has_warning {
        "NOTICE: Code quality warnings detected; clarification recommended.".to_string()
    } else if !is_verified_success {
        "QUARANTINE: Code structure unverified due to incomplete evaluation.".to_string()
    } else {
        "PASS: High structural clarity, strong typing, clean formatting.".to_string()
    };

    let p_swe = ReviewerPerspective {
        name: "Software Architecture & Code Quality".to_string(),
        total_reviewers: 2000,
        votes_approve: swe_app,
        votes_clarification: swe_cla,
        votes_quarantine: swe_qua,
        votes_veto: swe_vet,
        perspective_score: swe_score,
        assessment: swe_assessment,
    };

    // 4. Empirical Provenance & Reproducibility (2,000 reviewers / 20% weight)
    let (prov_app, prov_cla, prov_qua, prov_vet, prov_assessment, integrity_veto) = if node
        .diff_sha256
        .len()
        != 64
    {
        (
            0,
            0,
            0,
            2000,
            "VETO: Cryptographic diff hash corrupt or invalid format.".to_string(),
            true,
        )
    } else if node.status == AttemptStatus::InternalError {
        (
            0,
            500,
            1500,
            0,
            "NOTICE: Internal error during evaluation execution trace.".to_string(),
            false,
        )
    } else if !is_verified_success {
        (
            0,
            500,
            1000,
            500,
            "NOTICE: Incomplete execution trace or unverified metrics.".to_string(),
            false,
        )
    } else {
        // Compare cryptographic patch hash against diff_sha256
        let patch_status = match &node.patch {
            Some(p) => {
                let computed = compute_sha256(p.as_bytes());
                if computed == node.diff_sha256 {
                    Ok(())
                } else {
                    Err(format!(
                        "Patch SHA256 mismatch (computed {computed}, expected {})",
                        node.diff_sha256
                    ))
                }
            }
            None => {
                if node.status == AttemptStatus::Baseline {
                    Ok(())
                } else {
                    Err("Patch text missing from attempt node".to_string())
                }
            }
        };

        // Check benchmark artifact presence, structure, and metric bindings
        let benchmark_status = match &node.sab_report_path {
            Some(path) => validate_benchmark_report_resolved(path, repo_root, node),
            None => {
                if node.status == AttemptStatus::Baseline && node.metrics.is_some() {
                    Ok(())
                } else {
                    Err("Benchmark report artifact not recorded on attempt node".to_string())
                }
            }
        };

        match (patch_status, benchmark_status) {
            (Err(err), _) if err.contains("mismatch") || err.contains("corrupted") => (
                0,
                0,
                0,
                2000,
                format!("VETO: Integrity failure — {err}."),
                true,
            ),
            (_, Err(err)) if err.contains("mismatch") || err.contains("corrupted") => (
                0,
                0,
                0,
                2000,
                format!("VETO: Integrity failure — {err}."),
                true,
            ),
            (Ok(()), Ok(())) => (
                2000,
                0,
                0,
                0,
                "PASS: Deterministic provenance, patch hash integrity, and benchmark artifact present and structurally valid."
                    .to_string(),
                false,
            ),
            (patch_res, bench_res) => {
                let mut issues = Vec::new();
                if let Err(e) = patch_res {
                    issues.push(e);
                }
                if let Err(e) = bench_res {
                    issues.push(e);
                }
                (
                    0,
                    1000,
                    1000,
                    0,
                    format!(
                        "UNVERIFIED: Provenance incomplete ({}) — full approval blocked.",
                        issues.join("; ")
                    ),
                    false,
                )
            }
        }
    };

    let prov_score = prov_app as f64 / 2000.0;

    let p_prov = ReviewerPerspective {
        name: "Empirical Provenance & Reproducibility".to_string(),
        total_reviewers: 2000,
        votes_approve: prov_app,
        votes_clarification: prov_cla,
        votes_quarantine: prov_qua,
        votes_veto: prov_vet,
        perspective_score: prov_score,
        assessment: prov_assessment,
    };

    let tot_app = s_app + sys_app + swe_app + prov_app;
    let tot_cla = s_cla + sys_cla + swe_cla + prov_cla;
    let tot_qua = s_qua + sys_qua + swe_qua + prov_qua;
    let tot_vet = s_vet + sys_vet + swe_vet + prov_vet;
    let consensus_score = tot_app as f64 / 10000.0;

    let decision = if safety_veto || integrity_veto || tot_vet >= 2500 || !is_verified_success {
        GovernanceDecision::HardRejectVeto
    } else if consensus_score >= 0.85 {
        GovernanceDecision::ApproveForPromotion
    } else if consensus_score >= 0.65 {
        GovernanceDecision::ConditionalClarification
    } else {
        GovernanceDecision::QuarantineForExperimentation
    };

    let deliberation_summary = if !is_verified_success {
        format!(
            "Synthetic Heuristic Scoring (10,000-Reviewer Projection Model): Verdict 'HARD REJECT & VETO' — Candidate failed validation (status: {:?}); promotion blocked without verified successful benchmark.",
            node.status
        )
    } else if integrity_veto {
        format!(
            "Synthetic Heuristic Scoring (10,000-Reviewer Projection Model): Verdict 'HARD REJECT & VETO' — {} [Automated heuristic scoring projection, not independent human peer review]",
            p_prov.assessment
        )
    } else if safety_veto {
        format!(
            "Synthetic Heuristic Scoring (10,000-Reviewer Projection Model): Verdict 'HARD REJECT & VETO' — {} [Automated heuristic scoring projection, not independent human peer review]",
            p_safety.assessment
        )
    } else {
        format!(
            "Synthetic Heuristic Scoring (10,000-Reviewer Projection Model): Verdict '{}' with {:.1}% approval ({}/10000 approve, {} clarify, {} quarantine, {} veto). [Automated heuristic scoring projection, not independent human peer review]",
            decision,
            consensus_score * 100.0,
            tot_app,
            tot_cla,
            tot_qua,
            tot_vet
        )
    };

    ReviewerConsensus {
        total_reviewers: 10000,
        votes_approve: tot_app,
        votes_clarification: tot_cla,
        votes_quarantine: tot_qua,
        votes_veto: tot_vet,
        consensus_score,
        decision,
        has_safety_veto: safety_veto || integrity_veto,
        perspectives: vec![p_safety, p_systems, p_swe, p_prov],
        deliberation_summary,
    }
}

/// Syntactically searches the repository for call sites or references of the specified symbols
/// in files outside `files_touched`.
pub fn find_affected_callers(
    repo_root: &Path,
    symbols: &[String],
    files_touched: &[String],
) -> Vec<String> {
    if symbols.is_empty() {
        return Vec::new();
    }
    let src_dir = repo_root.join("src");
    if !src_dir.exists() {
        return Vec::new();
    }

    let mut callers = Vec::new();
    let mut stack = vec![src_dir];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let rel = path
                    .strip_prefix(repo_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if files_touched.iter().any(|t| t == &rel) {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for sym in symbols {
                        let pat1 = format!("{sym}(");
                        let pat2 = format!("{sym}::");
                        if content.contains(&pat1) || content.contains(&pat2) {
                            callers.push(format!("{rel} (ref: {sym})"));
                            break;
                        }
                    }
                }
                if callers.len() >= 12 {
                    return callers;
                }
            }
        }
    }

    callers
}

/// Extracts co-occurring domain concepts and architectural tags from touched files, symbols, and descriptions.
pub fn extract_cooccurring_concepts(
    files: &[String],
    symbols: &[String],
    desc: &str,
) -> Vec<String> {
    let mut concepts = std::collections::HashSet::new();
    for f in files {
        let p = Path::new(f);
        for comp in p.components() {
            let s = comp.as_os_str().to_string_lossy();
            if s != "src" && s != "tests" && s != "unit" && !s.ends_with(".rs") {
                concepts.insert(s.to_string());
            }
        }
    }
    for sym in symbols {
        for part in sym.split('_') {
            if part.len() >= 4 {
                concepts.insert(part.to_lowercase());
            }
        }
    }
    for word in desc.split_whitespace() {
        let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
        if clean.len() >= 5 {
            let lower = clean.to_lowercase();
            if matches!(
                lower.as_str(),
                "evolution"
                    | "policy"
                    | "replay"
                    | "benchmark"
                    | "worktree"
                    | "safety"
                    | "citation"
                    | "partition"
                    | "metrics"
                    | "daemon"
            ) {
                concepts.insert(lower);
            }
        }
    }
    let mut list: Vec<String> = concepts.into_iter().collect();
    list.sort();
    list
}

/// Conducts an active investigation of a single evolutionary attempt node.
pub fn investigate_attempt(node: &AttemptNode, repo_root: &Path) -> InvestigativeDossier {
    let patch_str = node.patch.as_deref().unwrap_or("");
    let (findings, citations) = scan_patch_for_opaque_structures(
        patch_str,
        repo_root,
        &node.id,
        node.base_commit.as_deref(),
    );

    // Extract files touched across both JSON search-replace and unified diff formats
    let files_touched = extract_touched_files(patch_str);

    let lines_added = patch_str
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .count();
    let lines_removed = patch_str
        .lines()
        .filter(|l| l.starts_with('-') && !l.starts_with("---"))
        .count();

    // Degree 5: Safety checks (using repo-wide canonical protected path filter)
    let protected_clean = !files_touched
        .iter()
        .any(|f| crate::evolution::is_protected(Path::new(f)));

    let merkle_tree_equality = match (&node.git_tree_id, &node.committed_commit) {
        (Some(eval_tree), Some(commit)) => {
            let commit_tree = Command::new("git")
                .env_remove("GIT_INDEX_FILE")
                .args(["rev-parse", &format!("{commit}^{{tree}}")])
                .current_dir(repo_root)
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                });
            commit_tree.map(|ct| ct == *eval_tree)
        }
        _ => None,
    };

    let rule1_verified = match &node.metrics {
        Some(m) => {
            (node.status == AttemptStatus::Evaluated || node.status == AttemptStatus::Baseline)
                && m.tests_total > 0
                && m.tests_passed == m.tests_total
        }
        None => false,
    };

    let safety = Degree5Safety {
        protected_paths_clean: protected_clean,
        rule1_verified,
        merkle_tree_equality,
        has_killswitch_bypass: false,
    };

    let primary_symbols = extract_symbols_from_patch(patch_str);
    let callers_affected = find_affected_callers(repo_root, &primary_symbols, &files_touched);
    let cooccurring_concepts =
        extract_cooccurring_concepts(&files_touched, &primary_symbols, &node.description);

    let degrees = SixDegreesOfConnection {
        degree_1_intent: Degree1Intent {
            hypothesis_id: node.hypothesis_id.clone(),
            description: node.description.clone(),
            parent_attempt_id: node.parent_id.clone(),
            parent_failure_reason: node.failure_reason.clone(),
            parent_failure_class: node.failure_class,
        },
        degree_2_syntax: Degree2Syntax {
            diff_sha256: node.diff_sha256.clone(),
            files_touched: files_touched.clone(),
            lines_added,
            lines_removed,
            is_json_search_replace: patch_str.starts_with('['),
        },
        degree_3_ontology: Degree3Ontology {
            primary_symbols,
            callers_affected,
            cooccurring_concepts,
        },
        degree_4_empirical: Degree4Empirical {
            status: node.status,
            composite_score: node.composite_score,
            sab_score: node.metrics.as_ref().map(|m| m.sab_score),
            tests_passed: node.metrics.as_ref().map(|m| m.tests_passed),
            tests_total: node.metrics.as_ref().map(|m| m.tests_total),
            wall_time_ms: node.wall_time_ms,
            tokens_used: node.tokens_used,
        },
        degree_5_safety: safety.clone(),
        degree_6_lineage: Degree6Lineage {
            generation: node.generation,
            branch_id: node.branch_id.clone(),
            base_commit: node.base_commit.clone(),
            committed_commit: node.committed_commit.clone(),
            lineage_depth: node.generation,
            promotes_to_main: node.committed_commit.is_some(),
        },
    };

    let consensus =
        simulate_10000_reviewer_governance_resolved(node, &findings, &safety, Some(repo_root));

    InvestigativeDossier {
        attempt_id: node.id.clone(),
        timestamp: node.created_at.clone(),
        degrees,
        findings,
        citations,
        consensus,
    }
}

/// Generates a comprehensive GitHub-flavored Markdown report from an investigative dossier.
pub fn export_markdown(dossier: &InvestigativeDossier) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# RSI Investigative Review Dossier: `{}`\n\n",
        dossier.attempt_id
    ));
    out.push_str(&format!(
        "**Timestamp**: `{}` · **Consensus**: **{}** ({:.1}% Approval)\n\n",
        dossier.timestamp,
        dossier.consensus.decision,
        dossier.consensus.consensus_score * 100.0
    ));
    out.push_str(&format!("> {}\n\n", dossier.consensus.deliberation_summary));

    out.push_str("## 1. The 6 Degrees of Grounded Connection\n\n");
    out.push_str("| Degree | Dimension | Observed Structure & Value |\n");
    out.push_str("| :---: | :--- | :--- |\n");
    out.push_str(&format!(
        "| **1** | **Origin Intent** | Hypothesis `{}`: {} |\n",
        dossier.degrees.degree_1_intent.hypothesis_id, dossier.degrees.degree_1_intent.description
    ));
    out.push_str(&format!(
        "| **2** | **Syntactic Mutation** | Files: `{:?}` (+{} / -{} lines) · SHA256: `{}` |\n",
        dossier.degrees.degree_2_syntax.files_touched,
        dossier.degrees.degree_2_syntax.lines_added,
        dossier.degrees.degree_2_syntax.lines_removed,
        &dossier.degrees.degree_2_syntax.diff_sha256
            [..12.min(dossier.degrees.degree_2_syntax.diff_sha256.len())]
    ));
    let symbols_desc = if dossier.degrees.degree_3_ontology.primary_symbols.is_empty() {
        "None extracted / Unavailable".to_string()
    } else {
        format!("{:?}", dossier.degrees.degree_3_ontology.primary_symbols)
    };
    let callers_desc = if dossier
        .degrees
        .degree_3_ontology
        .callers_affected
        .is_empty()
    {
        "None identified / Unavailable".to_string()
    } else {
        format!("{:?}", dossier.degrees.degree_3_ontology.callers_affected)
    };
    let concepts_desc = if dossier
        .degrees
        .degree_3_ontology
        .cooccurring_concepts
        .is_empty()
    {
        "None detected".to_string()
    } else {
        dossier
            .degrees
            .degree_3_ontology
            .cooccurring_concepts
            .join(", ")
    };
    out.push_str(&format!(
        "| **3** | **Ontological Neighborhood** | Symbols: `{}` · Downstream Callers: `{}` · Concepts: `{}` |\n",
        symbols_desc, callers_desc, concepts_desc
    ));
    out.push_str(&format!(
        "| **4** | **Empirical Verification** | Status: `{:?}` · Score: `{:.4}` · Time: `{}ms` |\n",
        dossier.degrees.degree_4_empirical.status,
        dossier
            .degrees
            .degree_4_empirical
            .composite_score
            .unwrap_or(0.0),
        dossier.degrees.degree_4_empirical.wall_time_ms
    ));
    let merkle_desc = match dossier.degrees.degree_5_safety.merkle_tree_equality {
        Some(true) => "Verified Exact",
        Some(false) => "MISMATCH (Divergent)",
        None => "Unmeasured / Not recorded in attempt node",
    };
    let rule1_desc = if dossier.degrees.degree_4_empirical.status == AttemptStatus::Baseline
        && dossier.degrees.degree_4_empirical.sab_score.is_some()
    {
        "Baseline Benchmark Verified"
    } else if dossier.degrees.degree_5_safety.rule1_verified {
        "Candidate Test Suite Passed"
    } else {
        "Unverified / Failing Gates"
    };
    out.push_str(&format!(
        "| **5** | **Safety & Envelopes** | Protected Paths Clean: `{}` · Rule 1: `{}` · Merkle Tree: `{}` |\n",
        dossier.degrees.degree_5_safety.protected_paths_clean,
        rule1_desc,
        merkle_desc
    ));
    out.push_str(&format!("| **6** | **Lineage & Provenance** | Gen: `{}` · Branch: `{}` · Base: `{:?}` · Promoted: `{:?}` |\n\n",
        dossier.degrees.degree_6_lineage.generation,
        dossier.degrees.degree_6_lineage.branch_id,
        dossier.degrees.degree_6_lineage.base_commit.as_deref().map(|s| &s[..8.min(s.len())]),
        dossier.degrees.degree_6_lineage.committed_commit.as_deref().map(|s| &s[..8.min(s.len())])
    ));

    out.push_str("## 2. Opaque Structure Findings\n\n");
    if dossier.findings.is_empty() {
        out.push_str("No opaque or hazardous structures detected. Mutation adheres strictly to transparent coding contracts.\n\n");
    } else {
        out.push_str("| Severity | Category | Title & Explanation | Remediation |\n");
        out.push_str("| :---: | :--- | :--- | :--- |\n");
        for f in &dossier.findings {
            out.push_str(&format!(
                "| **{}** | `{}` | **{}**<br>{} | {} |\n",
                f.severity, f.category, f.title, f.explanation, f.remediation
            ));
        }
        out.push('\n');
    }

    out.push_str("## 3. Maximum-Power Grounded Citations\n\n");
    if dossier.citations.is_empty() {
        out.push_str("No localized code citations recorded.\n\n");
    } else {
        for c in &dossier.citations {
            let loc_str = if c.line_range == (0, 0) {
                if c.git_commit.is_some() {
                    format!("{}:[line range unmeasured in revision]", c.file_path)
                } else {
                    format!("{}:[line range unavailable in current tree]", c.file_path)
                }
            } else {
                format!("{}:L{}-L{}", c.file_path, c.line_range.0, c.line_range.1)
            };
            out.push_str(&format!(
                "* **Citation `{}`**: [{loc_str}](<{}>)\n",
                c.citation_id, c.hyperlink
            ));
            out.push_str(&format!(
                "  * Offending Mutation SHA256: `{}`\n",
                c.content_hash
            ));
            if let Some(ref before) = c.before_excerpt {
                if let Some(ref b_hash) = c.before_content_hash {
                    out.push_str(&format!("  * Baseline Excerpt SHA256: `{}`\n", b_hash));
                }
                out.push_str(&format!(
                    "  * Before Excerpt (Baseline):\n```rust\n{}\n```\n",
                    before
                ));
            }
            if let Some(ref after) = c.after_excerpt {
                out.push_str(&format!(
                    "  * After Excerpt (Patch):\n```rust\n{}\n```\n",
                    after
                ));
            }
            out.push_str(&format!(
                "  * Offending Evidence Excerpt:\n```rust\n{}\n```\n",
                c.exact_excerpt
            ));
        }
        out.push('\n');
    }

    out.push_str("## 4. Synthetic Heuristic Scoring (10,000-Reviewer Projection Model)\n\n");
    out.push_str("> [!NOTE]\n> Governance scores and vote distributions are synthetic heuristic projections derived from automated policy rules, static analysis, and benchmark telemetry, not independent human peer review.\n\n");
    out.push_str(
        "| Reviewer Perspective | Reviewers | Approve | Clarify | Quarantine | Veto | Score |\n",
    );
    out.push_str("| :--- | :---: | :---: | :---: | :---: | :---: | :---: |\n");
    for p in &dossier.consensus.perspectives {
        out.push_str(&format!(
            "| **{}** | {} | {} | {} | {} | {} | {:.1}% |\n",
            p.name,
            p.total_reviewers,
            p.votes_approve,
            p.votes_clarification,
            p.votes_quarantine,
            p.votes_veto,
            p.perspective_score * 100.0
        ));
    }
    out.push_str(&format!(
        "| **TOTAL CONSENSUS** | **10,000** | **{}** | **{}** | **{}** | **{}** | **{:.1}%** |\n\n",
        dossier.consensus.votes_approve,
        dossier.consensus.votes_clarification,
        dossier.consensus.votes_quarantine,
        dossier.consensus.votes_veto,
        dossier.consensus.consensus_score * 100.0
    ));

    out
}

/// Investigates the latest attempts recorded in an `attempts.jsonl` file.
pub fn investigate_attempts_file(
    attempts_file: &Path,
    repo_root: &Path,
) -> Result<Vec<InvestigativeDossier>> {
    let content = std::fs::read_to_string(attempts_file)?;
    let mut dossiers = Vec::new();
    let mut unparsable_count = 0;
    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<AttemptNode>(trimmed) {
            Ok(node) => {
                dossiers.push(investigate_attempt(&node, repo_root));
            }
            Err(e) => {
                unparsable_count += 1;
                eprintln!(
                    "Warning: skipping unparsable attempt log line {} in {}: {}",
                    line_no + 1,
                    attempts_file.display(),
                    e
                );
            }
        }
    }
    if unparsable_count > 0 {
        eprintln!(
            "Warning: {} unparsable attempt log line(s) skipped in {}",
            unparsable_count,
            attempts_file.display()
        );
    }
    Ok(dossiers)
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/investigate/investigate_test.rs"]
mod tests;
