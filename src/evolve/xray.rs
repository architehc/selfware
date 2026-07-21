//! Concept X-ray: the grounded ontological neighborhood of a selected type.
//!
//! Select a concept (a `struct` / `enum` / `trait` / `type`) and get where it is
//! defined, which traits it implements, who implements it, where it is
//! referenced, and which concepts co-occur with it. Every fact is derived from
//! real source lines — no inferred or hallucinated relationships.
//!
//! The index is built once by scanning the source tree; co-occurrence uses a
//! per-file identifier set (O(source size)) rather than a regex-per-concept
//! sweep, so a concept lookup is fast enough to drive an interactive panel.

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use walkdir::WalkDir;

fn def_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|trait|type)\s+([A-Z][A-Za-z0-9_]*)")
            .expect("valid def regex")
    })
}

fn impl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*impl(?:<[^>]*>)?\s+(?:([A-Z][A-Za-z0-9_]*)(?:<[^>]*>)?\s+for\s+)?([A-Z][A-Za-z0-9_]*)")
            .expect("valid impl regex")
    })
}

fn ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[A-Z][A-Za-z0-9_]*").expect("valid ident regex"))
}

/// One place a concept is declared.
#[derive(Debug, Clone, Serialize)]
pub struct DefinitionSite {
    pub kind: String,
    pub path: String,
    pub line: usize,
}

/// A module that mentions the concept, with a mention count.
#[derive(Debug, Clone, Serialize)]
pub struct ConceptRef {
    pub module: String,
    pub mentions: usize,
}

/// Another concept that co-occurs (appears in the same files).
#[derive(Debug, Clone, Serialize)]
pub struct RelatedConcept {
    pub name: String,
    pub kind: String,
    pub cooccurring_files: usize,
}

/// The full grounded x-ray of one concept.
#[derive(Debug, Clone, Serialize)]
pub struct ConceptXray {
    pub concept: String,
    pub definitions: Vec<DefinitionSite>,
    /// Traits implemented *for* this concept (`impl Trait for Concept`).
    pub implemented_traits: Vec<String>,
    /// Types that implement this concept as a trait (`impl Concept for Type`).
    pub implementors: Vec<String>,
    pub references: Vec<ConceptRef>,
    pub total_mentions: usize,
    pub related: Vec<RelatedConcept>,
    pub footprint_modules: usize,
}

struct FileEntry {
    module: String,
    /// Distinct capitalized identifiers present in the file (for co-occurrence).
    idents: HashSet<String>,
    /// Full text, kept for exact mention counting.
    text: String,
}

/// A prebuilt index over the source tree; answers concept x-ray queries.
pub struct ConceptIndex {
    defs: HashMap<String, Vec<DefinitionSite>>,
    /// (trait_opt, target) for every `impl` line.
    impls: Vec<(Option<String>, String)>,
    files: Vec<FileEntry>,
    concept_names: HashSet<String>,
}

fn module_of(path: &str) -> String {
    let norm = path.replace('\\', "/");
    // Path may be absolute (`/…/src/tools/mod.rs`) or repo-relative (`src/…`).
    // Take the segment after the last `/src/`, else strip a leading `src/`.
    let rel = norm
        .rsplit_once("/src/")
        .map(|(_, tail)| tail)
        .or_else(|| norm.strip_prefix("src/"))
        .unwrap_or(&norm);
    let rel = rel.strip_suffix(".rs").unwrap_or(rel);
    let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty() && *s != "mod").collect();
    if parts.is_empty() {
        "crate".to_string()
    } else {
        parts.join("::")
    }
}

impl ConceptIndex {
    /// An index over no sources — every query returns `None`. Used as a safe
    /// fallback when the source tree cannot be scanned.
    pub fn empty() -> Self {
        Self {
            defs: HashMap::new(),
            impls: Vec::new(),
            files: Vec::new(),
            concept_names: HashSet::new(),
        }
    }

    /// Scan `root` (e.g. `"src"`) and build the concept index.
    pub fn build(root: impl AsRef<Path>) -> Result<Self> {
        let mut defs: HashMap<String, Vec<DefinitionSite>> = HashMap::new();
        let mut impls: Vec<(Option<String>, String)> = Vec::new();
        let mut files: Vec<FileEntry> = Vec::new();

        for entry in WalkDir::new(root.as_ref()).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "rs") {
                let text = match std::fs::read_to_string(p) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let path = p.to_string_lossy().replace('\\', "/");
                let module = module_of(&path);
                for (i, line) in text.lines().enumerate() {
                    if let Some(c) = def_re().captures(line) {
                        defs.entry(c[2].to_string()).or_default().push(DefinitionSite {
                            kind: c[1].to_string(),
                            path: path.clone(),
                            line: i + 1,
                        });
                    }
                    if let Some(c) = impl_re().captures(line) {
                        let tr = c.get(1).map(|m| m.as_str().to_string());
                        impls.push((tr, c[2].to_string()));
                    }
                }
                let idents: HashSet<String> =
                    ident_re().find_iter(&text).map(|m| m.as_str().to_string()).collect();
                files.push(FileEntry { module, idents, text });
            }
        }
        let concept_names: HashSet<String> = defs.keys().cloned().collect();
        Ok(Self { defs, impls, files, concept_names })
    }

    /// Number of distinct concepts in the vocabulary.
    pub fn concept_count(&self) -> usize {
        self.concept_names.len()
    }

    /// Concepts ranked by how many files they appear in (ontology hubs).
    pub fn hubs(&self, limit: usize) -> Vec<RelatedConcept> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for f in &self.files {
            for c in &f.idents {
                if self.concept_names.contains(c) {
                    *counts.entry(c.as_str()).or_default() += 1;
                }
            }
        }
        let mut ranked: Vec<_> = counts.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        ranked.into_iter().take(limit).map(|(name, n)| RelatedConcept {
            name: name.to_string(),
            kind: self.kind_of(name),
            cooccurring_files: n,
        }).collect()
    }

    fn kind_of(&self, name: &str) -> String {
        self.defs.get(name)
            .and_then(|v| v.first())
            .map(|d| d.kind.clone())
            .unwrap_or_default()
    }

    /// Produce the x-ray for `concept`, or `None` if it is not a defined type.
    pub fn xray(&self, concept: &str) -> Option<ConceptXray> {
        let definitions = self.defs.get(concept)?.clone();
        let word = Regex::new(&format!(r"\b{}\b", regex::escape(concept))).ok()?;

        let mut refs: BTreeMap<String, usize> = BTreeMap::new();
        let mut total = 0usize;
        let mut cooccur: HashMap<&str, usize> = HashMap::new();
        for f in &self.files {
            if !f.idents.contains(concept) {
                continue;
            }
            let n = word.find_iter(&f.text).count();
            if n == 0 {
                continue;
            }
            *refs.entry(f.module.clone()).or_default() += n;
            total += n;
            for c in &f.idents {
                if c != concept && self.concept_names.contains(c) {
                    *cooccur.entry(c.as_str()).or_default() += 1;
                }
            }
        }

        let implemented_traits: Vec<String> = self.impls.iter()
            .filter(|(t, target)| target == concept && t.is_some())
            .filter_map(|(t, _)| t.clone())
            .collect();
        let implementors: Vec<String> = self.impls.iter()
            .filter(|(t, _)| t.as_deref() == Some(concept))
            .map(|(_, target)| target.clone())
            .collect::<HashSet<_>>().into_iter().collect();

        let mut references: Vec<ConceptRef> = refs.into_iter()
            .map(|(module, mentions)| ConceptRef { module, mentions }).collect();
        references.sort_by(|a, b| b.mentions.cmp(&a.mentions).then(a.module.cmp(&b.module)));
        let footprint_modules = references.iter()
            .map(|r| r.module.split("::").next().unwrap_or("").to_string())
            .collect::<HashSet<_>>().len();

        let mut related: Vec<RelatedConcept> = cooccur.into_iter().map(|(name, n)| RelatedConcept {
            name: name.to_string(),
            kind: self.kind_of(name),
            cooccurring_files: n,
        }).collect();
        related.sort_by(|a, b| b.cooccurring_files.cmp(&a.cooccurring_files).then(a.name.cmp(&b.name)));

        Some(ConceptXray {
            concept: concept.to_string(),
            definitions,
            implemented_traits,
            implementors,
            references,
            total_mentions: total,
            related,
            footprint_modules,
        })
    }
}
