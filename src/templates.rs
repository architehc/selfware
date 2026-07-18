//! Template engine for scaffolding new projects from embedded templates.
//!
//! Provides compile-time embedded templates for Rust, Python, and Node.js/TypeScript
//! projects, with `{{placeholder}}` variable substitution and optional CI workflow
//! generation.
//!
//! This module is critical for weaker models (4B-9B) that struggle to generate correct
//! project scaffolding from scratch. The templates give the agent a correct starting
//! point so it only needs to fill in the blanks.
//!
//! # Usage
//!
//! ```ignore
//! use selfware::templates::{TemplateEngine, ScaffoldOptions};
//! use std::path::Path;
//!
//! let engine = TemplateEngine::new();
//! let opts = ScaffoldOptions {
//!     description: "A REST API service".into(),
//!     framework: Some("axum".into()),
//!     with_ci: true,
//!     with_tests: true,
//!     qa_profile: "standard".into(),
//! };
//! let files = engine.scaffold_project("rust", "my-api", Path::new("./my-api"), &opts)?;
//! ```

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::interview::InterviewContext;

// ---------------------------------------------------------------------------
// Embedded templates (compile-time via include_str!)
// ---------------------------------------------------------------------------

const RUST_CARGO_TOML: &str = include_str!("../templates/rust/Cargo.toml.template");
const PYTHON_PYPROJECT_TOML: &str = include_str!("../templates/python/pyproject.toml");
const NODEJS_PACKAGE_JSON: &str = include_str!("../templates/nodejs/package.json");
const NODEJS_TSCONFIG_JSON: &str = include_str!("../templates/nodejs/tsconfig.json");
const NODEJS_ESLINT_CONFIG: &str = include_str!("../templates/nodejs/eslint.config.mjs");
const NODEJS_PRETTIERRC: &str = include_str!("../templates/nodejs/.prettierrc");
const NODEJS_VITEST_CONFIG: &str = include_str!("../templates/nodejs/vitest.config.ts");

const WORKFLOW_RUST_QA: &str = include_str!("../templates/workflows/rust-qa.yml");
const WORKFLOW_PYTHON_QA: &str = include_str!("../templates/workflows/python-qa.yml");
const WORKFLOW_NODEJS_QA: &str = include_str!("../templates/workflows/nodejs-qa.yml");

const QA_SCHEMA_YAML: &str = include_str!("../selfware-qa-schema.yaml");

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Options controlling how a project is scaffolded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldOptions {
    /// Short description of the project.
    pub description: String,
    /// Framework / library to wire into the scaffold (e.g. "axum", "FastAPI").
    pub framework: Option<String>,
    /// Include a CI workflow in `.github/workflows/`.
    pub with_ci: bool,
    /// Include test directories and test configuration.
    pub with_tests: bool,
    /// QA profile to use: "standard", "strict", or "minimal".
    pub qa_profile: String,
}

impl Default for ScaffoldOptions {
    fn default() -> Self {
        Self {
            description: String::new(),
            framework: None,
            with_ci: true,
            with_tests: true,
            qa_profile: "standard".into(),
        }
    }
}

/// Metadata about an available template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    /// Language identifier (e.g. "rust", "python", "nodejs").
    pub language: String,
    /// Human-readable description.
    pub description: String,
    /// Files that will be created by this template.
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// QA schema types (parsed from selfware-qa-schema.yaml)
// ---------------------------------------------------------------------------

/// Top-level QA schema configuration parsed from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaConfig {
    pub qa_profile: QaSchemaProfile,
}

/// A single QA profile from the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaProfile {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub stages: Vec<QaSchemaStage>,
    #[serde(default)]
    pub quality_gates: Vec<QaSchemaGate>,
    #[serde(default)]
    pub scoring: Option<QaSchemaScoring>,
    #[serde(default)]
    pub coverage: Option<QaSchemaCoverage>,
    #[serde(default)]
    pub feedback_loops: Option<QaSchemaFeedbackLoops>,
    #[serde(default)]
    pub language_overrides: Option<HashMap<String, serde_yaml::Value>>,
}

/// A QA pipeline stage definition from the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaStage {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub fail_fast: bool,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub coverage_threshold: Option<u64>,
    #[serde(default)]
    pub severity_threshold: Option<String>,
    #[serde(default)]
    pub tools: HashMap<String, Vec<QaSchemaTool>>,
}

fn default_true() -> bool {
    true
}
fn default_timeout() -> u64 {
    60
}

/// A tool command within a QA stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaTool {
    pub command: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Quality gate definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaGate {
    pub stage: String,
    #[serde(default)]
    pub fail_on_error: bool,
    #[serde(default)]
    pub max_warnings: Option<u64>,
    #[serde(default)]
    pub min_coverage: Option<u64>,
    #[serde(default)]
    pub severity_threshold: Option<String>,
    #[serde(default)]
    pub description: String,
}

/// Scoring weights and grade thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaScoring {
    #[serde(default)]
    pub weights: HashMap<String, f64>,
    #[serde(default)]
    pub grade_thresholds: HashMap<String, u64>,
}

/// Coverage configuration from the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaCoverage {
    #[serde(default)]
    pub min_overall: u64,
    #[serde(default)]
    pub min_per_file: u64,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

/// Feedback loop configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSchemaFeedbackLoops {
    #[serde(default)]
    pub auto_fix: Option<serde_yaml::Value>,
    #[serde(default)]
    pub retry_with_context: Option<serde_yaml::Value>,
    #[serde(default)]
    pub escalation: Option<serde_yaml::Value>,
}

// ---------------------------------------------------------------------------
// TemplateEngine
// ---------------------------------------------------------------------------

/// Engine for scaffolding new projects from embedded or runtime-override templates.
pub struct TemplateEngine {
    /// Optional directory for user-level template overrides (e.g. `~/.selfware/templates/`).
    override_dir: Option<PathBuf>,
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateEngine {
    /// Create a new engine. If `~/.selfware/templates/` exists, it will be used
    /// as a runtime override source.
    pub fn new() -> Self {
        let override_dir = dirs::home_dir()
            .map(|h| h.join(".selfware").join("templates"))
            .filter(|p| p.is_dir());
        Self { override_dir }
    }

    /// Create an engine with a specific override directory (useful for testing).
    pub fn with_override_dir(dir: Option<PathBuf>) -> Self {
        Self {
            override_dir: dir.filter(|p| p.is_dir()),
        }
    }

    // -----------------------------------------------------------------------
    // Template rendering
    // -----------------------------------------------------------------------

    /// Replace all `{{placeholder}}` occurrences in `template` with values from `vars`.
    /// Unrecognized placeholders are left as-is.
    pub fn render_template(template: &str, vars: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }

    // -----------------------------------------------------------------------
    // Template listing
    // -----------------------------------------------------------------------

    /// List all available templates with descriptions.
    pub fn available_templates() -> Vec<TemplateInfo> {
        vec![
            TemplateInfo {
                language: "rust".into(),
                description: "Rust project with Cargo.toml, src/main.rs, src/lib.rs, tests/".into(),
                files: vec![
                    "Cargo.toml".into(),
                    "src/main.rs".into(),
                    "src/lib.rs".into(),
                    "tests/integration_test.rs".into(),
                ],
            },
            TemplateInfo {
                language: "python".into(),
                description: "Python project with pyproject.toml, src/<module>/__init__.py, tests/"
                    .into(),
                files: vec![
                    "pyproject.toml".into(),
                    "src/<module>/__init__.py".into(),
                    "src/<module>/cli.py".into(),
                    "tests/__init__.py".into(),
                    "tests/test_main.py".into(),
                ],
            },
            TemplateInfo {
                language: "nodejs".into(),
                description:
                    "Node.js/TypeScript project with package.json, tsconfig, eslint, vitest".into(),
                files: vec![
                    "package.json".into(),
                    "tsconfig.json".into(),
                    "eslint.config.mjs".into(),
                    ".prettierrc".into(),
                    "vitest.config.ts".into(),
                    "src/index.ts".into(),
                    "tests/index.test.ts".into(),
                ],
            },
        ]
    }

    // -----------------------------------------------------------------------
    // Template loading (embedded or runtime override)
    // -----------------------------------------------------------------------

    /// Load a template file, preferring a runtime override if present.
    fn load_template(&self, relative_path: &str, embedded: &str) -> String {
        if let Some(ref dir) = self.override_dir {
            let override_path = dir.join(relative_path);
            if override_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&override_path) {
                    return content;
                }
            }
        }
        embedded.to_string()
    }

    // -----------------------------------------------------------------------
    // Project scaffolding
    // -----------------------------------------------------------------------

    /// Scaffold a new project from templates.
    ///
    /// Returns the list of files created (relative to `project_dir`).
    ///
    /// Refuses to overwrite: if any target file already exists, the whole
    /// scaffold is rejected (nothing is written) and the error lists the
    /// conflicting files. Use [`Self::scaffold_project_force`] to clobber.
    pub fn scaffold_project(
        &self,
        language: &str,
        project_name: &str,
        project_dir: &Path,
        options: &ScaffoldOptions,
    ) -> Result<Vec<String>> {
        self.scaffold_project_inner(language, project_name, project_dir, options, false)
    }

    /// Like [`Self::scaffold_project`], but overwrites existing files.
    ///
    /// This is the explicit opt-in intended for `selfware init --scaffold
    /// --force`: the default path refuses to clobber existing project files
    /// because silently overwriting a user's `Cargo.toml` / `src/main.rs`
    /// is data loss.
    pub fn scaffold_project_force(
        &self,
        language: &str,
        project_name: &str,
        project_dir: &Path,
        options: &ScaffoldOptions,
    ) -> Result<Vec<String>> {
        self.scaffold_project_inner(language, project_name, project_dir, options, true)
    }

    fn scaffold_project_inner(
        &self,
        language: &str,
        project_name: &str,
        project_dir: &Path,
        options: &ScaffoldOptions,
        force: bool,
    ) -> Result<Vec<String>> {
        let lang = language.to_lowercase();
        match lang.as_str() {
            "rust" => self.scaffold_rust(project_name, project_dir, options, force),
            "python" => self.scaffold_python(project_name, project_dir, options, force),
            "nodejs" | "node" | "typescript" | "node.js" | "ts" => {
                self.scaffold_nodejs(project_name, project_dir, options, force)
            }
            other => bail!(
                "Unsupported language '{}'. Supported: rust, python, nodejs",
                other
            ),
        }
    }

    /// Build the standard variable map for template rendering.
    fn build_vars(&self, project_name: &str, options: &ScaffoldOptions) -> HashMap<String, String> {
        let module_name = project_name.replace('-', "_");
        let mut vars = HashMap::new();
        vars.insert("project_name".into(), project_name.into());
        vars.insert("project_description".into(), options.description.clone());
        vars.insert("module_name".into(), module_name);
        vars.insert("repository_url".into(), String::new());
        vars.insert("project_url".into(), String::new());
        vars.insert("docs_url".into(), String::new());
        vars.insert("keywords".into(), String::new());
        vars.insert("categories".into(), String::new());
        vars
    }

    /// Write a file and record its relative path.
    fn write_file(
        project_dir: &Path,
        relative: &str,
        content: &str,
        created: &mut Vec<String>,
    ) -> Result<()> {
        let full = project_dir.join(relative);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
        std::fs::write(&full, content).with_context(|| format!("writing {}", full.display()))?;
        created.push(relative.to_string());
        Ok(())
    }

    /// Write a whole scaffold's `(relative path, content)` pairs, with an
    /// overwrite preflight: unless `force` is set, ANY existing target file
    /// rejects the ENTIRE scaffold before anything is written — a partially
    /// overlapping project must never produce a half-written scaffold, and
    /// silently clobbering an existing `Cargo.toml` or `src/main.rs` is
    /// silent data loss.
    fn write_files(
        project_dir: &Path,
        files: &[(String, String)],
        force: bool,
    ) -> Result<Vec<String>> {
        if !force {
            let existing: Vec<&str> = files
                .iter()
                .map(|(rel, _)| rel.as_str())
                .filter(|rel| project_dir.join(rel).exists())
                .collect();
            if !existing.is_empty() {
                bail!(
                    "refusing to scaffold into '{}': {} file(s) already exist: {}. \
                     Remove them, choose an empty output directory, or re-run with \
                     --force to overwrite.",
                    project_dir.display(),
                    existing.len(),
                    existing.join(", ")
                );
            }
        }
        let mut created = Vec::with_capacity(files.len());
        for (rel, content) in files {
            Self::write_file(project_dir, rel, content, &mut created)?;
        }
        Ok(created)
    }

    // -- Rust ---------------------------------------------------------------

    fn scaffold_rust(
        &self,
        project_name: &str,
        project_dir: &Path,
        options: &ScaffoldOptions,
        force: bool,
    ) -> Result<Vec<String>> {
        let vars = self.build_vars(project_name, options);
        let mut files: Vec<(String, String)> = Vec::new();

        // Cargo.toml
        let cargo_tmpl = self.load_template("rust/Cargo.toml", RUST_CARGO_TOML);
        let cargo_content = Self::render_template(&cargo_tmpl, &vars);
        files.push(("Cargo.toml".into(), cargo_content));

        // src/main.rs
        let main_rs = format!(
            r#"use anyhow::Result;

fn main() -> Result<()> {{
    println!("Hello from {}!");
    Ok(())
}}
"#,
            project_name
        );
        files.push(("src/main.rs".into(), main_rs));

        // src/lib.rs
        let lib_rs = format!(
            r#"//! {} - {}

pub fn greet() -> &'static str {{
    "Hello from {}!"
}}
"#,
            project_name, options.description, project_name
        );
        files.push(("src/lib.rs".into(), lib_rs));

        // tests/
        if options.with_tests {
            let test_rs = format!(
                r#"use {}::greet;

#[test]
fn test_greet() {{
    assert!(greet().contains("{}"));
}}
"#,
                project_name.replace('-', "_"),
                project_name,
            );
            files.push(("tests/integration_test.rs".into(), test_rs));
        }

        // CI workflow
        if options.with_ci {
            let wf = self.load_template("workflows/rust-qa.yml", WORKFLOW_RUST_QA);
            files.push((".github/workflows/rust-qa.yml".into(), wf));
        }

        Self::write_files(project_dir, &files, force)
    }

    // -- Python -------------------------------------------------------------

    fn scaffold_python(
        &self,
        project_name: &str,
        project_dir: &Path,
        options: &ScaffoldOptions,
        force: bool,
    ) -> Result<Vec<String>> {
        let vars = self.build_vars(project_name, options);
        let module_name = project_name.replace('-', "_");
        let mut files: Vec<(String, String)> = Vec::new();

        // pyproject.toml
        let pyproject_tmpl = self.load_template("python/pyproject.toml", PYTHON_PYPROJECT_TOML);
        let pyproject_content = Self::render_template(&pyproject_tmpl, &vars);
        files.push(("pyproject.toml".into(), pyproject_content));

        // src/<module>/__init__.py
        let init_py = format!(
            r#""""{} - {}""""

__version__ = "0.1.0"


def main() -> None:
    """Entry point."""
    print("Hello from {}!")
"#,
            module_name, options.description, project_name
        );
        files.push((format!("src/{}/__init__.py", module_name), init_py));

        // src/<module>/cli.py
        let cli_py = format!(
            r#"""Command-line interface for {}.""

import argparse

from . import main


def cli() -> None:
    """Parse arguments and run."""
    parser = argparse.ArgumentParser(description="{}")
    _ = parser.parse_args()
    main()


if __name__ == "__main__":
    cli()
"#,
            project_name, options.description
        );
        files.push((format!("src/{}/cli.py", module_name), cli_py));

        // tests/
        if options.with_tests {
            files.push(("tests/__init__.py".into(), String::new()));

            let test_py = format!(
                r#"""Tests for {}.""

from {} import main


def test_main(capsys):
    """Test that main runs without error."""
    main()
    captured = capsys.readouterr()
    assert "{}" in captured.out
"#,
                project_name, module_name, project_name
            );
            files.push(("tests/test_main.py".into(), test_py));
        }

        // CI workflow
        if options.with_ci {
            let wf = self.load_template("workflows/python-qa.yml", WORKFLOW_PYTHON_QA);
            files.push((".github/workflows/python-qa.yml".into(), wf));
        }

        Self::write_files(project_dir, &files, force)
    }

    // -- Node.js / TypeScript -----------------------------------------------

    fn scaffold_nodejs(
        &self,
        project_name: &str,
        project_dir: &Path,
        options: &ScaffoldOptions,
        force: bool,
    ) -> Result<Vec<String>> {
        let vars = self.build_vars(project_name, options);
        let mut files: Vec<(String, String)> = Vec::new();

        // package.json
        let pkg_tmpl = self.load_template("nodejs/package.json", NODEJS_PACKAGE_JSON);
        let pkg_content = Self::render_template(&pkg_tmpl, &vars);
        files.push(("package.json".into(), pkg_content));

        // tsconfig.json
        let tsconfig = self.load_template("nodejs/tsconfig.json", NODEJS_TSCONFIG_JSON);
        files.push(("tsconfig.json".into(), tsconfig));

        // eslint.config.mjs
        let eslint = self.load_template("nodejs/eslint.config.mjs", NODEJS_ESLINT_CONFIG);
        files.push(("eslint.config.mjs".into(), eslint));

        // .prettierrc
        let prettier = self.load_template("nodejs/.prettierrc", NODEJS_PRETTIERRC);
        files.push((".prettierrc".into(), prettier));

        // vitest.config.ts
        let vitest = self.load_template("nodejs/vitest.config.ts", NODEJS_VITEST_CONFIG);
        files.push(("vitest.config.ts".into(), vitest));

        // src/index.ts
        let index_ts = format!(
            r#"/**
 * {} - {}
 */

export function greet(): string {{
  return "Hello from {}!";
}}

console.log(greet());
"#,
            project_name, options.description, project_name
        );
        files.push(("src/index.ts".into(), index_ts));

        // tests/
        if options.with_tests {
            let test_ts = format!(
                r#"import {{ describe, it, expect }} from "vitest";
import {{ greet }} from "../src/index";

describe("{}", () => {{
  it("should greet correctly", () => {{
    const result = greet();
    expect(result).toContain("{}");
  }});
}});
"#,
                project_name, project_name,
            );
            files.push(("tests/index.test.ts".into(), test_ts));
        }

        // CI workflow
        if options.with_ci {
            let wf = self.load_template("workflows/nodejs-qa.yml", WORKFLOW_NODEJS_QA);
            files.push((".github/workflows/nodejs-qa.yml".into(), wf));
        }

        Self::write_files(project_dir, &files, force)
    }
}

// ---------------------------------------------------------------------------
// QA schema loading
// ---------------------------------------------------------------------------

/// Load and parse the QA schema configuration.
///
/// If `path` is `Some`, reads from that file on disk; otherwise uses the
/// embedded `selfware-qa-schema.yaml`.
///
/// The YAML file contains multiple documents (separated by `---`). This
/// function parses the first document (the standard profile) by default.
pub fn load_qa_schema(path: Option<&Path>) -> Result<QaSchemaConfig> {
    // Default to loading the "standard" profile (first document).
    load_qa_schema_profile(path, "standard")
}

/// Load a specific QA profile by name from the multi-document schema.
///
/// Iterates over all YAML documents and returns the one matching `profile_name`.
pub fn load_qa_schema_profile(path: Option<&Path>, profile_name: &str) -> Result<QaSchemaConfig> {
    let content = match path {
        Some(p) => std::fs::read_to_string(p)
            .with_context(|| format!("reading QA schema from {}", p.display()))?,
        None => QA_SCHEMA_YAML.to_string(),
    };

    for document in serde_yaml::Deserializer::from_str(&content) {
        if let Ok(config) = QaSchemaConfig::deserialize(document) {
            if config.qa_profile.name == profile_name {
                return Ok(config);
            }
        }
    }

    bail!(
        "QA profile '{}' not found in schema. Available: standard, strict, minimal",
        profile_name
    )
}

/// Convert a [`QaSchemaConfig`] into weights compatible with
/// `QaWeights` from the QA profiles module.
pub fn qa_schema_to_weights(schema: &QaSchemaConfig) -> crate::testing::qa_profiles::QaWeights {
    let defaults = crate::testing::qa_profiles::QaWeights::standard();
    let scoring = match &schema.qa_profile.scoring {
        Some(s) => &s.weights,
        None => return defaults,
    };

    // The schema uses 0.0-1.0 weights; QaWeights uses absolute points
    // that sum to ~80. We scale by 100 to keep proportions.
    let get =
        |key: &str, fallback: f64| -> f64 { scoring.get(key).copied().unwrap_or(fallback) * 100.0 };

    crate::testing::qa_profiles::QaWeights {
        syntax: get("syntax", 0.10),
        format: get("format", 0.05),
        lint: get("lint", 0.15),
        type_check: get("typecheck", 0.10),
        test: get("test", 0.30),
        security: get("security", 0.10),
    }
}

// ---------------------------------------------------------------------------
// Interview integration
// ---------------------------------------------------------------------------

/// Scaffold a project based on answers collected during an interview session.
///
/// Maps [`InterviewContext`] fields to [`ScaffoldOptions`] and invokes the
/// template engine. Honors the interview's output-dir answer (previously
/// silently ignored — everything landed in the caller's cwd). Refuses to
/// overwrite existing files (see [`TemplateEngine::scaffold_project`]).
pub fn scaffold_from_context(ctx: &InterviewContext, project_dir: &Path) -> Result<Vec<String>> {
    let language = ctx.language.as_deref().unwrap_or("rust").to_lowercase();

    // Normalise interview language strings to template identifiers.
    let lang_key = if language.contains("typescript") || language.contains("node") {
        "nodejs"
    } else if language.contains("python") {
        "python"
    } else if language.contains("rust") {
        "rust"
    } else {
        // Best-effort: try as-is (will fail gracefully for unsupported langs).
        &language
    };

    let project_dir = resolve_output_dir(ctx, project_dir)?;

    // Derive a project name from the output directory or task.
    let project_name = project_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("my-project")
        .to_string();

    let qa_profile = match ctx.testing_preference {
        Some(crate::interview::TestingPreference::Tdd) => "strict",
        Some(crate::interview::TestingPreference::Minimal) => "minimal",
        Some(crate::interview::TestingPreference::None) => "minimal",
        _ => "standard",
    };

    let with_tests = !matches!(
        ctx.testing_preference,
        Some(crate::interview::TestingPreference::None)
    );

    let description = if ctx.task.is_empty() {
        ctx.extra_notes.first().cloned().unwrap_or_default()
    } else {
        ctx.task.clone()
    };

    let options = ScaffoldOptions {
        description,
        framework: ctx.framework.clone(),
        with_ci: true,
        with_tests,
        qa_profile: qa_profile.into(),
    };

    let engine = TemplateEngine::new();
    engine.scaffold_project(lang_key, &project_name, &project_dir, &options)
}

/// Resolve the directory to scaffold into from the interview's output-dir
/// answer. `None` / `"."` means the directory the caller passed; `"<temp>"`
/// means a fresh throwaway directory under the system temp dir; anything
/// else is a NEW subdirectory of `project_dir` with the given name. The
/// answer is a directory NAME, not a path — separators and `..` are
/// rejected so a freeform answer can't escape the chosen root.
fn resolve_output_dir(ctx: &InterviewContext, project_dir: &Path) -> Result<PathBuf> {
    let Some(answer) = ctx.output_dir.as_deref().map(str::trim) else {
        return Ok(project_dir.to_path_buf());
    };
    match answer {
        "" | "." => Ok(project_dir.to_path_buf()),
        "<temp>" => {
            let unique = format!(
                "selfware-scaffold-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            );
            Ok(std::env::temp_dir().join(unique))
        }
        name => {
            if name.contains(['/', '\\']) || name.contains("..") {
                bail!(
                    "invalid output directory name '{}': expected a plain directory name, not a path",
                    name
                );
            }
            Ok(project_dir.join(name))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    // -- Template rendering -------------------------------------------------

    #[test]
    fn test_render_template_basic() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "hello-world".into());
        vars.insert("desc".into(), "A test project".into());

        let result = TemplateEngine::render_template("name={{name}}, desc={{desc}}", &vars);
        assert_eq!(result, "name=hello-world, desc=A test project");
    }

    #[test]
    fn test_render_template_missing_placeholder_kept() {
        let vars = HashMap::new();
        let result = TemplateEngine::render_template("{{unknown}}", &vars);
        assert_eq!(result, "{{unknown}}");
    }

    #[test]
    fn test_render_template_multiple_occurrences() {
        let mut vars = HashMap::new();
        vars.insert("x".into(), "42".into());
        let result = TemplateEngine::render_template("a={{x}} b={{x}}", &vars);
        assert_eq!(result, "a=42 b=42");
    }

    #[test]
    fn test_render_template_empty_value() {
        let mut vars = HashMap::new();
        vars.insert("project_name".into(), "".into());
        let result = TemplateEngine::render_template("name={{project_name}}", &vars);
        assert_eq!(result, "name=");
    }

    #[test]
    fn test_render_template_no_placeholders() {
        let vars = HashMap::new();
        let result = TemplateEngine::render_template("plain text", &vars);
        assert_eq!(result, "plain text");
    }

    // -- Rust scaffolding ---------------------------------------------------

    #[test]
    fn test_scaffold_rust_project() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            description: "Test Rust project".into(),
            framework: None,
            with_ci: true,
            with_tests: true,
            qa_profile: "standard".into(),
        };

        let files = engine
            .scaffold_project("rust", "my-app", dir.path(), &opts)
            .unwrap();

        // Verify Cargo.toml was created with correct name
        let cargo_path = dir.path().join("Cargo.toml");
        assert!(cargo_path.exists(), "Cargo.toml should exist");
        let cargo_content = std::fs::read_to_string(&cargo_path).unwrap();
        assert!(
            cargo_content.contains("name = \"my-app\""),
            "Cargo.toml should contain project name"
        );
        assert!(
            cargo_content.contains("Test Rust project"),
            "Cargo.toml should contain description"
        );

        // Verify src/main.rs
        assert!(dir.path().join("src/main.rs").exists());
        let main_content = std::fs::read_to_string(dir.path().join("src/main.rs")).unwrap();
        assert!(main_content.contains("my-app"));

        // Verify src/lib.rs
        assert!(dir.path().join("src/lib.rs").exists());

        // Verify tests/
        assert!(dir.path().join("tests/integration_test.rs").exists());

        // Verify CI workflow
        assert!(dir.path().join(".github/workflows/rust-qa.yml").exists());

        // All expected files present
        assert!(files.contains(&"Cargo.toml".to_string()));
        assert!(files.contains(&"src/main.rs".to_string()));
        assert!(files.contains(&"src/lib.rs".to_string()));
        assert!(files.contains(&"tests/integration_test.rs".to_string()));
        assert!(files.contains(&".github/workflows/rust-qa.yml".to_string()));
    }

    // -- Python scaffolding -------------------------------------------------

    #[test]
    fn test_scaffold_python_project() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            description: "Test Python project".into(),
            framework: None,
            with_ci: true,
            with_tests: true,
            qa_profile: "standard".into(),
        };

        let files = engine
            .scaffold_project("python", "my-api", dir.path(), &opts)
            .unwrap();

        // Verify pyproject.toml
        let pyproject_path = dir.path().join("pyproject.toml");
        assert!(pyproject_path.exists(), "pyproject.toml should exist");
        let pyproject_content = std::fs::read_to_string(&pyproject_path).unwrap();
        assert!(
            pyproject_content.contains("name = \"my-api\""),
            "pyproject.toml should contain project name"
        );

        // Verify module directory
        assert!(dir.path().join("src/my_api/__init__.py").exists());
        assert!(dir.path().join("src/my_api/cli.py").exists());

        // Verify tests
        assert!(dir.path().join("tests/__init__.py").exists());
        assert!(dir.path().join("tests/test_main.py").exists());

        // Verify CI
        assert!(dir.path().join(".github/workflows/python-qa.yml").exists());

        assert!(files.contains(&"pyproject.toml".to_string()));
        assert!(files.contains(&"src/my_api/__init__.py".to_string()));
    }

    // -- Node.js scaffolding ------------------------------------------------

    #[test]
    fn test_scaffold_nodejs_project() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            description: "Test Node project".into(),
            framework: None,
            with_ci: true,
            with_tests: true,
            qa_profile: "standard".into(),
        };

        let files = engine
            .scaffold_project("nodejs", "my-service", dir.path(), &opts)
            .unwrap();

        // Verify all 5 config files
        assert!(dir.path().join("package.json").exists());
        assert!(dir.path().join("tsconfig.json").exists());
        assert!(dir.path().join("eslint.config.mjs").exists());
        assert!(dir.path().join(".prettierrc").exists());
        assert!(dir.path().join("vitest.config.ts").exists());

        // Verify source and test
        assert!(dir.path().join("src/index.ts").exists());
        assert!(dir.path().join("tests/index.test.ts").exists());

        // Verify package.json has correct name
        let pkg_content = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(pkg_content.contains("\"name\": \"my-service\""));

        // CI
        assert!(dir.path().join(".github/workflows/nodejs-qa.yml").exists());

        // Check all 5 config files are in the list
        assert!(files.contains(&"package.json".to_string()));
        assert!(files.contains(&"tsconfig.json".to_string()));
        assert!(files.contains(&"eslint.config.mjs".to_string()));
        assert!(files.contains(&".prettierrc".to_string()));
        assert!(files.contains(&"vitest.config.ts".to_string()));
    }

    #[test]
    fn test_scaffold_nodejs_aliases() {
        // All aliases should work
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions::default();

        for alias in &["nodejs", "node", "typescript", "node.js", "ts"] {
            let sub = dir.path().join(alias);
            std::fs::create_dir_all(&sub).unwrap();
            let result = engine.scaffold_project(alias, "test", &sub, &opts);
            assert!(
                result.is_ok(),
                "alias '{}' should succeed: {:?}",
                alias,
                result.err()
            );
        }
    }

    // -- QA schema ----------------------------------------------------------

    #[test]
    fn test_load_qa_schema_embedded() {
        let config = load_qa_schema(None).unwrap();
        assert_eq!(config.qa_profile.name, "standard");
        assert!(!config.qa_profile.stages.is_empty());
        assert!(!config.qa_profile.quality_gates.is_empty());
    }

    #[test]
    fn test_load_qa_schema_from_disk() {
        let dir = TempDir::new().unwrap();
        let schema_path = dir.path().join("qa-schema.yaml");
        std::fs::write(&schema_path, QA_SCHEMA_YAML).unwrap();

        let config = load_qa_schema_profile(Some(&schema_path), "standard").unwrap();
        assert_eq!(config.qa_profile.name, "standard");
    }

    #[test]
    fn test_load_qa_schema_profile_standard() {
        let config = load_qa_schema_profile(None, "standard").unwrap();
        assert_eq!(config.qa_profile.name, "standard");
    }

    #[test]
    fn test_load_qa_schema_profile_strict() {
        let config = load_qa_schema_profile(None, "strict").unwrap();
        assert_eq!(config.qa_profile.name, "strict");
    }

    #[test]
    fn test_load_qa_schema_profile_minimal() {
        let config = load_qa_schema_profile(None, "minimal").unwrap();
        assert_eq!(config.qa_profile.name, "minimal");
    }

    #[test]
    fn test_load_qa_schema_profile_unknown() {
        let result = load_qa_schema_profile(None, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_qa_schema_to_weights() {
        let config = load_qa_schema(None).unwrap();
        let weights = qa_schema_to_weights(&config);
        // syntax weight = 0.10 * 100 = 10.0
        assert!((weights.syntax - 10.0).abs() < 0.01);
        // test weight = 0.30 * 100 = 30.0
        assert!((weights.test - 30.0).abs() < 0.01);
        assert!(weights.total() > 0.0);
    }

    // -- Embedded template validity -----------------------------------------

    #[test]
    fn test_embedded_rust_cargo_toml_is_valid_toml() {
        // Render with dummy vars first so placeholders don't break parsing
        let mut vars = HashMap::new();
        vars.insert("project_name".into(), "test_project".into());
        vars.insert("module_name".into(), "test_project".into());
        vars.insert("project_description".into(), "test".into());
        vars.insert("repository_url".into(), "".into());
        vars.insert("project_url".into(), "".into());
        vars.insert("keywords".into(), "test".into());
        vars.insert("categories".into(), "test".into());

        let rendered = TemplateEngine::render_template(RUST_CARGO_TOML, &vars);
        let parsed: Result<toml::Value, _> = toml::from_str(&rendered);
        assert!(
            parsed.is_ok(),
            "Rendered Cargo.toml should be valid TOML: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn test_embedded_nodejs_package_json_is_valid_json() {
        let mut vars = HashMap::new();
        vars.insert("project_name".into(), "test-project".into());
        vars.insert("project_description".into(), "test".into());
        vars.insert("repository_url".into(), "".into());
        vars.insert("project_url".into(), "".into());
        vars.insert("keywords".into(), "test".into());

        let rendered = TemplateEngine::render_template(NODEJS_PACKAGE_JSON, &vars);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&rendered);
        assert!(
            parsed.is_ok(),
            "Rendered package.json should be valid JSON: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn test_embedded_prettierrc_is_valid_json() {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(NODEJS_PRETTIERRC);
        assert!(
            parsed.is_ok(),
            ".prettierrc should be valid JSON: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn test_embedded_tsconfig_is_parseable() {
        // tsconfig.json uses JSON-with-comments (JSONC) which TypeScript supports
        // but serde_json does not. Verify it at least contains the expected keys.
        assert!(NODEJS_TSCONFIG_JSON.contains("compilerOptions"));
        assert!(NODEJS_TSCONFIG_JSON.contains("\"strict\": true"));
        assert!(NODEJS_TSCONFIG_JSON.contains("\"outDir\""));
    }

    #[test]
    fn test_embedded_qa_schema_is_valid_yaml() {
        // The schema is a multi-document YAML. Verify each document parses
        // individually via the iterator API.
        let mut count = 0;
        for document in serde_yaml::Deserializer::from_str(QA_SCHEMA_YAML) {
            let val = serde_yaml::Value::deserialize(document);
            assert!(
                val.is_ok(),
                "YAML document {} should parse: {:?}",
                count,
                val.err()
            );
            count += 1;
        }
        assert!(
            count >= 3,
            "Should have at least 3 YAML documents (standard, strict, minimal)"
        );
    }

    // -- CI workflow generation ---------------------------------------------

    #[test]
    fn test_ci_workflow_generation_rust() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            with_ci: true,
            ..Default::default()
        };

        let files = engine
            .scaffold_project("rust", "ci-test", dir.path(), &opts)
            .unwrap();
        assert!(files.contains(&".github/workflows/rust-qa.yml".to_string()));

        let wf_content =
            std::fs::read_to_string(dir.path().join(".github/workflows/rust-qa.yml")).unwrap();
        assert!(wf_content.contains("cargo check"));
        assert!(wf_content.contains("cargo clippy"));
    }

    #[test]
    fn test_ci_workflow_generation_python() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            with_ci: true,
            ..Default::default()
        };

        let files = engine
            .scaffold_project("python", "ci-test", dir.path(), &opts)
            .unwrap();
        assert!(files.contains(&".github/workflows/python-qa.yml".to_string()));
    }

    #[test]
    fn test_ci_workflow_generation_nodejs() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            with_ci: true,
            ..Default::default()
        };

        let files = engine
            .scaffold_project("nodejs", "ci-test", dir.path(), &opts)
            .unwrap();
        assert!(files.contains(&".github/workflows/nodejs-qa.yml".to_string()));
    }

    #[test]
    fn test_no_ci_when_disabled() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            with_ci: false,
            ..Default::default()
        };

        let files = engine
            .scaffold_project("rust", "no-ci", dir.path(), &opts)
            .unwrap();
        assert!(!files.iter().any(|f| f.contains(".github")));
    }

    #[test]
    fn test_no_tests_when_disabled() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions {
            with_tests: false,
            ..Default::default()
        };

        let files = engine
            .scaffold_project("rust", "no-tests", dir.path(), &opts)
            .unwrap();
        assert!(!files.iter().any(|f| f.contains("tests/")));
    }

    // -- Interview integration ----------------------------------------------

    #[test]
    fn test_scaffold_from_context_rust() {
        use crate::interview::{InterviewContext, ProjectType, TestingPreference};

        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("my-rust-app");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ctx = InterviewContext {
            language: Some("Rust".into()),
            framework: Some("axum".into()),
            project_type: Some(ProjectType::WebApi),
            testing_preference: Some(TestingPreference::TestsAfter),
            output_dir: None,
            scope: None,
            extra_notes: vec![],
            task: "Build a REST API".into(),
        };

        let files = scaffold_from_context(&ctx, &project_dir).unwrap();
        assert!(files.contains(&"Cargo.toml".to_string()));
        assert!(files.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_scaffold_from_context_python() {
        use crate::interview::{InterviewContext, TestingPreference};

        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("my-python-app");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ctx = InterviewContext {
            language: Some("Python".into()),
            framework: None,
            project_type: None,
            testing_preference: Some(TestingPreference::Tdd),
            output_dir: None,
            scope: None,
            extra_notes: vec![],
            task: "A Python service".into(),
        };

        let files = scaffold_from_context(&ctx, &project_dir).unwrap();
        assert!(files.contains(&"pyproject.toml".to_string()));
    }

    #[test]
    fn test_scaffold_from_context_no_tests() {
        use crate::interview::{InterviewContext, TestingPreference};

        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("no-tests-app");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ctx = InterviewContext {
            language: Some("Rust".into()),
            framework: None,
            project_type: None,
            testing_preference: Some(TestingPreference::None),
            output_dir: None,
            scope: None,
            extra_notes: vec![],
            task: "Quick script".into(),
        };

        let files = scaffold_from_context(&ctx, &project_dir).unwrap();
        assert!(!files.iter().any(|f| f.contains("tests/")));
    }

    #[test]
    fn scaffold_from_context_writes_rust_project() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = crate::interview::InterviewContext {
            language: Some("rust".into()),
            framework: None,
            project_type: None,
            testing_preference: Some(crate::interview::TestingPreference::Tdd),
            output_dir: None,
            scope: None,
            extra_notes: vec![],
            task: "test scaffold".into(),
        };
        let files = scaffold_from_context(&ctx, dir.path()).unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.ends_with("Cargo.toml")));
    }

    // -- Overwrite refusal ---------------------------------------------------

    #[test]
    fn test_scaffold_refuses_to_overwrite_existing_files() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("app");
        std::fs::create_dir_all(&project_dir).unwrap();
        // Pre-existing user file that a scaffold would silently clobber.
        std::fs::write(project_dir.join("Cargo.toml"), "[package]\nname = \"mine\"\n").unwrap();

        let engine = TemplateEngine::new();
        let opts = ScaffoldOptions::default();
        let result = engine.scaffold_project("rust", "app", &project_dir, &opts);

        let err = result
            .err()
            .expect("scaffolding over existing files must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("Cargo.toml"),
            "error must list the conflicting file(s): {}",
            msg
        );
        assert!(
            msg.contains("already exist") || msg.contains("refusing"),
            "error must explain the refusal: {}",
            msg
        );
        // Nothing else may be written: no half-scaffold left behind.
        assert!(
            !project_dir.join("src/main.rs").exists(),
            "no files should be written when the scaffold is refused"
        );
        // The existing file must be untouched.
        let content = std::fs::read_to_string(project_dir.join("Cargo.toml")).unwrap();
        assert!(content.contains("name = \"mine\""));
    }

    #[test]
    fn test_scaffold_force_overwrites_existing_files() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("app");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("Cargo.toml"), "[package]\nname = \"mine\"\n").unwrap();

        let engine = TemplateEngine::new();
        let opts = ScaffoldOptions::default();
        let files = engine
            .scaffold_project_force("rust", "app", &project_dir, &opts)
            .expect("force scaffold should succeed");
        assert!(files.contains(&"Cargo.toml".to_string()));
        // The forced scaffold actually replaced the file.
        let content = std::fs::read_to_string(project_dir.join("Cargo.toml")).unwrap();
        assert!(
            content.contains("name = \"app\""),
            "force should clobber, got: {}",
            content
        );
    }

    #[test]
    fn test_scaffold_from_context_honors_output_dir_subdirectory() {
        use crate::interview::InterviewContext;

        let dir = TempDir::new().unwrap();
        let ctx = InterviewContext {
            language: Some("Rust".into()),
            framework: None,
            project_type: None,
            testing_preference: None,
            output_dir: Some("my-new-app".into()),
            scope: None,
            extra_notes: vec![],
            task: "test scaffold".into(),
        };

        let files = scaffold_from_context(&ctx, dir.path()).unwrap();
        assert!(files.contains(&"Cargo.toml".to_string()));
        // Files must land in the chosen subdirectory, NOT the caller's dir.
        assert!(
            dir.path().join("my-new-app/Cargo.toml").exists(),
            "scaffold should honor the interview's output-dir answer"
        );
        assert!(
            !dir.path().join("Cargo.toml").exists(),
            "nothing should be written into the caller's directory"
        );
    }

    #[test]
    fn test_resolve_output_dir_rejects_path_traversal() {
        use crate::interview::InterviewContext;

        let base = std::path::Path::new("/tmp/scaffold-root");
        let ctx = |dir: &str| InterviewContext {
            language: None,
            framework: None,
            project_type: None,
            testing_preference: None,
            output_dir: Some(dir.to_string()),
            scope: None,
            extra_notes: vec![],
            task: String::new(),
        };

        for bad in ["../escape", "/abs/path", "a/b", "..", "x\\y"] {
            assert!(
                resolve_output_dir(&ctx(bad), base).is_err(),
                "'{}' must be rejected as an output-dir answer",
                bad
            );
        }
        // Plain names and the current-dir answers are accepted.
        assert_eq!(
            resolve_output_dir(&ctx("my-app"), base).unwrap(),
            base.join("my-app")
        );
        assert_eq!(resolve_output_dir(&ctx("."), base).unwrap(), base);
    }

    // -- Available templates ------------------------------------------------

    #[test]
    fn test_available_templates() {
        let templates = TemplateEngine::available_templates();
        assert_eq!(templates.len(), 3);
        assert!(templates.iter().any(|t| t.language == "rust"));
        assert!(templates.iter().any(|t| t.language == "python"));
        assert!(templates.iter().any(|t| t.language == "nodejs"));
    }

    // -- Unsupported language -----------------------------------------------

    #[test]
    fn test_scaffold_unsupported_language() {
        let dir = TempDir::new().unwrap();
        let engine = TemplateEngine::with_override_dir(None);
        let opts = ScaffoldOptions::default();

        let result = engine.scaffold_project("haskell", "test", dir.path(), &opts);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unsupported language"));
    }

    // -- Runtime override ---------------------------------------------------

    #[test]
    fn test_runtime_override() {
        let dir = TempDir::new().unwrap();
        let override_dir = dir.path().join("overrides");
        let rust_dir = override_dir.join("rust");
        std::fs::create_dir_all(&rust_dir).unwrap();

        // Write a custom Cargo.toml override
        std::fs::write(
            rust_dir.join("Cargo.toml"),
            "[package]\nname = \"{{project_name}}\"\nversion = \"99.0.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let engine = TemplateEngine::with_override_dir(Some(override_dir));
        let opts = ScaffoldOptions::default();
        let project_dir = dir.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let _files = engine
            .scaffold_project("rust", "overridden", &project_dir, &opts)
            .unwrap();

        let cargo_content = std::fs::read_to_string(project_dir.join("Cargo.toml")).unwrap();
        assert!(
            cargo_content.contains("99.0.0"),
            "Should use the overridden template"
        );
    }
}
