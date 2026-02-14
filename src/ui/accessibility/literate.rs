//! Literate Programming System
//!
//! Code as narrative: executable documentation, notebook-style workflows,
//! prose-first development.
//!
//! # Concepts
//!
//! - **Cells**: Atomic units of prose or code
//! - **Documents**: Collections of cells forming a narrative
//! - **Tangling**: Extracting code from literate documents
//! - **Weaving**: Generating documentation from code + prose
//! - **Notebooks**: Interactive execution of cell sequences

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Type of cell content
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellType {
    /// Prose/markdown content
    Prose,
    /// Executable code
    Code,
    /// Shell commands
    Shell,
    /// Data/configuration (YAML, JSON, TOML)
    Data,
    /// Output from execution
    Output,
    /// Diagram description (mermaid, plantuml)
    Diagram,
}

impl CellType {
    /// Get cell type from language identifier
    pub fn from_language(lang: &str) -> Self {
        match lang.to_lowercase().as_str() {
            "bash" | "sh" | "shell" | "zsh" => CellType::Shell,
            "yaml" | "yml" | "json" | "toml" | "ini" => CellType::Data,
            "mermaid" | "plantuml" | "graphviz" | "dot" => CellType::Diagram,
            "" => CellType::Prose,
            _ => CellType::Code,
        }
    }
}

/// A single cell in a literate document
#[derive(Debug, Clone)]
pub struct Cell {
    /// Unique cell identifier
    pub id: String,
    /// Cell type
    pub cell_type: CellType,
    /// Cell content (prose or code)
    pub content: String,
    /// Language for code cells
    pub language: Option<String>,
    /// Cell metadata
    pub metadata: HashMap<String, String>,
    /// Execution count (for code cells)
    pub execution_count: Option<u32>,
    /// Cell output (for executed cells)
    pub output: Option<String>,
    /// Dependencies on other cells
    pub depends_on: Vec<String>,
    /// Tags for organization
    pub tags: Vec<String>,
    /// Whether cell is collapsed
    pub collapsed: bool,
}

impl Cell {
    /// Create a new prose cell
    pub fn prose(content: &str) -> Self {
        Self {
            id: Self::generate_id(),
            cell_type: CellType::Prose,
            content: content.to_string(),
            language: None,
            metadata: HashMap::new(),
            execution_count: None,
            output: None,
            depends_on: Vec::new(),
            tags: Vec::new(),
            collapsed: false,
        }
    }

    /// Create a new code cell
    pub fn code(content: &str, language: &str) -> Self {
        Self {
            id: Self::generate_id(),
            cell_type: CellType::from_language(language),
            content: content.to_string(),
            language: Some(language.to_string()),
            metadata: HashMap::new(),
            execution_count: None,
            output: None,
            depends_on: Vec::new(),
            tags: Vec::new(),
            collapsed: false,
        }
    }

    /// Generate unique cell ID
    fn generate_id() -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        format!("cell_{}", now.as_nanos() % 1_000_000_000)
    }

    /// Check if cell is executable
    pub fn is_executable(&self) -> bool {
        matches!(self.cell_type, CellType::Code | CellType::Shell)
    }

    /// Set cell output
    pub fn set_output(&mut self, output: String) {
        self.output = Some(output);
        self.execution_count = Some(self.execution_count.unwrap_or(0) + 1);
    }

    /// Add dependency on another cell
    pub fn depends(&mut self, cell_id: &str) {
        if !self.depends_on.contains(&cell_id.to_string()) {
            self.depends_on.push(cell_id.to_string());
        }
    }

    /// Add a tag
    pub fn tag(&mut self, tag: &str) {
        if !self.tags.contains(&tag.to_string()) {
            self.tags.push(tag.to_string());
        }
    }
}

/// A literate document containing cells
#[derive(Debug, Clone)]
pub struct LiterateDocument {
    /// Document title
    pub title: String,
    /// Document description
    pub description: Option<String>,
    /// Ordered list of cells
    pub cells: Vec<Cell>,
    /// Document metadata
    pub metadata: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: u64,
    /// Last modified timestamp
    pub modified_at: u64,
    /// Author information
    pub author: Option<String>,
    /// Document version
    pub version: String,
}

impl LiterateDocument {
    /// Create a new empty document
    pub fn new(title: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            title: title.to_string(),
            description: None,
            cells: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            modified_at: now,
            author: None,
            version: "1.0.0".to_string(),
        }
    }

    /// Add a cell to the document
    pub fn add_cell(&mut self, cell: Cell) {
        self.cells.push(cell);
        self.touch();
    }

    /// Insert cell at position
    pub fn insert_cell(&mut self, index: usize, cell: Cell) {
        if index <= self.cells.len() {
            self.cells.insert(index, cell);
            self.touch();
        }
    }

    /// Remove cell by ID
    pub fn remove_cell(&mut self, cell_id: &str) -> Option<Cell> {
        if let Some(pos) = self.cells.iter().position(|c| c.id == cell_id) {
            self.touch();
            Some(self.cells.remove(pos))
        } else {
            None
        }
    }

    /// Get cell by ID
    pub fn get_cell(&self, cell_id: &str) -> Option<&Cell> {
        self.cells.iter().find(|c| c.id == cell_id)
    }

    /// Get mutable cell by ID
    pub fn get_cell_mut(&mut self, cell_id: &str) -> Option<&mut Cell> {
        self.cells.iter_mut().find(|c| c.id == cell_id)
    }

    /// Update modified timestamp
    fn touch(&mut self) {
        self.modified_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Get all code cells
    pub fn code_cells(&self) -> Vec<&Cell> {
        self.cells.iter().filter(|c| c.is_executable()).collect()
    }

    /// Get all prose cells
    pub fn prose_cells(&self) -> Vec<&Cell> {
        self.cells
            .iter()
            .filter(|c| c.cell_type == CellType::Prose)
            .collect()
    }

    /// Count cells by type
    pub fn cell_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for cell in &self.cells {
            let key = format!("{:?}", cell.cell_type);
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }
}

/// Parser for literate documents (markdown with code blocks)
#[derive(Debug, Default)]
pub struct DocumentParser {
    /// Fence characters for code blocks
    fence_chars: String,
}

impl DocumentParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self {
            fence_chars: "```".to_string(),
        }
    }

    /// Parse markdown content into a literate document
    pub fn parse(&self, content: &str, title: &str) -> LiterateDocument {
        let mut doc = LiterateDocument::new(title);
        let mut current_prose = String::new();
        let mut in_code_block = false;
        let mut code_content = String::new();
        let mut code_language = String::new();

        for line in content.lines() {
            if line.starts_with(&self.fence_chars) && !in_code_block {
                // Start of code block
                if !current_prose.trim().is_empty() {
                    doc.add_cell(Cell::prose(current_prose.trim()));
                    current_prose.clear();
                }
                code_language = line
                    .trim_start_matches(&self.fence_chars)
                    .trim()
                    .to_string();
                in_code_block = true;
            } else if line.starts_with(&self.fence_chars) && in_code_block {
                // End of code block
                if !code_content.trim().is_empty() {
                    doc.add_cell(Cell::code(code_content.trim(), &code_language));
                }
                code_content.clear();
                code_language.clear();
                in_code_block = false;
            } else if in_code_block {
                // Inside code block
                if !code_content.is_empty() {
                    code_content.push('\n');
                }
                code_content.push_str(line);
            } else {
                // Prose content
                if !current_prose.is_empty() {
                    current_prose.push('\n');
                }
                current_prose.push_str(line);
            }
        }

        // Add remaining prose
        if !current_prose.trim().is_empty() {
            doc.add_cell(Cell::prose(current_prose.trim()));
        }

        doc
    }

    /// Parse with metadata extraction from YAML front matter
    pub fn parse_with_metadata(&self, content: &str) -> LiterateDocument {
        let (metadata, body) = self.extract_front_matter(content);
        let title = metadata
            .get("title")
            .cloned()
            .unwrap_or_else(|| "Untitled".to_string());

        let mut doc = self.parse(body, &title);
        doc.metadata = metadata.clone();
        if let Some(desc) = metadata.get("description") {
            doc.description = Some(desc.clone());
        }
        if let Some(author) = metadata.get("author") {
            doc.author = Some(author.clone());
        }
        if let Some(version) = metadata.get("version") {
            doc.version = version.clone();
        }
        doc
    }

    /// Extract YAML front matter
    fn extract_front_matter<'a>(&self, content: &'a str) -> (HashMap<String, String>, &'a str) {
        let mut metadata = HashMap::new();

        if !content.starts_with("---") {
            return (metadata, content);
        }

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return (metadata, content);
        }

        // Simple YAML parsing (key: value)
        for line in parts[1].lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                metadata.insert(
                    key.trim().to_string(),
                    value
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string(),
                );
            }
        }

        (metadata, parts[2])
    }
}

/// Tangler - extracts code from literate documents
#[derive(Debug, Default)]
pub struct Tangler {
    /// Filter by language
    language_filter: Option<String>,
    /// Include cell IDs as comments
    include_cell_ids: bool,
}

impl Tangler {
    /// Create a new tangler
    pub fn new() -> Self {
        Self::default()
    }

    /// Set language filter
    pub fn filter_language(&mut self, language: &str) {
        self.language_filter = Some(language.to_string());
    }

    /// Enable cell ID comments
    pub fn with_cell_ids(&mut self) {
        self.include_cell_ids = true;
    }

    /// Extract all code from document
    pub fn tangle(&self, doc: &LiterateDocument) -> String {
        let mut output = String::new();

        for cell in &doc.cells {
            if !cell.is_executable() {
                continue;
            }

            if let Some(ref filter) = self.language_filter {
                if cell.language.as_ref() != Some(filter) {
                    continue;
                }
            }

            if self.include_cell_ids {
                output.push_str(&format!("// Cell: {}\n", cell.id));
            }

            output.push_str(&cell.content);
            output.push_str("\n\n");
        }

        output.trim().to_string()
    }

    /// Extract code to file by language
    pub fn tangle_by_language(&self, doc: &LiterateDocument) -> HashMap<String, String> {
        let mut by_lang: HashMap<String, Vec<&str>> = HashMap::new();

        for cell in &doc.cells {
            if !cell.is_executable() {
                continue;
            }

            if let Some(ref lang) = cell.language {
                by_lang.entry(lang.clone()).or_default().push(&cell.content);
            }
        }

        by_lang
            .into_iter()
            .map(|(lang, chunks)| (lang, chunks.join("\n\n")))
            .collect()
    }
}

/// Weaver - generates documentation from literate documents
#[derive(Debug)]
pub struct Weaver {
    /// Output format
    output_format: WeaveFormat,
    /// Include code outputs
    include_outputs: bool,
    /// Include execution counts
    include_execution_counts: bool,
}

/// Output format for weaving
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaveFormat {
    /// GitHub-flavored Markdown
    Markdown,
    /// HTML document
    Html,
    /// LaTeX document
    LaTeX,
    /// Plain text
    PlainText,
}

impl Default for Weaver {
    fn default() -> Self {
        Self {
            output_format: WeaveFormat::Markdown,
            include_outputs: true,
            include_execution_counts: false,
        }
    }
}

impl Weaver {
    /// Create a new weaver
    pub fn new(format: WeaveFormat) -> Self {
        Self {
            output_format: format,
            ..Default::default()
        }
    }

    /// Include outputs in woven document
    pub fn with_outputs(&mut self, include: bool) {
        self.include_outputs = include;
    }

    /// Generate documentation from document
    pub fn weave(&self, doc: &LiterateDocument) -> String {
        match self.output_format {
            WeaveFormat::Markdown => self.weave_markdown(doc),
            WeaveFormat::Html => self.weave_html(doc),
            WeaveFormat::LaTeX => self.weave_latex(doc),
            WeaveFormat::PlainText => self.weave_plaintext(doc),
        }
    }

    /// Weave to markdown
    fn weave_markdown(&self, doc: &LiterateDocument) -> String {
        let mut output = String::new();

        // Title
        output.push_str(&format!("# {}\n\n", doc.title));

        // Description
        if let Some(ref desc) = doc.description {
            output.push_str(&format!("{}\n\n", desc));
        }

        // Metadata
        if !doc.metadata.is_empty() {
            output.push_str("---\n\n");
        }

        // Cells
        for cell in &doc.cells {
            match cell.cell_type {
                CellType::Prose => {
                    output.push_str(&cell.content);
                    output.push_str("\n\n");
                }
                CellType::Code | CellType::Shell => {
                    let lang = cell.language.as_deref().unwrap_or("");
                    if self.include_execution_counts {
                        if let Some(count) = cell.execution_count {
                            output.push_str(&format!("*[{}]:*\n", count));
                        }
                    }
                    output.push_str(&format!("```{}\n", lang));
                    output.push_str(&cell.content);
                    output.push_str("\n```\n\n");

                    if self.include_outputs {
                        if let Some(ref out) = cell.output {
                            output.push_str("**Output:**\n```\n");
                            output.push_str(out);
                            output.push_str("\n```\n\n");
                        }
                    }
                }
                CellType::Data => {
                    let lang = cell.language.as_deref().unwrap_or("yaml");
                    output.push_str(&format!("```{}\n", lang));
                    output.push_str(&cell.content);
                    output.push_str("\n```\n\n");
                }
                CellType::Diagram => {
                    let lang = cell.language.as_deref().unwrap_or("mermaid");
                    output.push_str(&format!("```{}\n", lang));
                    output.push_str(&cell.content);
                    output.push_str("\n```\n\n");
                }
                CellType::Output => {
                    output.push_str("```\n");
                    output.push_str(&cell.content);
                    output.push_str("\n```\n\n");
                }
            }
        }

        output.trim().to_string()
    }

    /// Weave to HTML
    fn weave_html(&self, doc: &LiterateDocument) -> String {
        let mut output = String::new();

        output.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        output.push_str(&format!("<title>{}</title>\n", doc.title));
        output.push_str("<style>\n");
        output.push_str(
            "body { font-family: sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; }\n",
        );
        output.push_str(
            "pre { background: #f4f4f4; padding: 10px; border-radius: 4px; overflow-x: auto; }\n",
        );
        output.push_str("code { font-family: monospace; }\n");
        output.push_str(".output { background: #e8f4e8; }\n");
        output.push_str("</style>\n");
        output.push_str("</head>\n<body>\n");

        output.push_str(&format!("<h1>{}</h1>\n", doc.title));

        if let Some(ref desc) = doc.description {
            output.push_str(&format!("<p><em>{}</em></p>\n", desc));
        }

        for cell in &doc.cells {
            match cell.cell_type {
                CellType::Prose => {
                    output.push_str(&format!("<p>{}</p>\n", html_escape(&cell.content)));
                }
                CellType::Code | CellType::Shell => {
                    let lang = cell.language.as_deref().unwrap_or("code");
                    output.push_str(&format!("<pre><code class=\"language-{}\">\n", lang));
                    output.push_str(&html_escape(&cell.content));
                    output.push_str("</code></pre>\n");

                    if self.include_outputs {
                        if let Some(ref out) = cell.output {
                            output.push_str("<pre class=\"output\">\n");
                            output.push_str(&html_escape(out));
                            output.push_str("</pre>\n");
                        }
                    }
                }
                CellType::Data | CellType::Diagram => {
                    output.push_str("<pre>\n");
                    output.push_str(&html_escape(&cell.content));
                    output.push_str("</pre>\n");
                }
                CellType::Output => {
                    output.push_str("<pre class=\"output\">\n");
                    output.push_str(&html_escape(&cell.content));
                    output.push_str("</pre>\n");
                }
            }
        }

        output.push_str("</body>\n</html>");
        output
    }

    /// Weave to LaTeX
    fn weave_latex(&self, doc: &LiterateDocument) -> String {
        let mut output = String::new();

        output.push_str("\\documentclass{article}\n");
        output.push_str("\\usepackage{listings}\n");
        output.push_str("\\usepackage{xcolor}\n\n");
        output.push_str("\\lstset{basicstyle=\\ttfamily, breaklines=true, frame=single}\n\n");
        output.push_str("\\begin{document}\n\n");
        output.push_str(&format!("\\title{{{}}}\n", latex_escape(&doc.title)));
        if let Some(ref author) = doc.author {
            output.push_str(&format!("\\author{{{}}}\n", latex_escape(author)));
        }
        output.push_str("\\maketitle\n\n");

        if let Some(ref desc) = doc.description {
            output.push_str(&format!(
                "\\begin{{abstract}}\n{}\n\\end{{abstract}}\n\n",
                latex_escape(desc)
            ));
        }

        for cell in &doc.cells {
            match cell.cell_type {
                CellType::Prose => {
                    output.push_str(&latex_escape(&cell.content));
                    output.push_str("\n\n");
                }
                CellType::Code | CellType::Shell => {
                    let lang = cell.language.as_deref().unwrap_or("");
                    output.push_str(&format!("\\begin{{lstlisting}}[language={}]\n", lang));
                    output.push_str(&cell.content);
                    output.push_str("\n\\end{lstlisting}\n\n");
                }
                _ => {
                    output.push_str("\\begin{verbatim}\n");
                    output.push_str(&cell.content);
                    output.push_str("\n\\end{verbatim}\n\n");
                }
            }
        }

        output.push_str("\\end{document}");
        output
    }

    /// Weave to plain text
    fn weave_plaintext(&self, doc: &LiterateDocument) -> String {
        let mut output = String::new();

        output.push_str(&format!("{}\n", doc.title));
        output.push_str(&"=".repeat(doc.title.len()));
        output.push_str("\n\n");

        if let Some(ref desc) = doc.description {
            output.push_str(desc);
            output.push_str("\n\n");
        }

        for cell in &doc.cells {
            match cell.cell_type {
                CellType::Prose => {
                    output.push_str(&cell.content);
                    output.push_str("\n\n");
                }
                CellType::Code | CellType::Shell => {
                    output.push_str("---\n");
                    output.push_str(&cell.content);
                    output.push_str("\n---\n\n");

                    if self.include_outputs {
                        if let Some(ref out) = cell.output {
                            output.push_str("Output:\n");
                            output.push_str(out);
                            output.push_str("\n\n");
                        }
                    }
                }
                _ => {
                    output.push_str(&cell.content);
                    output.push_str("\n\n");
                }
            }
        }

        output.trim().to_string()
    }
}

/// Execution result
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Cell ID that was executed
    pub cell_id: String,
    /// Execution succeeded
    pub success: bool,
    /// Output from execution
    pub output: String,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub duration_ms: u64,
}

/// Cell executor for running code cells
#[derive(Debug)]
pub struct CellExecutor {
    /// Variables from previous executions
    pub variables: HashMap<String, String>,
    /// Execution history
    pub history: Vec<ExecutionResult>,
    /// Maximum execution time per cell
    pub timeout_ms: u64,
    /// Enable real execution (default: false for safety)
    pub live_execution: bool,
    /// Working directory for shell commands
    pub working_dir: Option<std::path::PathBuf>,
}

impl Default for CellExecutor {
    fn default() -> Self {
        Self {
            variables: HashMap::new(),
            history: Vec::new(),
            timeout_ms: 30_000,
            live_execution: false,
            working_dir: None,
        }
    }
}

impl CellExecutor {
    /// Create a new executor
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable live shell execution (use with caution)
    ///
    /// When enabled, shell cells will be executed via `sh -c`.
    /// Code cells are still simulated for safety.
    pub fn enable_live_execution(&mut self, working_dir: Option<std::path::PathBuf>) {
        self.live_execution = true;
        self.working_dir = working_dir;
    }

    /// Set execution timeout
    pub fn set_timeout(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    /// Execute a single cell
    ///
    /// For shell cells with `live_execution` enabled, runs actual commands.
    /// For code cells, returns simulated results (real execution requires
    /// language-specific runtimes).
    pub fn execute(&mut self, cell: &Cell) -> ExecutionResult {
        let start = std::time::Instant::now();

        if !cell.is_executable() {
            return ExecutionResult {
                cell_id: cell.id.clone(),
                success: false,
                output: String::new(),
                error: Some("Cell is not executable".to_string()),
                duration_ms: 0,
            };
        }

        // Simulate execution based on cell type and content
        let (success, output, error) = self.simulate_execution(cell);

        let result = ExecutionResult {
            cell_id: cell.id.clone(),
            success,
            output,
            error,
            duration_ms: start.elapsed().as_millis() as u64,
        };

        self.history.push(result.clone());
        result
    }

    /// Execute shell command for real (blocking)
    fn execute_shell_live(&self, command: &str) -> (bool, String, Option<String>) {
        use std::process::Command;

        // Safety check for dangerous commands
        let content_lower = command.to_lowercase();
        if content_lower.contains("rm -rf")
            || content_lower.contains("sudo")
            || content_lower.contains("> /dev/")
        {
            return (
                false,
                String::new(),
                Some("Potentially dangerous command blocked".to_string()),
            );
        }

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);

        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    (true, stdout, None)
                } else {
                    let error = if stderr.is_empty() {
                        format!("Exit code: {}", output.status.code().unwrap_or(-1))
                    } else {
                        stderr
                    };
                    (false, stdout, Some(error))
                }
            }
            Err(e) => (
                false,
                String::new(),
                Some(format!("Failed to execute: {}", e)),
            ),
        }
    }

    /// Simulate or execute based on mode and cell type
    fn simulate_execution(&mut self, cell: &Cell) -> (bool, String, Option<String>) {
        match cell.cell_type {
            CellType::Shell => {
                // Use real execution if enabled
                if self.live_execution {
                    return self.execute_shell_live(&cell.content);
                }

                // Otherwise simulate
                let content_lower = cell.content.to_lowercase();
                if content_lower.contains("rm -rf")
                    || content_lower.contains("sudo")
                    || content_lower.contains("> /dev/")
                {
                    return (
                        false,
                        String::new(),
                        Some("Potentially dangerous command blocked".to_string()),
                    );
                }

                // Simulate shell command output
                if cell.content.starts_with("echo ") {
                    let msg = cell.content.trim_start_matches("echo ").trim();
                    (true, msg.to_string(), None)
                } else if cell.content.contains("ls") {
                    (true, "file1.txt\nfile2.rs\nCargo.toml".to_string(), None)
                } else {
                    (
                        true,
                        format!(
                            "[Simulated output for: {}]",
                            cell.content.lines().next().unwrap_or("")
                        ),
                        None,
                    )
                }
            }
            CellType::Code => {
                // Code cells are always simulated (real execution requires language runtimes)
                let lang = cell.language.as_deref().unwrap_or("unknown");

                // Store variables defined in code
                for line in cell.content.lines() {
                    if let Some((var, val)) = self.parse_assignment(line, lang) {
                        self.variables.insert(var, val);
                    }
                }

                // Check for common patterns
                if cell.content.contains("print") || cell.content.contains("println") {
                    (true, "[Output from print statements]".to_string(), None)
                } else if cell.content.contains("error") || cell.content.contains("panic") {
                    (
                        false,
                        String::new(),
                        Some("Execution error (simulated)".to_string()),
                    )
                } else {
                    (true, format!("[Executed {} code successfully]", lang), None)
                }
            }
            _ => (true, String::new(), None),
        }
    }

    /// Parse variable assignment from code line
    fn parse_assignment(&self, line: &str, lang: &str) -> Option<(String, String)> {
        let line = line.trim();

        match lang {
            "rust" => {
                // let x = value;
                if line.starts_with("let ") {
                    if let Some((name, value)) = line
                        .trim_start_matches("let ")
                        .trim_start_matches("mut ")
                        .split_once('=')
                    {
                        let name = name.split(':').next()?.trim();
                        return Some((
                            name.to_string(),
                            value.trim().trim_end_matches(';').to_string(),
                        ));
                    }
                }
            }
            "python" => {
                // x = value
                if let Some((name, value)) = line.split_once('=') {
                    if !name.contains('(') && !name.contains('[') {
                        return Some((name.trim().to_string(), value.trim().to_string()));
                    }
                }
            }
            "javascript" | "typescript" => {
                // const/let/var x = value
                for prefix in &["const ", "let ", "var "] {
                    if line.starts_with(prefix) {
                        if let Some((name, value)) = line.trim_start_matches(prefix).split_once('=')
                        {
                            return Some((
                                name.trim().to_string(),
                                value.trim().trim_end_matches(';').to_string(),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }

        None
    }

    /// Execute all cells in a document
    pub fn execute_all(&mut self, doc: &mut LiterateDocument) -> Vec<ExecutionResult> {
        let mut results = Vec::new();

        for cell in &mut doc.cells {
            if cell.is_executable() {
                let result = self.execute(cell);
                if result.success {
                    cell.set_output(result.output.clone());
                }
                results.push(result);
            }
        }

        results
    }

    /// Get variable value
    pub fn get_variable(&self, name: &str) -> Option<&String> {
        self.variables.get(name)
    }

    /// Clear execution state
    pub fn clear(&mut self) {
        self.variables.clear();
        self.history.clear();
    }
}

/// Notebook workflow manager
#[derive(Debug)]
pub struct NotebookWorkflow {
    /// Current document
    pub document: LiterateDocument,
    /// Cell executor
    pub executor: CellExecutor,
    /// Current cell index
    pub current_cell: usize,
    /// Workflow state
    pub state: WorkflowState,
    /// Undo history
    undo_stack: Vec<LiterateDocument>,
    /// Maximum undo levels
    max_undo: usize,
}

/// Workflow execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowState {
    /// Ready to execute
    Ready,
    /// Currently executing
    Running,
    /// Paused mid-execution
    Paused,
    /// Completed execution
    Completed,
    /// Error encountered
    Error,
}

impl NotebookWorkflow {
    /// Create a new workflow from document
    pub fn new(document: LiterateDocument) -> Self {
        Self {
            document,
            executor: CellExecutor::new(),
            current_cell: 0,
            state: WorkflowState::Ready,
            undo_stack: Vec::new(),
            max_undo: 10,
        }
    }

    /// Create from markdown content
    pub fn from_markdown(content: &str, title: &str) -> Self {
        let parser = DocumentParser::new();
        let doc = parser.parse(content, title);
        Self::new(doc)
    }

    /// Execute next cell
    pub fn step(&mut self) -> Option<ExecutionResult> {
        if self.current_cell >= self.document.cells.len() {
            self.state = WorkflowState::Completed;
            return None;
        }

        self.state = WorkflowState::Running;
        let cell = &self.document.cells[self.current_cell];

        if cell.is_executable() {
            let result = self.executor.execute(cell);

            // Update cell output
            if result.success {
                if let Some(cell_mut) = self.document.cells.get_mut(self.current_cell) {
                    cell_mut.set_output(result.output.clone());
                }
            }

            self.current_cell += 1;

            if !result.success {
                self.state = WorkflowState::Error;
            } else if self.current_cell >= self.document.cells.len() {
                self.state = WorkflowState::Completed;
            } else {
                self.state = WorkflowState::Ready;
            }

            Some(result)
        } else {
            self.current_cell += 1;
            self.step() // Skip non-executable cells
        }
    }

    /// Execute all remaining cells
    pub fn run_all(&mut self) -> Vec<ExecutionResult> {
        let mut results = Vec::new();
        while let Some(result) = self.step() {
            results.push(result);
            if self.state == WorkflowState::Error {
                break;
            }
        }
        results
    }

    /// Reset to beginning
    pub fn reset(&mut self) {
        self.current_cell = 0;
        self.state = WorkflowState::Ready;
        self.executor.clear();

        // Clear cell outputs
        for cell in &mut self.document.cells {
            cell.output = None;
            cell.execution_count = None;
        }
    }

    /// Save current state for undo
    pub fn checkpoint(&mut self) {
        if self.undo_stack.len() >= self.max_undo {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(self.document.clone());
    }

    /// Undo last change
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.document = prev;
            true
        } else {
            false
        }
    }

    /// Add prose cell
    pub fn add_prose(&mut self, content: &str) {
        self.checkpoint();
        self.document.add_cell(Cell::prose(content));
    }

    /// Add code cell
    pub fn add_code(&mut self, content: &str, language: &str) {
        self.checkpoint();
        self.document.add_cell(Cell::code(content, language));
    }

    /// Get execution progress
    pub fn progress(&self) -> (usize, usize) {
        let total = self
            .document
            .cells
            .iter()
            .filter(|c| c.is_executable())
            .count();
        let completed = self.executor.history.len();
        (completed, total)
    }

    /// Export to markdown
    pub fn to_markdown(&self) -> String {
        let weaver = Weaver::new(WeaveFormat::Markdown);
        weaver.weave(&self.document)
    }

    /// Export code only
    pub fn to_code(&self) -> String {
        let tangler = Tangler::new();
        tangler.tangle(&self.document)
    }
}

// Helper functions

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escape LaTeX special characters
fn latex_escape(s: &str) -> String {
    s.replace('\\', "\\textbackslash{}")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('$', "\\$")
        .replace('%', "\\%")
        .replace('&', "\\&")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('^', "\\textasciicircum{}")
        .replace('~', "\\textasciitilde{}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_prose() {
        let cell = Cell::prose("This is some prose content.");
        assert_eq!(cell.cell_type, CellType::Prose);
        assert!(!cell.is_executable());
        assert!(cell.language.is_none());
    }

    #[test]
    fn test_cell_code() {
        let cell = Cell::code("fn main() {}", "rust");
        assert_eq!(cell.cell_type, CellType::Code);
        assert!(cell.is_executable());
        assert_eq!(cell.language, Some("rust".to_string()));
    }

    #[test]
    fn test_cell_shell() {
        let cell = Cell::code("echo hello", "bash");
        assert_eq!(cell.cell_type, CellType::Shell);
        assert!(cell.is_executable());
    }

    #[test]
    fn test_cell_set_output() {
        let mut cell = Cell::code("print('hi')", "python");
        cell.set_output("hi".to_string());
        assert_eq!(cell.output, Some("hi".to_string()));
        assert_eq!(cell.execution_count, Some(1));

        cell.set_output("hi again".to_string());
        assert_eq!(cell.execution_count, Some(2));
    }

    #[test]
    fn test_cell_type_from_language() {
        assert_eq!(CellType::from_language("rust"), CellType::Code);
        assert_eq!(CellType::from_language("python"), CellType::Code);
        assert_eq!(CellType::from_language("bash"), CellType::Shell);
        assert_eq!(CellType::from_language("sh"), CellType::Shell);
        assert_eq!(CellType::from_language("yaml"), CellType::Data);
        assert_eq!(CellType::from_language("json"), CellType::Data);
        assert_eq!(CellType::from_language("mermaid"), CellType::Diagram);
        assert_eq!(CellType::from_language(""), CellType::Prose);
    }

    #[test]
    fn test_document_new() {
        let doc = LiterateDocument::new("My Document");
        assert_eq!(doc.title, "My Document");
        assert!(doc.cells.is_empty());
        assert!(doc.created_at > 0);
    }

    #[test]
    fn test_document_add_cell() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("Introduction"));
        doc.add_cell(Cell::code("let x = 1;", "rust"));

        assert_eq!(doc.cells.len(), 2);
        assert_eq!(doc.cells[0].cell_type, CellType::Prose);
        assert_eq!(doc.cells[1].cell_type, CellType::Code);
    }

    #[test]
    fn test_document_remove_cell() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("First"));
        let cell_id = doc.cells[0].id.clone();

        let removed = doc.remove_cell(&cell_id);
        assert!(removed.is_some());
        assert!(doc.cells.is_empty());
    }

    #[test]
    fn test_document_code_cells() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("Text"));
        doc.add_cell(Cell::code("code1", "rust"));
        doc.add_cell(Cell::prose("More text"));
        doc.add_cell(Cell::code("code2", "python"));

        let code_cells = doc.code_cells();
        assert_eq!(code_cells.len(), 2);
    }

    #[test]
    fn test_parser_simple() {
        let parser = DocumentParser::new();
        let content = r#"# Heading

Some prose here.

```rust
fn main() {}
```

More text.
"#;

        let doc = parser.parse(content, "Test Doc");
        assert_eq!(doc.title, "Test Doc");
        assert_eq!(doc.cells.len(), 3);
        assert_eq!(doc.cells[0].cell_type, CellType::Prose);
        assert_eq!(doc.cells[1].cell_type, CellType::Code);
        assert_eq!(doc.cells[2].cell_type, CellType::Prose);
    }

    #[test]
    fn test_parser_multiple_code_blocks() {
        let parser = DocumentParser::new();
        let content = r#"
```python
print("hello")
```

```bash
echo "world"
```
"#;

        let doc = parser.parse(content, "Multi");
        assert_eq!(doc.cells.len(), 2);
        assert_eq!(doc.cells[0].language, Some("python".to_string()));
        assert_eq!(doc.cells[1].language, Some("bash".to_string()));
    }

    #[test]
    fn test_parser_with_metadata() {
        let parser = DocumentParser::new();
        let content = r#"---
title: My Title
author: John Doe
version: 2.0.0
---

Content here.
"#;

        let doc = parser.parse_with_metadata(content);
        assert_eq!(doc.title, "My Title");
        assert_eq!(doc.author, Some("John Doe".to_string()));
        assert_eq!(doc.version, "2.0.0");
    }

    #[test]
    fn test_tangler_basic() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("Description"));
        doc.add_cell(Cell::code("fn foo() {}", "rust"));
        doc.add_cell(Cell::code("fn bar() {}", "rust"));

        let tangler = Tangler::new();
        let code = tangler.tangle(&doc);

        assert!(code.contains("fn foo() {}"));
        assert!(code.contains("fn bar() {}"));
        assert!(!code.contains("Description"));
    }

    #[test]
    fn test_tangler_by_language() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::code("fn main() {}", "rust"));
        doc.add_cell(Cell::code("print('hi')", "python"));
        doc.add_cell(Cell::code("console.log('hi')", "javascript"));

        let tangler = Tangler::new();
        let by_lang = tangler.tangle_by_language(&doc);

        assert_eq!(by_lang.len(), 3);
        assert!(by_lang.get("rust").unwrap().contains("fn main"));
        assert!(by_lang.get("python").unwrap().contains("print"));
    }

    #[test]
    fn test_tangler_filter_language() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::code("fn main() {}", "rust"));
        doc.add_cell(Cell::code("print('hi')", "python"));

        let mut tangler = Tangler::new();
        tangler.filter_language("rust");
        let code = tangler.tangle(&doc);

        assert!(code.contains("fn main"));
        assert!(!code.contains("print"));
    }

    #[test]
    fn test_weaver_markdown() {
        let mut doc = LiterateDocument::new("My Doc");
        doc.description = Some("A description".to_string());
        doc.add_cell(Cell::prose("Introduction"));
        doc.add_cell(Cell::code("let x = 1;", "rust"));

        let weaver = Weaver::new(WeaveFormat::Markdown);
        let output = weaver.weave(&doc);

        assert!(output.contains("# My Doc"));
        assert!(output.contains("A description"));
        assert!(output.contains("Introduction"));
        assert!(output.contains("```rust"));
    }

    #[test]
    fn test_weaver_html() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("Hello <world>"));

        let weaver = Weaver::new(WeaveFormat::Html);
        let output = weaver.weave(&doc);

        assert!(output.contains("<!DOCTYPE html>"));
        assert!(output.contains("<title>Test</title>"));
        assert!(output.contains("&lt;world&gt;")); // Escaped
    }

    #[test]
    fn test_weaver_latex() {
        let mut doc = LiterateDocument::new("Test Doc");
        doc.add_cell(Cell::prose("Price is $100"));

        let weaver = Weaver::new(WeaveFormat::LaTeX);
        let output = weaver.weave(&doc);

        assert!(output.contains("\\documentclass"));
        assert!(output.contains("\\title{Test Doc}"));
        assert!(output.contains("\\$100")); // Escaped
    }

    #[test]
    fn test_weaver_plaintext() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("Some text"));
        doc.add_cell(Cell::code("code here", "rust"));

        let weaver = Weaver::new(WeaveFormat::PlainText);
        let output = weaver.weave(&doc);

        assert!(output.contains("Test"));
        assert!(output.contains("Some text"));
        assert!(output.contains("code here"));
    }

    #[test]
    fn test_executor_shell_echo() {
        let mut executor = CellExecutor::new();
        let cell = Cell::code("echo hello world", "bash");

        let result = executor.execute(&cell);
        assert!(result.success);
        assert_eq!(result.output, "hello world");
    }

    #[test]
    fn test_executor_dangerous_command() {
        let mut executor = CellExecutor::new();
        let cell = Cell::code("rm -rf /", "bash");

        let result = executor.execute(&cell);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_executor_rust_code() {
        let mut executor = CellExecutor::new();
        let cell = Cell::code("let x = 42;", "rust");

        let result = executor.execute(&cell);
        assert!(result.success);

        // Variable should be captured
        assert_eq!(executor.get_variable("x"), Some(&"42".to_string()));
    }

    #[test]
    fn test_executor_python_code() {
        let mut executor = CellExecutor::new();
        let cell = Cell::code("x = 'hello'", "python");

        let result = executor.execute(&cell);
        assert!(result.success);
        assert_eq!(executor.get_variable("x"), Some(&"'hello'".to_string()));
    }

    #[test]
    fn test_executor_non_executable() {
        let mut executor = CellExecutor::new();
        let cell = Cell::prose("Just prose");

        let result = executor.execute(&cell);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_notebook_workflow_new() {
        let doc = LiterateDocument::new("Test");
        let workflow = NotebookWorkflow::new(doc);

        assert_eq!(workflow.state, WorkflowState::Ready);
        assert_eq!(workflow.current_cell, 0);
    }

    #[test]
    fn test_notebook_from_markdown() {
        let content = r#"
# Hello

```rust
fn main() {}
```
"#;
        let workflow = NotebookWorkflow::from_markdown(content, "Test");
        assert_eq!(workflow.document.cells.len(), 2);
    }

    #[test]
    fn test_notebook_step() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::code("echo hi", "bash"));
        doc.add_cell(Cell::code("echo bye", "bash"));

        let mut workflow = NotebookWorkflow::new(doc);

        let result = workflow.step();
        assert!(result.is_some());
        assert!(result.unwrap().success);
        assert_eq!(workflow.current_cell, 1);

        let result = workflow.step();
        assert!(result.is_some());
        assert_eq!(workflow.state, WorkflowState::Completed);
    }

    #[test]
    fn test_notebook_run_all() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::code("echo a", "bash"));
        doc.add_cell(Cell::code("echo b", "bash"));

        let mut workflow = NotebookWorkflow::new(doc);
        let results = workflow.run_all();

        assert_eq!(results.len(), 2);
        assert_eq!(workflow.state, WorkflowState::Completed);
    }

    #[test]
    fn test_notebook_reset() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::code("echo x", "bash"));

        let mut workflow = NotebookWorkflow::new(doc);
        workflow.run_all();
        assert_eq!(workflow.state, WorkflowState::Completed);

        workflow.reset();
        assert_eq!(workflow.state, WorkflowState::Ready);
        assert_eq!(workflow.current_cell, 0);
    }

    #[test]
    fn test_notebook_progress() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::code("echo 1", "bash"));
        doc.add_cell(Cell::prose("text"));
        doc.add_cell(Cell::code("echo 2", "bash"));

        let mut workflow = NotebookWorkflow::new(doc);
        assert_eq!(workflow.progress(), (0, 2));

        workflow.step();
        assert_eq!(workflow.progress(), (1, 2));

        workflow.run_all();
        assert_eq!(workflow.progress(), (2, 2));
    }

    #[test]
    fn test_notebook_undo() {
        let doc = LiterateDocument::new("Test");
        let mut workflow = NotebookWorkflow::new(doc);

        workflow.add_prose("First");
        workflow.add_code("code", "rust");
        assert_eq!(workflow.document.cells.len(), 2);

        workflow.undo();
        assert_eq!(workflow.document.cells.len(), 1);

        workflow.undo();
        assert_eq!(workflow.document.cells.len(), 0);
    }

    #[test]
    fn test_notebook_export_markdown() {
        let mut doc = LiterateDocument::new("Export Test");
        doc.add_cell(Cell::prose("Hello"));
        doc.add_cell(Cell::code("print(1)", "python"));

        let workflow = NotebookWorkflow::new(doc);
        let md = workflow.to_markdown();

        assert!(md.contains("# Export Test"));
        assert!(md.contains("Hello"));
        assert!(md.contains("```python"));
    }

    #[test]
    fn test_notebook_export_code() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("Description"));
        doc.add_cell(Cell::code("fn main() {}", "rust"));

        let workflow = NotebookWorkflow::new(doc);
        let code = workflow.to_code();

        assert!(code.contains("fn main() {}"));
        assert!(!code.contains("Description"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn test_latex_escape() {
        assert_eq!(latex_escape("$100"), "\\$100");
        assert_eq!(latex_escape("50%"), "50\\%");
        assert_eq!(latex_escape("a_b"), "a\\_b");
    }

    #[test]
    fn test_cell_tags() {
        let mut cell = Cell::code("code", "rust");
        cell.tag("important");
        cell.tag("review");
        cell.tag("important"); // Duplicate

        assert_eq!(cell.tags.len(), 2);
        assert!(cell.tags.contains(&"important".to_string()));
        assert!(cell.tags.contains(&"review".to_string()));
    }

    #[test]
    fn test_cell_dependencies() {
        let mut cell = Cell::code("code", "rust");
        cell.depends("cell_1");
        cell.depends("cell_2");
        cell.depends("cell_1"); // Duplicate

        assert_eq!(cell.depends_on.len(), 2);
    }

    #[test]
    fn test_document_cell_counts() {
        let mut doc = LiterateDocument::new("Test");
        doc.add_cell(Cell::prose("a"));
        doc.add_cell(Cell::prose("b"));
        doc.add_cell(Cell::code("c", "rust"));
        doc.add_cell(Cell::code("d", "bash"));

        let counts = doc.cell_counts();
        assert_eq!(counts.get("Prose"), Some(&2));
        assert_eq!(counts.get("Code"), Some(&1));
        assert_eq!(counts.get("Shell"), Some(&1));
    }

    #[test]
    fn test_executor_history() {
        let mut executor = CellExecutor::new();
        executor.execute(&Cell::code("echo 1", "bash"));
        executor.execute(&Cell::code("echo 2", "bash"));

        assert_eq!(executor.history.len(), 2);
        assert!(executor.history[0].success);
        assert!(executor.history[1].success);
    }

    #[test]
    fn test_executor_clear() {
        let mut executor = CellExecutor::new();
        executor.execute(&Cell::code("let x = 1;", "rust"));

        assert!(!executor.variables.is_empty());
        assert!(!executor.history.is_empty());

        executor.clear();
        assert!(executor.variables.is_empty());
        assert!(executor.history.is_empty());
    }
}
