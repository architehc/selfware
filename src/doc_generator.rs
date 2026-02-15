//! Project Documentation Generator
//!
//! Auto-generate documentation from code, maintain synchronization between
//! docs and code, create Architecture Decision Records (ADRs), and generate
//! changelogs from git history.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic counter for generating unique IDs
static DOC_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
static ADR_COUNTER: AtomicU64 = AtomicU64::new(0);
static CHANGELOG_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique document ID
fn generate_doc_id() -> String {
    format!("doc-{}", DOC_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate a unique ADR ID
fn generate_adr_id() -> String {
    format!("adr-{}", ADR_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate a unique changelog ID
fn generate_changelog_id() -> String {
    format!(
        "changelog-{}",
        CHANGELOG_COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

/// Documentation type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocType {
    /// API documentation
    Api,
    /// Architecture documentation
    Architecture,
    /// User guide
    UserGuide,
    /// Developer guide
    DeveloperGuide,
    /// Tutorial
    Tutorial,
    /// Reference
    Reference,
    /// Architecture Decision Record
    Adr,
    /// Changelog
    Changelog,
    /// Readme
    Readme,
}

impl std::fmt::Display for DocType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocType::Api => write!(f, "API"),
            DocType::Architecture => write!(f, "Architecture"),
            DocType::UserGuide => write!(f, "User Guide"),
            DocType::DeveloperGuide => write!(f, "Developer Guide"),
            DocType::Tutorial => write!(f, "Tutorial"),
            DocType::Reference => write!(f, "Reference"),
            DocType::Adr => write!(f, "ADR"),
            DocType::Changelog => write!(f, "Changelog"),
            DocType::Readme => write!(f, "README"),
        }
    }
}

/// Documentation format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocFormat {
    /// Markdown format
    Markdown,
    /// HTML format
    Html,
    /// ReStructuredText
    Rst,
    /// AsciiDoc
    AsciiDoc,
    /// Plain text
    PlainText,
}

impl DocFormat {
    /// Get file extension for format
    pub fn extension(&self) -> &str {
        match self {
            DocFormat::Markdown => "md",
            DocFormat::Html => "html",
            DocFormat::Rst => "rst",
            DocFormat::AsciiDoc => "adoc",
            DocFormat::PlainText => "txt",
        }
    }
}

/// Represents an extracted documentation item from code
#[derive(Debug, Clone)]
pub struct DocItem {
    /// Unique identifier
    pub id: String,
    /// Item name
    pub name: String,
    /// Documentation type
    pub doc_type: DocType,
    /// Source file path
    pub source_path: Option<PathBuf>,
    /// Line number in source
    pub line_number: Option<usize>,
    /// Brief description
    pub brief: String,
    /// Detailed description
    pub description: String,
    /// Examples
    pub examples: Vec<String>,
    /// Related items
    pub see_also: Vec<String>,
    /// Tags/categories
    pub tags: Vec<String>,
    /// Parameters (for functions)
    pub parameters: Vec<ParameterDoc>,
    /// Return value documentation
    pub returns: Option<String>,
    /// Errors that can be returned
    pub errors: Vec<String>,
    /// Deprecation notice
    pub deprecated: Option<String>,
    /// Since version
    pub since: Option<String>,
}

impl DocItem {
    /// Create a new documentation item
    pub fn new(name: impl Into<String>, doc_type: DocType) -> Self {
        Self {
            id: generate_doc_id(),
            name: name.into(),
            doc_type,
            source_path: None,
            line_number: None,
            brief: String::new(),
            description: String::new(),
            examples: Vec::new(),
            see_also: Vec::new(),
            tags: Vec::new(),
            parameters: Vec::new(),
            returns: None,
            errors: Vec::new(),
            deprecated: None,
            since: None,
        }
    }

    /// Set brief description
    pub fn with_brief(mut self, brief: impl Into<String>) -> Self {
        self.brief = brief.into();
        self
    }

    /// Set detailed description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set source location
    pub fn with_source(mut self, path: PathBuf, line: usize) -> Self {
        self.source_path = Some(path);
        self.line_number = Some(line);
        self
    }

    /// Add an example
    pub fn with_example(mut self, example: impl Into<String>) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Add a parameter
    pub fn with_parameter(mut self, param: ParameterDoc) -> Self {
        self.parameters.push(param);
        self
    }

    /// Set return documentation
    pub fn with_returns(mut self, returns: impl Into<String>) -> Self {
        self.returns = Some(returns.into());
        self
    }

    /// Render to markdown
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // Title
        output.push_str(&format!("## {}\n\n", self.name));

        // Deprecation warning
        if let Some(ref deprecated) = self.deprecated {
            output.push_str(&format!("> **Deprecated**: {}\n\n", deprecated));
        }

        // Brief
        if !self.brief.is_empty() {
            output.push_str(&format!("{}\n\n", self.brief));
        }

        // Description
        if !self.description.is_empty() {
            output.push_str(&format!("{}\n\n", self.description));
        }

        // Parameters
        if !self.parameters.is_empty() {
            output.push_str("### Parameters\n\n");
            for param in &self.parameters {
                output.push_str(&format!(
                    "- `{}` ({}) - {}\n",
                    param.name, param.param_type, param.description
                ));
            }
            output.push('\n');
        }

        // Returns
        if let Some(ref returns) = self.returns {
            output.push_str(&format!("### Returns\n\n{}\n\n", returns));
        }

        // Errors
        if !self.errors.is_empty() {
            output.push_str("### Errors\n\n");
            for error in &self.errors {
                output.push_str(&format!("- {}\n", error));
            }
            output.push('\n');
        }

        // Examples
        if !self.examples.is_empty() {
            output.push_str("### Examples\n\n");
            for example in &self.examples {
                output.push_str(&format!("```rust\n{}\n```\n\n", example));
            }
        }

        // See also
        if !self.see_also.is_empty() {
            output.push_str("### See Also\n\n");
            for item in &self.see_also {
                output.push_str(&format!("- {}\n", item));
            }
            output.push('\n');
        }

        // Source location
        if let (Some(path), Some(line)) = (&self.source_path, self.line_number) {
            output.push_str(&format!(
                "*Defined in [{}:{}]({}#L{})*\n",
                path.display(),
                line,
                path.display(),
                line
            ));
        }

        output
    }
}

/// Parameter documentation
#[derive(Debug, Clone)]
pub struct ParameterDoc {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: String,
    /// Description
    pub description: String,
    /// Default value
    pub default: Option<String>,
    /// Whether parameter is required
    pub required: bool,
}

impl ParameterDoc {
    /// Create a new parameter doc
    pub fn new(name: impl Into<String>, param_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param_type: param_type.into(),
            description: String::new(),
            default: None,
            required: true,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set default value
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self.required = false;
        self
    }
}

/// Architecture Decision Record
#[derive(Debug, Clone)]
pub struct Adr {
    /// Unique identifier
    pub id: String,
    /// ADR number
    pub number: u32,
    /// Title
    pub title: String,
    /// Date created
    pub date: String,
    /// Status
    pub status: AdrStatus,
    /// Context explaining the issue
    pub context: String,
    /// Decision made
    pub decision: String,
    /// Consequences of the decision
    pub consequences: Vec<String>,
    /// Alternatives considered
    pub alternatives: Vec<AdrAlternative>,
    /// Related ADRs
    pub related: Vec<u32>,
    /// Tags
    pub tags: Vec<String>,
    /// Author
    pub author: Option<String>,
}

impl Adr {
    /// Create a new ADR
    pub fn new(number: u32, title: impl Into<String>) -> Self {
        Self {
            id: generate_adr_id(),
            number,
            title: title.into(),
            date: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            status: AdrStatus::Proposed,
            context: String::new(),
            decision: String::new(),
            consequences: Vec::new(),
            alternatives: Vec::new(),
            related: Vec::new(),
            tags: Vec::new(),
            author: None,
        }
    }

    /// Set context
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = context.into();
        self
    }

    /// Set decision
    pub fn with_decision(mut self, decision: impl Into<String>) -> Self {
        self.decision = decision.into();
        self
    }

    /// Add consequence
    pub fn with_consequence(mut self, consequence: impl Into<String>) -> Self {
        self.consequences.push(consequence.into());
        self
    }

    /// Add alternative
    pub fn with_alternative(mut self, alt: AdrAlternative) -> Self {
        self.alternatives.push(alt);
        self
    }

    /// Set status
    pub fn with_status(mut self, status: AdrStatus) -> Self {
        self.status = status;
        self
    }

    /// Generate filename
    pub fn filename(&self) -> String {
        let slug = self
            .title
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();
        format!("{:04}-{}.md", self.number, slug)
    }

    /// Render to markdown
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("# ADR-{:04}: {}\n\n", self.number, self.title));

        output.push_str(&format!("**Date**: {}\n\n", self.date));
        output.push_str(&format!("**Status**: {}\n\n", self.status));

        if let Some(ref author) = self.author {
            output.push_str(&format!("**Author**: {}\n\n", author));
        }

        if !self.tags.is_empty() {
            output.push_str(&format!("**Tags**: {}\n\n", self.tags.join(", ")));
        }

        output.push_str("## Context\n\n");
        output.push_str(&self.context);
        output.push_str("\n\n");

        output.push_str("## Decision\n\n");
        output.push_str(&self.decision);
        output.push_str("\n\n");

        if !self.alternatives.is_empty() {
            output.push_str("## Alternatives Considered\n\n");
            for alt in &self.alternatives {
                output.push_str(&format!("### {}\n\n", alt.name));
                output.push_str(&format!("{}\n\n", alt.description));
                if !alt.pros.is_empty() {
                    output.push_str("**Pros:**\n");
                    for pro in &alt.pros {
                        output.push_str(&format!("- {}\n", pro));
                    }
                    output.push('\n');
                }
                if !alt.cons.is_empty() {
                    output.push_str("**Cons:**\n");
                    for con in &alt.cons {
                        output.push_str(&format!("- {}\n", con));
                    }
                    output.push('\n');
                }
            }
        }

        output.push_str("## Consequences\n\n");
        for consequence in &self.consequences {
            output.push_str(&format!("- {}\n", consequence));
        }
        output.push('\n');

        if !self.related.is_empty() {
            output.push_str("## Related ADRs\n\n");
            for related in &self.related {
                output.push_str(&format!("- ADR-{:04}\n", related));
            }
            output.push('\n');
        }

        output
    }
}

/// ADR status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdrStatus {
    /// Proposed but not yet accepted
    Proposed,
    /// Accepted and in use
    Accepted,
    /// Deprecated, replaced by another
    Deprecated,
    /// Superseded by another ADR
    Superseded,
    /// Rejected
    Rejected,
}

impl std::fmt::Display for AdrStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdrStatus::Proposed => write!(f, "Proposed"),
            AdrStatus::Accepted => write!(f, "Accepted"),
            AdrStatus::Deprecated => write!(f, "Deprecated"),
            AdrStatus::Superseded => write!(f, "Superseded"),
            AdrStatus::Rejected => write!(f, "Rejected"),
        }
    }
}

/// ADR alternative option
#[derive(Debug, Clone)]
pub struct AdrAlternative {
    /// Alternative name
    pub name: String,
    /// Description
    pub description: String,
    /// Pros
    pub pros: Vec<String>,
    /// Cons
    pub cons: Vec<String>,
}

impl AdrAlternative {
    /// Create new alternative
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            pros: Vec::new(),
            cons: Vec::new(),
        }
    }

    /// Add pro
    pub fn with_pro(mut self, pro: impl Into<String>) -> Self {
        self.pros.push(pro.into());
        self
    }

    /// Add con
    pub fn with_con(mut self, con: impl Into<String>) -> Self {
        self.cons.push(con.into());
        self
    }
}

/// Changelog entry type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeType {
    /// New feature added
    Added,
    /// Change in existing functionality
    Changed,
    /// Soon-to-be removed features
    Deprecated,
    /// Removed features
    Removed,
    /// Bug fixes
    Fixed,
    /// Security fixes
    Security,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Added => write!(f, "Added"),
            ChangeType::Changed => write!(f, "Changed"),
            ChangeType::Deprecated => write!(f, "Deprecated"),
            ChangeType::Removed => write!(f, "Removed"),
            ChangeType::Fixed => write!(f, "Fixed"),
            ChangeType::Security => write!(f, "Security"),
        }
    }
}

/// A changelog entry
#[derive(Debug, Clone)]
pub struct ChangelogEntry {
    /// Change type
    pub change_type: ChangeType,
    /// Description of the change
    pub description: String,
    /// Related issue/PR number
    pub reference: Option<String>,
    /// Author
    pub author: Option<String>,
    /// Breaking change flag
    pub breaking: bool,
}

impl ChangelogEntry {
    /// Create a new changelog entry
    pub fn new(change_type: ChangeType, description: impl Into<String>) -> Self {
        Self {
            change_type,
            description: description.into(),
            reference: None,
            author: None,
            breaking: false,
        }
    }

    /// Set reference
    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    /// Set author
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Mark as breaking change
    pub fn breaking(mut self) -> Self {
        self.breaking = true;
        self
    }
}

/// A version's changelog
#[derive(Debug, Clone)]
pub struct VersionChangelog {
    /// Version number
    pub version: String,
    /// Release date
    pub date: Option<String>,
    /// Entries grouped by type
    pub entries: HashMap<ChangeType, Vec<ChangelogEntry>>,
    /// Whether this is a yanked release
    pub yanked: bool,
}

impl VersionChangelog {
    /// Create a new version changelog
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            date: None,
            entries: HashMap::new(),
            yanked: false,
        }
    }

    /// Set release date
    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Add an entry
    pub fn add_entry(&mut self, entry: ChangelogEntry) {
        self.entries
            .entry(entry.change_type)
            .or_default()
            .push(entry);
    }

    /// Check if version has breaking changes
    pub fn has_breaking_changes(&self) -> bool {
        self.entries
            .values()
            .any(|entries| entries.iter().any(|e| e.breaking))
    }

    /// Render to markdown
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // Version header
        let date_str = self.date.as_deref().unwrap_or("Unreleased");
        let yanked_str = if self.yanked { " [YANKED]" } else { "" };
        output.push_str(&format!(
            "## [{}] - {}{}\n\n",
            self.version, date_str, yanked_str
        ));

        // Order of sections per Keep a Changelog
        let order = [
            ChangeType::Added,
            ChangeType::Changed,
            ChangeType::Deprecated,
            ChangeType::Removed,
            ChangeType::Fixed,
            ChangeType::Security,
        ];

        for change_type in &order {
            if let Some(entries) = self.entries.get(change_type) {
                if !entries.is_empty() {
                    output.push_str(&format!("### {}\n\n", change_type));
                    for entry in entries {
                        let breaking_prefix = if entry.breaking { "**BREAKING:** " } else { "" };
                        let reference = entry
                            .reference
                            .as_ref()
                            .map(|r| format!(" ({})", r))
                            .unwrap_or_default();
                        let author = entry
                            .author
                            .as_ref()
                            .map(|a| format!(" - @{}", a))
                            .unwrap_or_default();
                        output.push_str(&format!(
                            "- {}{}{}{}\n",
                            breaking_prefix, entry.description, reference, author
                        ));
                    }
                    output.push('\n');
                }
            }
        }

        output
    }
}

/// Complete changelog
#[derive(Debug, Clone)]
pub struct Changelog {
    /// Unique identifier
    pub id: String,
    /// Project name
    pub project: String,
    /// Versions (most recent first)
    pub versions: Vec<VersionChangelog>,
    /// Unreleased changes
    pub unreleased: VersionChangelog,
}

impl Changelog {
    /// Create a new changelog
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            id: generate_changelog_id(),
            project: project.into(),
            versions: Vec::new(),
            unreleased: VersionChangelog::new("Unreleased"),
        }
    }

    /// Add unreleased entry
    pub fn add_unreleased(&mut self, entry: ChangelogEntry) {
        self.unreleased.add_entry(entry);
    }

    /// Release a version
    pub fn release(&mut self, version: impl Into<String>, date: impl Into<String>) {
        let mut changelog =
            std::mem::replace(&mut self.unreleased, VersionChangelog::new("Unreleased"));
        changelog.version = version.into();
        changelog.date = Some(date.into());
        self.versions.insert(0, changelog);
    }

    /// Render to markdown
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        output.push_str("# Changelog\n\n");
        output.push_str("All notable changes to this project will be documented in this file.\n\n");
        output.push_str(
            "The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),\n",
        );
        output.push_str("and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).\n\n");

        // Unreleased
        if !self.unreleased.entries.is_empty() {
            output.push_str(&self.unreleased.to_markdown());
        }

        // Versions
        for version in &self.versions {
            output.push_str(&version.to_markdown());
        }

        output
    }
}

/// Extracts documentation from source code
#[derive(Debug)]
pub struct CodeDocExtractor {
    /// Supported languages
    languages: HashMap<String, LanguageConfig>,
}

/// Configuration for a language's documentation extraction
#[derive(Debug, Clone)]
pub struct LanguageConfig {
    /// Single-line comment prefix
    pub line_comment: String,
    /// Block comment start
    pub block_comment_start: String,
    /// Block comment end
    pub block_comment_end: String,
    /// Doc comment prefix
    pub doc_comment: String,
    /// File extensions
    pub extensions: Vec<String>,
}

impl CodeDocExtractor {
    /// Create a new code doc extractor
    pub fn new() -> Self {
        let mut languages = HashMap::new();

        // Rust
        languages.insert(
            "rust".to_string(),
            LanguageConfig {
                line_comment: "//".to_string(),
                block_comment_start: "/*".to_string(),
                block_comment_end: "*/".to_string(),
                doc_comment: "///".to_string(),
                extensions: vec!["rs".to_string()],
            },
        );

        // JavaScript/TypeScript
        languages.insert(
            "javascript".to_string(),
            LanguageConfig {
                line_comment: "//".to_string(),
                block_comment_start: "/*".to_string(),
                block_comment_end: "*/".to_string(),
                doc_comment: "/**".to_string(),
                extensions: vec![
                    "js".to_string(),
                    "ts".to_string(),
                    "jsx".to_string(),
                    "tsx".to_string(),
                ],
            },
        );

        // Python
        languages.insert(
            "python".to_string(),
            LanguageConfig {
                line_comment: "#".to_string(),
                block_comment_start: "\"\"\"".to_string(),
                block_comment_end: "\"\"\"".to_string(),
                doc_comment: "\"\"\"".to_string(),
                extensions: vec!["py".to_string()],
            },
        );

        // Go
        languages.insert(
            "go".to_string(),
            LanguageConfig {
                line_comment: "//".to_string(),
                block_comment_start: "/*".to_string(),
                block_comment_end: "*/".to_string(),
                doc_comment: "//".to_string(),
                extensions: vec!["go".to_string()],
            },
        );

        Self { languages }
    }

    /// Detect language from file extension
    pub fn detect_language(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        for (name, config) in &self.languages {
            if config.extensions.contains(&ext.to_string()) {
                return Some(name.clone());
            }
        }
        None
    }

    /// Extract documentation from source code
    pub fn extract(&self, content: &str, language: &str) -> Vec<DocItem> {
        let config = match self.languages.get(language) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut items = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Check for doc comment
            if line.starts_with(&config.doc_comment) {
                let mut doc_lines = Vec::new();

                // Collect consecutive doc comments
                while i < lines.len() && lines[i].trim().starts_with(&config.doc_comment) {
                    let doc_line = lines[i]
                        .trim()
                        .strip_prefix(&config.doc_comment)
                        .unwrap_or("")
                        .trim();
                    doc_lines.push(doc_line.to_string());
                    i += 1;
                }

                // Look for the item being documented
                if i < lines.len() {
                    let item_line = lines[i];
                    if let Some(item) = self.parse_item_declaration(item_line, language, &doc_lines)
                    {
                        items.push(item);
                    }
                }
            }

            i += 1;
        }

        items
    }

    /// Parse an item declaration and create a DocItem
    fn parse_item_declaration(
        &self,
        line: &str,
        language: &str,
        doc_lines: &[String],
    ) -> Option<DocItem> {
        let line = line.trim();

        match language {
            "rust" => self.parse_rust_declaration(line, doc_lines),
            "javascript" | "typescript" => self.parse_js_declaration(line, doc_lines),
            "python" => self.parse_python_declaration(line, doc_lines),
            "go" => self.parse_go_declaration(line, doc_lines),
            _ => None,
        }
    }

    /// Parse Rust declaration
    fn parse_rust_declaration(&self, line: &str, doc_lines: &[String]) -> Option<DocItem> {
        // Match function: pub fn name(...) -> Type
        if line.contains("fn ") {
            let name = line.split("fn ").nth(1)?.split(['(', '<']).next()?.trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            return Some(item);
        }

        // Match struct: pub struct Name
        if line.contains("struct ") {
            let name = line
                .split("struct ")
                .nth(1)?
                .split(['{', '<', '(', ';'])
                .next()?
                .trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            item.tags.push("struct".to_string());
            return Some(item);
        }

        // Match enum
        if line.contains("enum ") {
            let name = line.split("enum ").nth(1)?.split(['{', '<']).next()?.trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            item.tags.push("enum".to_string());
            return Some(item);
        }

        // Match trait
        if line.contains("trait ") {
            let name = line
                .split("trait ")
                .nth(1)?
                .split(['{', '<', ':'])
                .next()?
                .trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            item.tags.push("trait".to_string());
            return Some(item);
        }

        None
    }

    /// Parse JavaScript/TypeScript declaration
    fn parse_js_declaration(&self, line: &str, doc_lines: &[String]) -> Option<DocItem> {
        // Match function
        if line.contains("function ") || line.contains("const ") && line.contains("= (") {
            let name = if line.contains("function ") {
                line.split("function ").nth(1)?.split(['(', '<']).next()?
            } else {
                line.split("const ").nth(1)?.split([' ', '=']).next()?
            };
            let mut item = DocItem::new(name.trim(), DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            return Some(item);
        }

        // Match class
        if line.contains("class ") {
            let name = line
                .split("class ")
                .nth(1)?
                .split(['{', ' '])
                .next()?
                .trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            item.tags.push("class".to_string());
            return Some(item);
        }

        None
    }

    /// Parse Python declaration
    fn parse_python_declaration(&self, line: &str, doc_lines: &[String]) -> Option<DocItem> {
        // Match function
        if line.starts_with("def ") || line.starts_with("async def ") {
            let name = line.split("def ").nth(1)?.split('(').next()?.trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            return Some(item);
        }

        // Match class
        if line.starts_with("class ") {
            let name = line
                .split("class ")
                .nth(1)?
                .split(['(', ':'])
                .next()?
                .trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            item.tags.push("class".to_string());
            return Some(item);
        }

        None
    }

    /// Parse Go declaration
    fn parse_go_declaration(&self, line: &str, doc_lines: &[String]) -> Option<DocItem> {
        // Match function
        if line.starts_with("func ") {
            let rest = line.strip_prefix("func ")?;
            let name = if rest.starts_with('(') {
                // Method: func (r Receiver) Name(...)
                rest.split(") ").nth(1)?.split('(').next()?
            } else {
                // Function: func Name(...)
                rest.split('(').next()?
            };
            let mut item = DocItem::new(name.trim(), DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            return Some(item);
        }

        // Match type struct
        if line.starts_with("type ") && line.contains("struct") {
            let name = line.split("type ").nth(1)?.split(' ').next()?.trim();
            let mut item = DocItem::new(name, DocType::Api);
            item.brief = doc_lines.first().cloned().unwrap_or_default();
            item.description = doc_lines.join("\n");
            item.tags.push("struct".to_string());
            return Some(item);
        }

        None
    }
}

impl Default for CodeDocExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Documentation synchronization status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    /// Docs are in sync with code
    InSync,
    /// Docs are outdated
    Outdated,
    /// Documentation is missing
    Missing,
    /// Orphaned documentation (code was removed)
    Orphaned,
}

/// Doc sync issue
#[derive(Debug, Clone)]
pub struct SyncIssue {
    /// File with the issue
    pub file: PathBuf,
    /// Item name
    pub item: String,
    /// Sync status
    pub status: SyncStatus,
    /// Description of the issue
    pub description: String,
}

/// Checks documentation synchronization with code
#[derive(Debug)]
pub struct DocSyncChecker {
    /// Code documentation extractor
    extractor: CodeDocExtractor,
}

impl DocSyncChecker {
    /// Create a new sync checker
    pub fn new() -> Self {
        Self {
            extractor: CodeDocExtractor::new(),
        }
    }

    /// Check synchronization between code and docs
    pub fn check(&self, source_dir: &Path, docs_dir: &Path) -> Vec<SyncIssue> {
        let mut issues = Vec::new();

        // This is a simplified implementation
        // A full implementation would:
        // 1. Extract all doc items from source
        // 2. Parse existing documentation
        // 3. Compare and find discrepancies

        // Check if docs directory exists
        if !docs_dir.exists() {
            issues.push(SyncIssue {
                file: docs_dir.to_path_buf(),
                item: "docs/".to_string(),
                status: SyncStatus::Missing,
                description: "Documentation directory does not exist".to_string(),
            });
        }

        // Check source files
        if let Ok(entries) = std::fs::read_dir(source_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(lang) = self.extractor.detect_language(&path) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let items = self.extractor.extract(&content, &lang);
                            for item in items {
                                if item.brief.is_empty() && item.description.is_empty() {
                                    issues.push(SyncIssue {
                                        file: path.clone(),
                                        item: item.name.clone(),
                                        status: SyncStatus::Missing,
                                        description: format!(
                                            "Item '{}' has no documentation",
                                            item.name
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        issues
    }
}

impl Default for DocSyncChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// ADR manager
#[derive(Debug)]
pub struct AdrManager {
    /// Directory for ADRs
    adr_dir: PathBuf,
    /// ADR template
    template: Option<String>,
}

impl AdrManager {
    /// Create a new ADR manager
    pub fn new(adr_dir: PathBuf) -> Self {
        Self {
            adr_dir,
            template: None,
        }
    }

    /// Set custom template
    pub fn with_template(mut self, template: impl Into<String>) -> Self {
        self.template = Some(template.into());
        self
    }

    /// Get next ADR number
    pub fn next_number(&self) -> u32 {
        if !self.adr_dir.exists() {
            return 1;
        }

        std::fs::read_dir(&self.adr_dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.split('-').next()?.parse::<u32>().ok()
                    })
                    .max()
                    .unwrap_or(0)
                    + 1
            })
            .unwrap_or(1)
    }

    /// Create a new ADR
    pub fn create(&self, title: impl Into<String>) -> Adr {
        let number = self.next_number();
        Adr::new(number, title)
    }

    /// Save ADR to file
    pub fn save(&self, adr: &Adr) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.adr_dir)?;
        let path = self.adr_dir.join(adr.filename());
        std::fs::write(&path, adr.to_markdown())?;
        Ok(path)
    }

    /// List all ADRs
    pub fn list(&self) -> Vec<(u32, String, AdrStatus)> {
        let mut adrs = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.adr_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".md") {
                    if let Some(num) = name.split('-').next().and_then(|n| n.parse::<u32>().ok()) {
                        // Parse title from filename
                        let title = name
                            .strip_suffix(".md")
                            .unwrap_or(&name)
                            .split('-')
                            .skip(1)
                            .collect::<Vec<_>>()
                            .join(" ");

                        // Default to Accepted status
                        adrs.push((num, title, AdrStatus::Accepted));
                    }
                }
            }
        }

        adrs.sort_by_key(|a| a.0);
        adrs
    }
}

/// Changelog generator from git history
#[derive(Debug)]
pub struct ChangelogGenerator {
    /// Repository path
    _repo_path: PathBuf,
    /// Conventional commit parsing
    conventional: bool,
}

impl ChangelogGenerator {
    /// Create a new changelog generator
    pub fn new(repo_path: PathBuf) -> Self {
        Self {
            _repo_path: repo_path,
            conventional: true,
        }
    }

    /// Disable conventional commit parsing
    pub fn without_conventional(mut self) -> Self {
        self.conventional = false;
        self
    }

    /// Parse change type from conventional commit
    pub fn parse_commit_type(&self, message: &str) -> ChangeType {
        if !self.conventional {
            return ChangeType::Changed;
        }

        let lower = message.to_lowercase();

        if lower.starts_with("feat") {
            ChangeType::Added
        } else if lower.starts_with("fix") {
            ChangeType::Fixed
        } else if lower.starts_with("security") || lower.contains("cve") {
            ChangeType::Security
        } else if lower.starts_with("deprecate") {
            ChangeType::Deprecated
        } else if lower.starts_with("remove") || lower.starts_with("revert") {
            ChangeType::Removed
        } else {
            ChangeType::Changed
        }
    }

    /// Check if commit is breaking change
    pub fn is_breaking(&self, message: &str) -> bool {
        message.contains("!:") || message.to_lowercase().contains("breaking")
    }

    /// Extract scope from conventional commit
    pub fn extract_scope(&self, message: &str) -> Option<String> {
        if !self.conventional {
            return None;
        }

        let start = message.find('(')?;
        let end = message.find(')')?;
        if start < end {
            Some(message[start + 1..end].to_string())
        } else {
            None
        }
    }

    /// Extract description from conventional commit
    pub fn extract_description(&self, message: &str) -> String {
        if !self.conventional {
            return message.to_string();
        }

        // Format: type(scope): description
        // or: type: description
        if let Some(colon_pos) = message.find(':') {
            message[colon_pos + 1..].trim().to_string()
        } else {
            message.to_string()
        }
    }

    /// Generate changelog from git log output
    pub fn generate_from_log(&self, git_log: &str, project_name: &str) -> Changelog {
        let mut changelog = Changelog::new(project_name);

        for line in git_log.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let change_type = self.parse_commit_type(line);
            let description = self.extract_description(line);
            let breaking = self.is_breaking(line);

            let mut entry = ChangelogEntry::new(change_type, description);
            if breaking {
                entry = entry.breaking();
            }

            changelog.add_unreleased(entry);
        }

        changelog
    }
}

/// Main documentation generator orchestrating all components
#[derive(Debug)]
pub struct DocumentationGenerator {
    /// Project root directory
    _project_root: PathBuf,
    /// Output directory for generated docs
    output_dir: PathBuf,
    /// Documentation format
    format: DocFormat,
    /// Code documentation extractor
    extractor: CodeDocExtractor,
    /// ADR manager
    adr_manager: AdrManager,
    /// Changelog generator
    changelog_generator: ChangelogGenerator,
    /// Doc sync checker
    sync_checker: DocSyncChecker,
}

impl DocumentationGenerator {
    /// Create a new documentation generator
    pub fn new(project_root: PathBuf) -> Self {
        let output_dir = project_root.join("docs");
        let adr_dir = output_dir.join("adr");

        Self {
            _project_root: project_root.clone(),
            output_dir,
            format: DocFormat::Markdown,
            extractor: CodeDocExtractor::new(),
            adr_manager: AdrManager::new(adr_dir),
            changelog_generator: ChangelogGenerator::new(project_root),
            sync_checker: DocSyncChecker::new(),
        }
    }

    /// Set output directory
    pub fn with_output_dir(mut self, dir: PathBuf) -> Self {
        self.output_dir = dir;
        self
    }

    /// Set documentation format
    pub fn with_format(mut self, format: DocFormat) -> Self {
        self.format = format;
        self
    }

    /// Generate API documentation
    pub fn generate_api_docs(&self, source_dir: &Path) -> std::io::Result<Vec<DocItem>> {
        let mut all_items = Vec::new();

        self.collect_source_files(source_dir, &mut |path| {
            if let Some(lang) = self.extractor.detect_language(path) {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let mut items = self.extractor.extract(&content, &lang);
                    for item in &mut items {
                        item.source_path = Some(path.to_path_buf());
                    }
                    all_items.extend(items);
                }
            }
        });

        // Write API docs
        std::fs::create_dir_all(self.output_dir.join("api"))?;

        let mut api_index = String::new();
        api_index.push_str("# API Documentation\n\n");

        for item in &all_items {
            api_index.push_str(&format!("- [{}]({})\n", item.name, item.id));
        }

        let ext = self.format.extension();
        std::fs::write(
            self.output_dir.join("api").join(format!("index.{}", ext)),
            api_index,
        )?;

        for item in &all_items {
            std::fs::write(
                self.output_dir
                    .join("api")
                    .join(format!("{}.{}", item.id, ext)),
                item.to_markdown(),
            )?;
        }

        Ok(all_items)
    }

    /// Create a new ADR
    pub fn create_adr(&self, title: impl Into<String>) -> std::io::Result<Adr> {
        let adr = self.adr_manager.create(title);
        Ok(adr)
    }

    /// Save an ADR
    pub fn save_adr(&self, adr: &Adr) -> std::io::Result<PathBuf> {
        self.adr_manager.save(adr)
    }

    /// List all ADRs
    pub fn list_adrs(&self) -> Vec<(u32, String, AdrStatus)> {
        self.adr_manager.list()
    }

    /// Check doc synchronization
    pub fn check_sync(&self, source_dir: &Path) -> Vec<SyncIssue> {
        self.sync_checker.check(source_dir, &self.output_dir)
    }

    /// Generate changelog
    pub fn generate_changelog(&self, git_log: &str) -> Changelog {
        self.changelog_generator
            .generate_from_log(git_log, "Project")
    }

    /// Helper to collect source files
    fn collect_source_files(&self, dir: &Path, callback: &mut impl FnMut(&Path)) {
        collect_source_files_recursive(dir, callback);
    }
}

/// Recursively collect source files, skipping hidden/build directories
fn collect_source_files_recursive(dir: &Path, callback: &mut impl FnMut(&Path)) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip hidden directories and common non-source dirs
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') && name != "target" && name != "node_modules" {
                    collect_source_files_recursive(&path, callback);
                }
            } else if path.is_file() {
                callback(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_item_creation() {
        let item = DocItem::new("test_function", DocType::Api)
            .with_brief("A test function")
            .with_description("This is a detailed description");

        assert_eq!(item.name, "test_function");
        assert_eq!(item.brief, "A test function");
        assert!(!item.description.is_empty());
    }

    #[test]
    fn test_doc_item_with_parameters() {
        let item = DocItem::new("calculate", DocType::Api)
            .with_parameter(ParameterDoc::new("a", "i32").with_description("First number"))
            .with_parameter(ParameterDoc::new("b", "i32").with_description("Second number"))
            .with_returns("The sum of a and b");

        assert_eq!(item.parameters.len(), 2);
        assert!(item.returns.is_some());
    }

    #[test]
    fn test_doc_item_to_markdown() {
        let item = DocItem::new("my_function", DocType::Api)
            .with_brief("Brief description")
            .with_example("let x = my_function();");

        let md = item.to_markdown();

        assert!(md.contains("## my_function"));
        assert!(md.contains("Brief description"));
        assert!(md.contains("```rust"));
        assert!(md.contains("let x = my_function();"));
    }

    #[test]
    fn test_parameter_doc() {
        let param = ParameterDoc::new("count", "usize")
            .with_description("Number of items")
            .with_default("10");

        assert_eq!(param.name, "count");
        assert_eq!(param.param_type, "usize");
        assert!(!param.required);
        assert_eq!(param.default, Some("10".to_string()));
    }

    #[test]
    fn test_adr_creation() {
        let adr = Adr::new(1, "Use Rust for implementation")
            .with_context("We need a systems programming language")
            .with_decision("Use Rust for its safety guarantees")
            .with_consequence("Team needs Rust training")
            .with_status(AdrStatus::Accepted);

        assert_eq!(adr.number, 1);
        assert_eq!(adr.status, AdrStatus::Accepted);
        assert_eq!(adr.consequences.len(), 1);
    }

    #[test]
    fn test_adr_filename() {
        let adr = Adr::new(5, "Use PostgreSQL Database");
        let filename = adr.filename();

        assert!(filename.starts_with("0005-"));
        assert!(filename.ends_with(".md"));
        assert!(filename.contains("use-postgresql-database"));
    }

    #[test]
    fn test_adr_with_alternatives() {
        let alt = AdrAlternative::new("SQLite", "Embedded database")
            .with_pro("Simple setup")
            .with_con("Limited scalability");

        let adr = Adr::new(2, "Database Selection").with_alternative(alt);

        assert_eq!(adr.alternatives.len(), 1);
        assert_eq!(adr.alternatives[0].pros.len(), 1);
        assert_eq!(adr.alternatives[0].cons.len(), 1);
    }

    #[test]
    fn test_adr_to_markdown() {
        let adr = Adr::new(1, "Test ADR")
            .with_context("Context here")
            .with_decision("Decision here")
            .with_consequence("Consequence here");

        let md = adr.to_markdown();

        assert!(md.contains("# ADR-0001: Test ADR"));
        assert!(md.contains("## Context"));
        assert!(md.contains("## Decision"));
        assert!(md.contains("## Consequences"));
    }

    #[test]
    fn test_adr_status_display() {
        assert_eq!(format!("{}", AdrStatus::Proposed), "Proposed");
        assert_eq!(format!("{}", AdrStatus::Accepted), "Accepted");
        assert_eq!(format!("{}", AdrStatus::Deprecated), "Deprecated");
    }

    #[test]
    fn test_changelog_entry_creation() {
        let entry = ChangelogEntry::new(ChangeType::Added, "New feature")
            .with_reference("#123")
            .with_author("developer");

        assert_eq!(entry.change_type, ChangeType::Added);
        assert_eq!(entry.reference, Some("#123".to_string()));
        assert!(!entry.breaking);
    }

    #[test]
    fn test_changelog_entry_breaking() {
        let entry = ChangelogEntry::new(ChangeType::Changed, "API change").breaking();

        assert!(entry.breaking);
    }

    #[test]
    fn test_version_changelog() {
        let mut version = VersionChangelog::new("1.0.0").with_date("2024-01-15");

        version.add_entry(ChangelogEntry::new(ChangeType::Added, "Feature A"));
        version.add_entry(ChangelogEntry::new(ChangeType::Fixed, "Bug B"));

        assert_eq!(version.version, "1.0.0");
        assert!(version.entries.contains_key(&ChangeType::Added));
        assert!(version.entries.contains_key(&ChangeType::Fixed));
    }

    #[test]
    fn test_version_changelog_has_breaking_changes() {
        let mut version = VersionChangelog::new("2.0.0");

        version.add_entry(ChangelogEntry::new(ChangeType::Changed, "Breaking API").breaking());

        assert!(version.has_breaking_changes());
    }

    #[test]
    fn test_version_changelog_to_markdown() {
        let mut version = VersionChangelog::new("1.0.0").with_date("2024-01-15");

        version.add_entry(ChangelogEntry::new(ChangeType::Added, "New feature"));

        let md = version.to_markdown();

        assert!(md.contains("[1.0.0]"));
        assert!(md.contains("2024-01-15"));
        assert!(md.contains("### Added"));
        assert!(md.contains("New feature"));
    }

    #[test]
    fn test_changelog_creation() {
        let mut changelog = Changelog::new("My Project");

        changelog.add_unreleased(ChangelogEntry::new(ChangeType::Added, "WIP feature"));

        assert_eq!(changelog.project, "My Project");
        assert!(!changelog.unreleased.entries.is_empty());
    }

    #[test]
    fn test_changelog_release() {
        let mut changelog = Changelog::new("Test");

        changelog.add_unreleased(ChangelogEntry::new(ChangeType::Added, "Feature"));
        changelog.release("1.0.0", "2024-01-15");

        assert!(changelog.unreleased.entries.is_empty());
        assert_eq!(changelog.versions.len(), 1);
        assert_eq!(changelog.versions[0].version, "1.0.0");
    }

    #[test]
    fn test_changelog_to_markdown() {
        let mut changelog = Changelog::new("Test Project");

        changelog.add_unreleased(ChangelogEntry::new(ChangeType::Added, "New"));

        let md = changelog.to_markdown();

        assert!(md.contains("# Changelog"));
        assert!(md.contains("Keep a Changelog"));
        assert!(md.contains("Semantic Versioning"));
    }

    #[test]
    fn test_change_type_display() {
        assert_eq!(format!("{}", ChangeType::Added), "Added");
        assert_eq!(format!("{}", ChangeType::Fixed), "Fixed");
        assert_eq!(format!("{}", ChangeType::Security), "Security");
    }

    #[test]
    fn test_code_doc_extractor_creation() {
        let extractor = CodeDocExtractor::new();

        assert!(extractor.languages.contains_key("rust"));
        assert!(extractor.languages.contains_key("javascript"));
        assert!(extractor.languages.contains_key("python"));
        assert!(extractor.languages.contains_key("go"));
    }

    #[test]
    fn test_detect_language() {
        let extractor = CodeDocExtractor::new();

        assert_eq!(
            extractor.detect_language(Path::new("file.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            extractor.detect_language(Path::new("file.js")),
            Some("javascript".to_string())
        );
        assert_eq!(
            extractor.detect_language(Path::new("file.py")),
            Some("python".to_string())
        );
        assert_eq!(
            extractor.detect_language(Path::new("file.go")),
            Some("go".to_string())
        );
        assert_eq!(extractor.detect_language(Path::new("file.unknown")), None);
    }

    #[test]
    fn test_extract_rust_function() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
/// This is a test function
/// It does something useful
pub fn test_fn(x: i32) -> i32 {
    x + 1
}
"#;

        let items = extractor.extract(code, "rust");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "test_fn");
        assert!(items[0].brief.contains("test function"));
    }

    #[test]
    fn test_extract_rust_struct() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
/// A configuration struct
pub struct Config {
    pub name: String,
}
"#;

        let items = extractor.extract(code, "rust");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Config");
        assert!(items[0].tags.contains(&"struct".to_string()));
    }

    #[test]
    fn test_extract_rust_enum() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
/// Status values
pub enum Status {
    Active,
    Inactive,
}
"#;

        let items = extractor.extract(code, "rust");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Status");
        assert!(items[0].tags.contains(&"enum".to_string()));
    }

    #[test]
    fn test_extract_rust_trait() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
/// A behavior trait
pub trait Behavior {
    fn execute(&self);
}
"#;

        let items = extractor.extract(code, "rust");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Behavior");
        assert!(items[0].tags.contains(&"trait".to_string()));
    }

    #[test]
    fn test_extract_js_function() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
/** Calculate sum */
function calculateSum(a, b) {
    return a + b;
}
"#;

        let items = extractor.extract(code, "javascript");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "calculateSum");
    }

    #[test]
    fn test_extract_js_class() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
/** A helper class */
class Helper {
    constructor() {}
}
"#;

        let items = extractor.extract(code, "javascript");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Helper");
        assert!(items[0].tags.contains(&"class".to_string()));
    }

    #[test]
    fn test_extract_python_function() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
"""Process data"""
def process_data(data):
    return data
"#;

        let items = extractor.extract(code, "python");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "process_data");
    }

    #[test]
    fn test_extract_python_class() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
"""A data class"""
class DataProcessor:
    pass
"#;

        let items = extractor.extract(code, "python");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "DataProcessor");
        assert!(items[0].tags.contains(&"class".to_string()));
    }

    #[test]
    fn test_extract_go_function() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
// ProcessRequest handles HTTP requests
func ProcessRequest(w http.ResponseWriter, r *http.Request) {
}
"#;

        let items = extractor.extract(code, "go");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "ProcessRequest");
    }

    #[test]
    fn test_extract_go_struct() {
        let extractor = CodeDocExtractor::new();

        let code = r#"
// Server configuration
type ServerConfig struct {
    Port int
}
"#;

        let items = extractor.extract(code, "go");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "ServerConfig");
        assert!(items[0].tags.contains(&"struct".to_string()));
    }

    #[test]
    fn test_sync_status() {
        assert_eq!(SyncStatus::InSync, SyncStatus::InSync);
        assert_ne!(SyncStatus::InSync, SyncStatus::Outdated);
    }

    #[test]
    fn test_doc_sync_checker_creation() {
        let checker = DocSyncChecker::new();
        assert!(!checker.extractor.languages.is_empty());
    }

    #[test]
    fn test_adr_manager_next_number_empty() {
        let manager = AdrManager::new(PathBuf::from("/nonexistent"));
        assert_eq!(manager.next_number(), 1);
    }

    #[test]
    fn test_adr_manager_create() {
        let manager = AdrManager::new(PathBuf::from("/tmp"));
        let adr = manager.create("Test ADR");

        assert_eq!(adr.title, "Test ADR");
        assert!(adr.number >= 1);
    }

    #[test]
    fn test_changelog_generator_creation() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));
        assert!(gen.conventional);
    }

    #[test]
    fn test_changelog_generator_without_conventional() {
        let gen = ChangelogGenerator::new(PathBuf::from(".")).without_conventional();
        assert!(!gen.conventional);
    }

    #[test]
    fn test_parse_commit_type_feat() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(
            gen.parse_commit_type("feat: new feature"),
            ChangeType::Added
        );
        assert_eq!(
            gen.parse_commit_type("feat(scope): scoped feature"),
            ChangeType::Added
        );
    }

    #[test]
    fn test_parse_commit_type_fix() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(gen.parse_commit_type("fix: bug fix"), ChangeType::Fixed);
        assert_eq!(
            gen.parse_commit_type("fix(auth): auth bug"),
            ChangeType::Fixed
        );
    }

    #[test]
    fn test_parse_commit_type_security() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(
            gen.parse_commit_type("security: patch CVE-2024-123"),
            ChangeType::Security
        );
    }

    #[test]
    fn test_parse_commit_type_deprecate() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(
            gen.parse_commit_type("deprecate: old API"),
            ChangeType::Deprecated
        );
    }

    #[test]
    fn test_parse_commit_type_remove() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(
            gen.parse_commit_type("remove: dead code"),
            ChangeType::Removed
        );
        assert_eq!(
            gen.parse_commit_type("revert: bad commit"),
            ChangeType::Removed
        );
    }

    #[test]
    fn test_parse_commit_type_other() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(
            gen.parse_commit_type("docs: update readme"),
            ChangeType::Changed
        );
        assert_eq!(
            gen.parse_commit_type("refactor: clean up"),
            ChangeType::Changed
        );
    }

    #[test]
    fn test_is_breaking() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert!(gen.is_breaking("feat!: breaking change"));
        assert!(gen.is_breaking("feat: BREAKING change in API"));
        assert!(!gen.is_breaking("feat: normal feature"));
    }

    #[test]
    fn test_extract_scope() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(
            gen.extract_scope("feat(auth): login"),
            Some("auth".to_string())
        );
        assert_eq!(
            gen.extract_scope("fix(api): endpoint"),
            Some("api".to_string())
        );
        assert_eq!(gen.extract_scope("feat: no scope"), None);
    }

    #[test]
    fn test_extract_description() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        assert_eq!(gen.extract_description("feat: new feature"), "new feature");
        assert_eq!(gen.extract_description("fix(auth): login bug"), "login bug");
    }

    #[test]
    fn test_generate_from_log() {
        let gen = ChangelogGenerator::new(PathBuf::from("."));

        let log = "feat: add new feature\nfix: bug fix\n";
        let changelog = gen.generate_from_log(log, "Test");

        assert_eq!(changelog.project, "Test");
        assert!(changelog
            .unreleased
            .entries
            .contains_key(&ChangeType::Added));
        assert!(changelog
            .unreleased
            .entries
            .contains_key(&ChangeType::Fixed));
    }

    #[test]
    fn test_doc_type_display() {
        assert_eq!(format!("{}", DocType::Api), "API");
        assert_eq!(format!("{}", DocType::Architecture), "Architecture");
        assert_eq!(format!("{}", DocType::Adr), "ADR");
        assert_eq!(format!("{}", DocType::Changelog), "Changelog");
    }

    #[test]
    fn test_doc_format_extension() {
        assert_eq!(DocFormat::Markdown.extension(), "md");
        assert_eq!(DocFormat::Html.extension(), "html");
        assert_eq!(DocFormat::Rst.extension(), "rst");
        assert_eq!(DocFormat::AsciiDoc.extension(), "adoc");
        assert_eq!(DocFormat::PlainText.extension(), "txt");
    }

    #[test]
    fn test_documentation_generator_creation() {
        let gen = DocumentationGenerator::new(PathBuf::from("/tmp/test"));

        assert_eq!(gen.output_dir, PathBuf::from("/tmp/test/docs"));
        assert_eq!(gen.format, DocFormat::Markdown);
    }

    #[test]
    fn test_documentation_generator_with_output_dir() {
        let gen = DocumentationGenerator::new(PathBuf::from("/tmp/test"))
            .with_output_dir(PathBuf::from("/custom/docs"));

        assert_eq!(gen.output_dir, PathBuf::from("/custom/docs"));
    }

    #[test]
    fn test_documentation_generator_with_format() {
        let gen =
            DocumentationGenerator::new(PathBuf::from("/tmp/test")).with_format(DocFormat::Html);

        assert_eq!(gen.format, DocFormat::Html);
    }

    #[test]
    fn test_unique_doc_ids() {
        let item1 = DocItem::new("test1", DocType::Api);
        let item2 = DocItem::new("test2", DocType::Api);

        assert_ne!(item1.id, item2.id);
    }

    #[test]
    fn test_unique_adr_ids() {
        let adr1 = Adr::new(1, "Test 1");
        let adr2 = Adr::new(2, "Test 2");

        assert_ne!(adr1.id, adr2.id);
    }

    #[test]
    fn test_unique_changelog_ids() {
        let cl1 = Changelog::new("Project 1");
        let cl2 = Changelog::new("Project 2");

        assert_ne!(cl1.id, cl2.id);
    }

    #[test]
    fn test_doc_item_with_deprecation() {
        let mut item = DocItem::new("old_function", DocType::Api);
        item.deprecated = Some("Use new_function instead".to_string());

        let md = item.to_markdown();
        assert!(md.contains("**Deprecated**"));
        assert!(md.contains("new_function"));
    }

    #[test]
    fn test_doc_item_with_errors() {
        let mut item = DocItem::new("risky_function", DocType::Api);
        item.errors.push("IoError when file not found".to_string());
        item.errors.push("ParseError on invalid input".to_string());

        let md = item.to_markdown();
        assert!(md.contains("### Errors"));
        assert!(md.contains("IoError"));
        assert!(md.contains("ParseError"));
    }

    #[test]
    fn test_version_changelog_yanked() {
        let mut version = VersionChangelog::new("1.0.0");
        version.yanked = true;

        let md = version.to_markdown();
        assert!(md.contains("[YANKED]"));
    }

    #[test]
    fn test_language_config() {
        let config = LanguageConfig {
            line_comment: "//".to_string(),
            block_comment_start: "/*".to_string(),
            block_comment_end: "*/".to_string(),
            doc_comment: "///".to_string(),
            extensions: vec!["rs".to_string()],
        };

        assert_eq!(config.extensions[0], "rs");
        assert_eq!(config.doc_comment, "///");
    }

    #[test]
    fn test_sync_issue_creation() {
        let issue = SyncIssue {
            file: PathBuf::from("src/lib.rs"),
            item: "my_function".to_string(),
            status: SyncStatus::Missing,
            description: "No documentation".to_string(),
        };

        assert_eq!(issue.status, SyncStatus::Missing);
        assert!(issue.description.contains("documentation"));
    }
}
