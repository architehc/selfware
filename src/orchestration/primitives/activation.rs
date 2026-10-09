use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::config::SafetyConfig;
use crate::evolve::{Graph, GraphIndex, Node, NodeLayer};

use super::types::{
    valid_focus_symbol, validate_relative_artifact_path, ActivationPlan, AllowedRoute, GraphStatus,
    PrimitiveAssignment, ReadAttestationSummary, RepositoryBinding,
};

const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_GRAPH_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct PreparedArtifact {
    pub key: String,
    pub path: String,
    pub content: String,
    pub base_sha256: String,
    pub focus_symbol: Option<String>,
}

pub(crate) struct PreparedActivation {
    pub root: PathBuf,
    pub plan: ActivationPlan,
    pub artifacts: BTreeMap<String, PreparedArtifact>,
}

#[derive(Debug, Clone)]
struct CandidateDraft {
    path: String,
    graph_node_id: Option<String>,
    classification: String,
    focus_symbol: Option<String>,
    reasons: Vec<String>,
}

struct AdvisoryGraph {
    status: GraphStatus,
    digest: Option<String>,
    index: Option<Arc<GraphIndex>>,
}

pub(crate) async fn build_activation_plan(
    task: &str,
    requested_root: &Path,
    safety: &SafetyConfig,
    max_rounds: u8,
    max_artifacts: usize,
) -> Result<PreparedActivation> {
    anyhow::ensure!(!task.trim().is_empty(), "artifact harness task is empty");
    anyhow::ensure!(
        (1..=super::types::MAX_ROUNDS).contains(&max_rounds),
        "artifact harness rounds must be between 1 and {}",
        super::types::MAX_ROUNDS
    );
    anyhow::ensure!(
        (1..=super::types::MAX_ARTIFACTS).contains(&max_artifacts),
        "artifact harness max_artifacts must be between 1 and {}",
        super::types::MAX_ARTIFACTS
    );

    let root = resolve_root(requested_root, safety)?;
    let graph = load_advisory_graph(&root, safety).await;
    let census = localized_census(task, &root, safety).await?;
    let ordered = candidate_order(
        task,
        &root,
        &census.candidates,
        &census.eligible_relative_paths,
        graph.index.as_deref(),
        max_artifacts,
    );

    let mut warnings = Vec::new();
    if census.truncated {
        push_warning(
            &mut warnings,
            format!(
                "activation census stopped at a configured resource ceiling after admitting {} file(s) and reading {} bytes; graph suggestions remain confined to the admitted census",
                census.files.len(), census.scanned_bytes
            ),
        );
    }
    if census.walk_errors > 0 || census.read_errors > 0 {
        push_warning(
            &mut warnings,
            format!(
                "activation census was incomplete after {} walk error(s) and {} checked-read error(s); omitted source is not covered by this run",
                census.walk_errors, census.read_errors
            ),
        );
    }
    if census.ranking_truncated {
        push_warning(
            &mut warnings,
            "activation ranking reached its measured token ceiling; selected artifacts cover only completely ranked census entries".to_string(),
        );
    }
    let mut prepared = Vec::new();
    for candidate in ordered {
        if prepared.len() >= max_artifacts {
            break;
        }
        if let Err(error) = validate_relative_artifact_path(&candidate.path) {
            push_warning(
                &mut warnings,
                format!("skipped {}: {error}", candidate.path),
            );
            continue;
        }
        let read = super::safe_read::read_relative_regular(
            &root,
            Path::new(&candidate.path),
            safety,
            MAX_ARTIFACT_BYTES,
        )
        .await;
        let (content, bytes) = match read {
            Ok(value) => value,
            Err(error) => {
                push_warning(
                    &mut warnings,
                    format!("skipped {}: {}", candidate.path, safe_error(&error, &root)),
                );
                continue;
            }
        };
        if std::str::from_utf8(&bytes).is_err() {
            push_warning(
                &mut warnings,
                format!("skipped {}: source is not valid UTF-8", candidate.path),
            );
            continue;
        }
        let base_sha256 = sha256(&bytes);
        prepared.push((candidate, content, base_sha256));
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    let mut assignments = Vec::with_capacity(prepared.len());
    let mut artifacts = BTreeMap::new();
    for (position, (candidate, content, base_sha256)) in prepared.into_iter().enumerate() {
        let key = format!("artifact-{:03}", position + 1);
        let assignment = PrimitiveAssignment {
            key: key.clone(),
            graph_node_id: candidate.graph_node_id,
            path: candidate.path.clone(),
            classification: candidate.classification,
            base_sha256: base_sha256.clone(),
            focus_symbol: candidate.focus_symbol.clone(),
            reasons: candidate.reasons,
            read_attestation: ReadAttestationSummary {
                attestation_id: uuid::Uuid::new_v4().to_string(),
                exact_snapshot_read: true,
                write: false,
                execute: false,
                network: false,
            },
        };
        artifacts.insert(
            key.clone(),
            PreparedArtifact {
                key,
                path: candidate.path,
                content,
                base_sha256,
                focus_symbol: candidate.focus_symbol,
            },
        );
        assignments.push(assignment);
    }

    let allowed_routes = trusted_routes_for_assignments(&assignments, max_rounds);
    let working_set_sha256 = working_set_digest(&assignments);
    let repository_id = sha256(root.to_string_lossy().as_bytes());
    let task_sha256 = sha256(task.as_bytes());
    let activation_census_complete = census.census_complete();
    let selection_mode = match &graph.status {
        GraphStatus::Loaded { .. } => "trusted_census_plus_frozen_graph",
        GraphStatus::Missing => "trusted_census_graph_missing",
        GraphStatus::Rejected { .. } => "trusted_census_graph_rejected",
    };
    let mut plan = ActivationPlan {
        run_id,
        task_sha256,
        selection_mode: selection_mode.into(),
        max_rounds,
        graph: graph.status,
        binding: RepositoryBinding {
            repository_id,
            head_tree: git_head_tree(&root),
            graph_sha256: graph.digest,
            activation_census_sha256: census.manifest_sha256,
            activation_census_complete,
            activation_ranking_complete: !census.ranking_truncated,
            working_set_sha256,
            plan_sha256: "0".repeat(64),
        },
        assignments,
        allowed_routes,
        warnings,
    };
    plan.binding.plan_sha256 = plan_digest(&plan)?;
    plan.validate()?;

    Ok(PreparedActivation {
        root,
        plan,
        artifacts,
    })
}

fn resolve_root(requested: &Path, safety: &SafetyConfig) -> Result<PathBuf> {
    let anchored = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        crate::tools::workspace_root::current_path().join(requested)
    };
    reject_private_root(&anchored, "requested")?;
    let raw = anchored.to_string_lossy();
    crate::tools::file::validate_tool_path(&raw, safety)
        .with_context(|| format!("artifact harness root is outside path policy: {raw}"))?;
    let metadata = std::fs::symlink_metadata(&anchored).with_context(|| {
        format!(
            "cannot inspect artifact harness root: {}",
            anchored.display()
        )
    })?;
    anyhow::ensure!(
        metadata.is_dir(),
        "artifact harness root is not a directory"
    );
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "artifact harness root may not be a symlink"
    );
    let canonical = std::fs::canonicalize(&anchored).with_context(|| {
        format!(
            "cannot resolve artifact harness root: {}",
            anchored.display()
        )
    })?;
    reject_private_root(&canonical, "canonical")?;
    crate::tools::file::validate_tool_path(&canonical.to_string_lossy(), safety)
        .context("canonical artifact harness root is outside path policy")?;
    Ok(canonical)
}

fn reject_private_root(root: &Path, form: &str) -> Result<()> {
    for component in root.components() {
        let std::path::Component::Normal(name) = component else {
            continue;
        };
        let lowered = name.to_string_lossy().to_ascii_lowercase();
        anyhow::ensure!(
            !crate::safety::source_context::is_private_discovery_dir_name(name),
            "artifact harness {form} root contains forbidden private-state component '{lowered}'"
        );
    }
    Ok(())
}

async fn localized_census(
    task: &str,
    root: &Path,
    safety: &SafetyConfig,
) -> Result<crate::tools::localize_issue::LocalizationCensus> {
    let task = task.to_string();
    let root_string = root.to_string_lossy().into_owned();
    let safety = safety.clone();
    crate::tools::workspace_root::spawn_blocking(move || {
        crate::tools::localize_issue::localize_issue_with_census_sync(
            &task,
            &root_string,
            Some(&safety),
        )
    })
    .await
    .context("artifact localization task panicked")?
}

async fn recomputed_localization_census(
    root: &Path,
    safety: &SafetyConfig,
) -> Result<crate::tools::localize_issue::LocalizationCensus> {
    let root_string = root.to_string_lossy().into_owned();
    let safety = safety.clone();
    crate::tools::workspace_root::spawn_blocking(move || {
        crate::tools::localize_issue::recompute_localization_census_sync(
            &root_string,
            Some(&safety),
        )
    })
    .await
    .context("artifact localization census task panicked")?
}

async fn load_advisory_graph(root: &Path, safety: &SafetyConfig) -> AdvisoryGraph {
    let path = crate::evolve::graph_cache::graph_path(root);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return AdvisoryGraph {
                status: GraphStatus::Missing,
                digest: None,
                index: None,
            };
        }
        Err(error) => {
            return AdvisoryGraph {
                status: GraphStatus::Rejected {
                    reason: format!("graph metadata unavailable: {error}"),
                    sha256: None,
                },
                digest: None,
                index: None,
            };
        }
        Ok(_) => {}
    }
    let bytes = match super::safe_read::read_relative_regular(
        root,
        Path::new(".selfware/evolve-graph.yaml"),
        safety,
        MAX_GRAPH_BYTES,
    )
    .await
    {
        Ok((_text, bytes)) => bytes,
        Err(error) => {
            return AdvisoryGraph {
                status: GraphStatus::Rejected {
                    reason: format!("graph read rejected: {}", short_error(&error)),
                    sha256: None,
                },
                digest: None,
                index: None,
            };
        }
    };
    let digest = sha256(&bytes);
    let graph: Graph = match serde_yaml::from_slice(&bytes) {
        Ok(graph) => graph,
        Err(error) => {
            return AdvisoryGraph {
                status: GraphStatus::Rejected {
                    reason: format!("graph parse failed: {error}"),
                    sha256: Some(digest.clone()),
                },
                digest: Some(digest),
                index: None,
            };
        }
    };
    if let Err(reason) = validate_graph_input(&graph) {
        return AdvisoryGraph {
            status: GraphStatus::Rejected {
                reason,
                sha256: Some(digest.clone()),
            },
            digest: Some(digest),
            index: None,
        };
    }
    let index = Arc::new(GraphIndex::from_graph(Arc::new(graph), &digest));
    AdvisoryGraph {
        status: GraphStatus::Loaded {
            revision: index.revision.clone(),
            sha256: digest.clone(),
        },
        digest: Some(digest),
        index: Some(index),
    }
}

fn validate_graph_input(graph: &Graph) -> Result<(), String> {
    let mut ids = HashSet::new();
    for node in &graph.nodes {
        if !safe_graph_label(&node.id) {
            return Err("graph contains an unsafe node label".into());
        }
        if !ids.insert(node.id.as_str()) {
            return Err(format!("duplicate graph node id: {}", node.id));
        }
        if node.path.as_deref().is_some_and(|path| {
            crate::safety::source_context::path_contains_private_discovery_state(Path::new(path))
        }) {
            return Err("graph contains private coding-agent state".into());
        }
    }
    for edge in &graph.edges {
        if !safe_graph_label(&edge.from) || !safe_graph_label(&edge.to) {
            return Err("graph contains an unsafe edge label".into());
        }
    }
    Ok(())
}

fn safe_graph_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 1024
        && !label.chars().any(|character| {
            character.is_control() || crate::evolve::context_trust::is_hidden_unicode(character)
        })
        && !label.split(['/', '\\', ':']).any(|part| {
            crate::safety::source_context::is_private_tool_state_dir_name(std::ffi::OsStr::new(
                part,
            ))
        })
}

fn candidate_order(
    task: &str,
    root: &Path,
    localized: &[crate::tools::localize_issue::Candidate],
    eligible_relative_paths: &BTreeSet<String>,
    graph: Option<&GraphIndex>,
    max_artifacts: usize,
) -> Vec<CandidateDraft> {
    let mut seeds = Vec::new();
    for candidate in localized {
        let Some(path) = normalize_candidate_path(root, &candidate.file) else {
            continue;
        };
        if validate_relative_artifact_path(&path).is_err() {
            continue;
        }
        if !eligible_relative_paths.contains(&path) {
            continue;
        }
        let node = graph.and_then(|index| file_node_for_path(index, &path));
        let classification = classification_from_path(&path);
        seeds.push(CandidateDraft {
            path,
            graph_node_id: node.map(|node| node.id.clone()),
            classification,
            focus_symbol: valid_focus_symbol(&candidate.function)
                .then(|| candidate.function.clone()),
            reasons: vec![candidate.reason.clone()],
        });
    }

    let mut ordered = Vec::new();
    let mut seen = BTreeSet::new();
    let seed_quota = max_artifacts.div_ceil(2).min(seeds.len());
    for seed in seeds.iter().take(seed_quota) {
        push_candidate(&mut ordered, &mut seen, seed.clone());
    }
    if let Some(index) = graph {
        let primary: Vec<String> = ordered
            .iter()
            .filter_map(|draft| draft.graph_node_id.clone())
            .collect();
        for node_id in primary {
            let mut neighbors = Vec::new();
            neighbors.extend(index.tests_for(&node_id));
            neighbors.extend(index.dependencies(&node_id).iter().cloned());
            neighbors.extend(index.dependents(&node_id).iter().cloned());
            neighbors.sort();
            neighbors.dedup();
            for neighbor in neighbors {
                let Some(node) = owning_file_node(index, &neighbor) else {
                    continue;
                };
                let Some(path) = node.path.as_deref().and_then(normalize_graph_path) else {
                    continue;
                };
                if !eligible_relative_paths.contains(&path) {
                    continue;
                }
                let classification = classification_from_path(&path);
                push_candidate(
                    &mut ordered,
                    &mut seen,
                    CandidateDraft {
                        path,
                        graph_node_id: Some(node.id.clone()),
                        classification,
                        focus_symbol: None,
                        reasons: vec![format!("graph neighbor of {node_id}")],
                    },
                );
            }
        }
    }
    for seed in seeds.into_iter().skip(seed_quota) {
        push_candidate(&mut ordered, &mut seen, seed);
    }

    if let Some(index) = graph {
        let terms = crate::evolve::graph_index::split_terms(task);
        let mut lexical: Vec<(usize, &Node)> = index
            .graph
            .nodes
            .iter()
            .filter(|node| node.layer != NodeLayer::Symbol && node.path.is_some())
            .filter_map(|node| {
                let hits = crate::evolve::graph_index::lexical_hits(node, &terms);
                (hits > 0).then_some((hits, node))
            })
            .collect();
        lexical.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.id.cmp(&right.1.id))
        });
        for (hits, node) in lexical {
            let Some(path) = node.path.as_deref().and_then(normalize_graph_path) else {
                continue;
            };
            if !eligible_relative_paths.contains(&path) {
                continue;
            }
            let classification = classification_from_path(&path);
            push_candidate(
                &mut ordered,
                &mut seen,
                CandidateDraft {
                    path,
                    graph_node_id: Some(node.id.clone()),
                    classification,
                    focus_symbol: None,
                    reasons: vec![format!("{hits} graph lexical match(es)")],
                },
            );
        }
    }
    ordered.truncate(max_artifacts.saturating_mul(8));
    ordered
}

fn push_candidate(
    ordered: &mut Vec<CandidateDraft>,
    seen: &mut BTreeSet<String>,
    candidate: CandidateDraft,
) {
    if seen.insert(candidate.path.clone()) {
        ordered.push(candidate);
    }
}

fn normalize_candidate_path(root: &Path, candidate: &str) -> Option<String> {
    let path = Path::new(candidate);
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?
    } else {
        path
    };
    normalize_graph_path(&relative.to_string_lossy())
}

fn normalize_graph_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    validate_relative_artifact_path(&normalized).ok()?;
    Some(normalized)
}

fn file_node_for_path<'a>(index: &'a GraphIndex, path: &str) -> Option<&'a Node> {
    index
        .graph
        .nodes
        .iter()
        .filter(|node| node.layer != NodeLayer::Symbol && node.path.as_deref() == Some(path))
        .min_by(|left, right| left.id.cmp(&right.id))
}

fn owning_file_node<'a>(index: &'a GraphIndex, id: &str) -> Option<&'a Node> {
    let node = index.node(id)?;
    if node.layer == NodeLayer::Symbol {
        node.parent_id
            .as_deref()
            .and_then(|parent| index.node(parent))
    } else {
        Some(node)
    }
}

fn trusted_routes_for_assignments(
    assignments: &[PrimitiveAssignment],
    max_rounds: u8,
) -> BTreeSet<AllowedRoute> {
    if max_rounds <= 1 {
        return BTreeSet::new();
    }

    let mut routes = BTreeSet::new();
    for source in assignments {
        for destination in assignments {
            if source.key == destination.key {
                continue;
            }
            routes.insert(AllowedRoute {
                from: source.key.clone(),
                to: destination.key.clone(),
            });
        }
    }
    routes
}

fn classification_from_path(path: &str) -> String {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust_source",
        "py" => "python_source",
        "js" | "mjs" | "cjs" => "javascript_source",
        "ts" | "tsx" => "typescript_source",
        "go" => "go_source",
        _ => "source",
    }
    .to_string()
}

fn working_set_digest(assignments: &[PrimitiveAssignment]) -> String {
    let mut hasher = Sha256::new();
    for assignment in assignments {
        hasher.update(assignment.path.as_bytes());
        hasher.update([0]);
        hasher.update(assignment.base_sha256.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn plan_digest(plan: &ActivationPlan) -> Result<String> {
    #[derive(serde::Serialize)]
    struct DigestView<'a> {
        run_id: &'a str,
        task_sha256: &'a str,
        selection_mode: &'a str,
        max_rounds: u8,
        repository_id: &'a str,
        head_tree: &'a Option<String>,
        graph_sha256: &'a Option<String>,
        activation_census_sha256: &'a str,
        activation_census_complete: bool,
        activation_ranking_complete: bool,
        working_set_sha256: &'a str,
        assignments: &'a [PrimitiveAssignment],
        allowed_routes: &'a BTreeSet<AllowedRoute>,
    }
    let bytes = serde_json::to_vec(&DigestView {
        run_id: &plan.run_id,
        task_sha256: &plan.task_sha256,
        selection_mode: &plan.selection_mode,
        max_rounds: plan.max_rounds,
        repository_id: &plan.binding.repository_id,
        head_tree: &plan.binding.head_tree,
        graph_sha256: &plan.binding.graph_sha256,
        activation_census_sha256: &plan.binding.activation_census_sha256,
        activation_census_complete: plan.binding.activation_census_complete,
        activation_ranking_complete: plan.binding.activation_ranking_complete,
        working_set_sha256: &plan.binding.working_set_sha256,
        assignments: &plan.assignments,
        allowed_routes: &plan.allowed_routes,
    })?;
    Ok(sha256(&bytes))
}

fn git_head_tree(root: &Path) -> Option<String> {
    let repository = git2::Repository::discover(root).ok()?;
    let tree = repository.head().ok()?.peel_to_tree().ok()?;
    Some(tree.id().to_string())
}

pub(crate) async fn binding_changes(
    prepared: &PreparedActivation,
    safety: &SafetyConfig,
) -> Vec<String> {
    let mut changes = Vec::new();
    if git_head_tree(&prepared.root) != prepared.plan.binding.head_tree {
        changes.push("Git HEAD tree changed".to_string());
    }
    if current_graph_digest(&prepared.root, safety).await != prepared.plan.binding.graph_sha256 {
        changes.push("advisory graph bytes changed".to_string());
    }
    match recomputed_localization_census(&prepared.root, safety).await {
        Ok(census) if census.manifest_sha256 == prepared.plan.binding.activation_census_sha256 => {}
        Ok(_) => changes.push("activation census changed".to_string()),
        Err(error) => changes.push(format!(
            "activation census is no longer reproducible: {}",
            safe_error(&error, &prepared.root)
        )),
    }
    for assignment in &prepared.plan.assignments {
        let current = super::safe_read::read_relative_regular(
            &prepared.root,
            Path::new(&assignment.path),
            safety,
            MAX_ARTIFACT_BYTES,
        )
        .await;
        match current {
            Ok((_text, bytes)) if sha256(&bytes) == assignment.base_sha256 => {}
            Ok(_) => changes.push(format!("{} content changed", assignment.path)),
            Err(error) => changes.push(format!(
                "{} is no longer readable: {}",
                assignment.path,
                short_error(&error)
            )),
        }
    }
    changes
}

async fn current_graph_digest(root: &Path, safety: &SafetyConfig) -> Option<String> {
    let path = crate::evolve::graph_cache::graph_path(root);
    if std::fs::symlink_metadata(&path).is_err() {
        return None;
    }
    super::safe_read::read_relative_regular(
        root,
        Path::new(".selfware/evolve-graph.yaml"),
        safety,
        MAX_GRAPH_BYTES,
    )
    .await
    .ok()
    .map(|(_text, bytes)| sha256(&bytes))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn safe_error(error: &anyhow::Error, root: &Path) -> String {
    let redacted = crate::observability::telemetry::redact_secrets(&error.to_string());
    let local = redacted.replace(&root.to_string_lossy().to_string(), ".");
    display_safe(&local).chars().take(300).collect()
}

fn short_error(error: &anyhow::Error) -> String {
    display_safe(&crate::observability::telemetry::redact_secrets(
        &error.to_string(),
    ))
    .chars()
    .take(300)
    .collect()
}

fn display_safe(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn push_warning(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < 20 {
        warnings.push(warning);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration/primitives/activation_test.rs"]
mod tests;
