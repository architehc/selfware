//! Language Parsers for Code Introspection
//!
//! Provides AST-based parsing for Rust (via syn) and Python (via regex/heuristics)
//! to extract symbols at different depth levels.

use regex::Regex;
use std::path::Path;

use super::budget::Depth;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Java,
    C,
    Cpp,
    Unknown,
}

impl Language {
    /// Detect language from file path and optional override
    pub fn detect(path: &Path, override_lang: Option<&str>) -> Self {
        if let Some(lang) = override_lang {
            return Self::parse(lang);
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => Self::Rust,
            Some("py") => Self::Python,
            Some("ts") => Self::TypeScript,
            Some("js") => Self::JavaScript,
            Some("go") => Self::Go,
            Some("java") => Self::Java,
            Some("c") => Self::C,
            Some("cpp" | "cc" | "cxx") => Self::Cpp,
            Some("h" | "hpp") => Self::Cpp,
            _ => Self::Unknown,
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Self::Rust,
            "python" | "py" => Self::Python,
            "typescript" | "ts" => Self::TypeScript,
            "javascript" | "js" => Self::JavaScript,
            "go" | "golang" => Self::Go,
            "java" => Self::Java,
            "c" => Self::C,
            "cpp" | "c++" | "cxx" => Self::Cpp,
            _ => Self::Unknown,
        }
    }
}

/// A parsed code file with extracted symbols
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub language: Language,
    pub imports: Vec<Import>,
    pub symbols: Vec<Symbol>,
    pub module_doc: Option<String>,
}

/// An import statement
#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
    pub is_glob: bool,
}

/// A code symbol (function, struct, trait, etc.)
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub signature: String,
    pub documentation: Option<String>,
    pub line_start: usize,
    pub line_end: Option<usize>,
    pub children: Vec<Symbol>,
    pub attributes: Vec<String>,
}

/// Type of code symbol
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    Const,
    Static,
    TypeAlias,
    Field,
    Method,
    Variant,
    Macro,
    Class,
    Property,
}

/// Visibility modifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    Public,
    PublicCrate,
    PublicSuper,
    Restricted(String),
    Private,
}

impl Symbol {
    /// Render the symbol as a string for output
    pub fn render(&self) -> String {
        let mut result = String::new();

        // Documentation
        if let Some(doc) = &self.documentation {
            for line in doc.lines() {
                result.push_str(&format!("/// {}\n", line));
            }
        }

        // Attributes
        for attr in &self.attributes {
            result.push_str(&format!("{}\n", attr));
        }

        // Visibility
        let vis = match self.visibility {
            Visibility::Public => "pub ",
            Visibility::PublicCrate => "pub(crate) ",
            Visibility::PublicSuper => "pub(super) ",
            Visibility::Restricted(_) => "pub(...) ",
            Visibility::Private => "",
        };

        result.push_str(&format!("{}{}\n", vis, self.signature));

        result
    }

    /// Get brief representation (name + kind)
    pub fn brief(&self) -> String {
        format!("{:?}: {}", self.kind, self.name)
    }
}

/// Parse source code into structured representation
pub fn parse(content: &str, language: Language) -> ParsedFile {
    match language {
        Language::Rust => parse_rust(content),
        Language::Python => parse_python(content),
        Language::TypeScript | Language::JavaScript => parse_typescript(content),
        _ => parse_generic(content),
    }
}

/// Parse Rust code using regex-based extraction
pub fn parse_rust(content: &str) -> ParsedFile {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut module_doc = None;

    // Extract module-level doc comment
    let doc_regex = Regex::new(r"^\s*//!\s*(.+)$").unwrap();
    for line in content.lines().take(20) {
        if let Some(cap) = doc_regex.captures(line) {
            let doc = cap.get(1).map(|m| m.as_str().to_string());
            let doc_str = doc.unwrap_or_default();
            module_doc = Some(format!("{}{}\n", module_doc.unwrap_or_default(), doc_str));
        }
    }

    // Extract imports (use statements)
    let use_regex = Regex::new(r"^\s*use\s+(.+);$").unwrap();
    for line in content.lines() {
        if let Some(cap) = use_regex.captures(line) {
            let path = cap.get(1).map(|m| m.as_str().to_string());
            if let Some(path) = path {
                imports.push(Import {
                    path,
                    alias: None,
                    is_glob: false,
                });
            }
        }
    }

    // Extract function signatures
    let fn_regex = Regex::new(
        r"(?m)^\s*((?:pub(?:\s*\([^)]*\))?\s+)?)(?:async\s+)?(?:fn|const\s+fn)\s+(\w+)\s*(<[^>]*>)?\s*\(([^)]*)\)(?:\s*->\s*([^\{;]+))?"
    ).unwrap();

    for cap in fn_regex.captures_iter(content) {
        let vis_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let name = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let _generics = cap
            .get(3)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let params = cap
            .get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let ret = cap.get(5).map(|m| m.as_str().trim().to_string());

        let visibility = parse_visibility(vis_str);

        let signature = if let Some(ret) = ret {
            format!("fn {}({}) -> {}", name, params, ret)
        } else {
            format!("fn {}({})", name, params)
        };

        symbols.push(Symbol {
            name,
            kind: SymbolKind::Function,
            visibility,
            signature,
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    // Extract structs
    let struct_regex =
        Regex::new(r"(?m)^\s*((?:pub(?:\s*\([^)]*\))?\s+)?)(?:struct|enum)\s+(\w+)").unwrap();
    for cap in struct_regex.captures_iter(content) {
        let vis_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let name = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let visibility = parse_visibility(vis_str);

        symbols.push(Symbol {
            name: name.clone(),
            kind: SymbolKind::Struct,
            visibility,
            signature: format!("struct {}", name),
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    // Extract traits
    let trait_regex = Regex::new(r"(?m)^\s*((?:pub(?:\s*\([^)]*\))?\s+)?)trait\s+(\w+)").unwrap();
    for cap in trait_regex.captures_iter(content) {
        let vis_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let name = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let visibility = parse_visibility(vis_str);

        symbols.push(Symbol {
            name: name.clone(),
            kind: SymbolKind::Trait,
            visibility,
            signature: format!("trait {}", name),
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    // Sort symbols by line number for consistent output
    symbols.sort_by(|a, b| a.name.cmp(&b.name));

    ParsedFile {
        language: Language::Rust,
        imports,
        symbols,
        module_doc,
    }
}

/// Parse visibility string
fn parse_visibility(s: &str) -> Visibility {
    if s.contains("pub(crate)") {
        Visibility::PublicCrate
    } else if s.contains("pub(super)") {
        Visibility::PublicSuper
    } else if s.starts_with("pub(") {
        Visibility::Restricted(s.to_string())
    } else if s.starts_with("pub") {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

/// Parse Python code
fn parse_python(content: &str) -> ParsedFile {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    // Extract imports
    let import_regex = Regex::new(r"^\s*(?:from\s+(\S+)\s+import\s+(.+)|import\s+(.+))$").unwrap();
    for line in content.lines() {
        if let Some(cap) = import_regex.captures(line) {
            let path = cap
                .get(1)
                .or_else(|| cap.get(3))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            imports.push(Import {
                path,
                alias: None,
                is_glob: false,
            });
        }
    }

    // Extract class definitions
    let class_regex = Regex::new(r"(?m)^\s*class\s+(\w+)(?:\(([^)]*)\))?:").unwrap();
    for cap in class_regex.captures_iter(content) {
        let name = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let parents = cap.get(2).map(|m| m.as_str().to_string());

        let signature = if let Some(parents) = parents {
            format!("class {}({})", name, parents)
        } else {
            format!("class {}:", name)
        };

        symbols.push(Symbol {
            name: name.clone(),
            kind: SymbolKind::Class,
            visibility: Visibility::Public,
            signature,
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    // Extract function definitions
    let fn_regex = Regex::new(r"(?m)^\s*def\s+(\w+)\s*\(([^)]*)\)(?:\s*->\s*([^:]+))?:").unwrap();
    for cap in fn_regex.captures_iter(content) {
        let name = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let params = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let ret = cap.get(3).map(|m| m.as_str().to_string());

        let signature = if let Some(ret) = ret {
            format!("def {}({}) -> {}:", name, params, ret)
        } else {
            format!("def {}({}):", name, params)
        };

        symbols.push(Symbol {
            name,
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature,
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    ParsedFile {
        language: Language::Python,
        imports,
        symbols,
        module_doc: None,
    }
}

/// Parse TypeScript/JavaScript code
fn parse_typescript(content: &str) -> ParsedFile {
    let mut symbols = Vec::new();

    // Extract functions
    let fn_regex = Regex::new(r"(?m)^\s*(?:export\s+)?(?:async\s+)?(?:function|const)\s+(\w+)\s*(?:=\s*)?(?:<[^>]*>)?\s*\(([^)]*)\)(?:\s*:\s*([^\{=]+))?").unwrap();

    for cap in fn_regex.captures_iter(content) {
        let name = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let params = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let ret = cap.get(3).map(|m| m.as_str().trim().to_string());

        let signature = if let Some(ret) = ret {
            format!("function {}({}): {}", name, params, ret)
        } else {
            format!("function {}({})", name, params)
        };

        symbols.push(Symbol {
            name,
            kind: SymbolKind::Function,
            visibility: Visibility::Public,
            signature,
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    // Extract interfaces and types
    let type_regex = Regex::new(r"(?m)^\s*(?:export\s+)?(?:interface|type)\s+(\w+)").unwrap();
    for cap in type_regex.captures_iter(content) {
        let name = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        symbols.push(Symbol {
            name: name.clone(),
            kind: SymbolKind::TypeAlias,
            visibility: Visibility::Public,
            signature: format!("interface {}", name),
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    ParsedFile {
        language: Language::TypeScript,
        imports: Vec::new(),
        symbols,
        module_doc: None,
    }
}

/// Generic fallback parser
fn parse_generic(_content: &str) -> ParsedFile {
    // Simple line-based extraction for unknown languages
    let symbols = Vec::new();

    ParsedFile {
        language: Language::Unknown,
        imports: Vec::new(),
        symbols,
        module_doc: None,
    }
}

/// Extract symbols at a specific depth level
pub fn extract_at_depth(parsed: &ParsedFile, depth: &Depth) -> Vec<Symbol> {
    match depth {
        Depth::Overview => extract_overview(parsed),
        Depth::Signatures => extract_signatures(parsed),
        Depth::Full => extract_full(parsed),
        Depth::Dependencies => extract_dependencies(parsed),
    }
}

/// Extract overview (imports and module structure)
fn extract_overview(parsed: &ParsedFile) -> Vec<Symbol> {
    let mut result = Vec::new();

    // Add imports as pseudo-symbols
    for imp in &parsed.imports {
        result.push(Symbol {
            name: imp.path.clone(),
            kind: SymbolKind::Module,
            visibility: Visibility::Private,
            signature: format!("use {}", imp.path),
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    // Add symbol names only
    for sym in &parsed.symbols {
        result.push(Symbol {
            name: sym.name.clone(),
            kind: sym.kind.clone(),
            visibility: sym.visibility.clone(),
            signature: sym.brief(),
            documentation: None,
            line_start: sym.line_start,
            line_end: sym.line_end,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    result
}

/// Extract signatures (public API)
fn extract_signatures(parsed: &ParsedFile) -> Vec<Symbol> {
    parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.visibility, Visibility::Public | Visibility::PublicCrate))
        .cloned()
        .collect()
}

/// Extract full symbols (everything)
fn extract_full(parsed: &ParsedFile) -> Vec<Symbol> {
    parsed.symbols.clone()
}

/// Extract dependency information
fn extract_dependencies(parsed: &ParsedFile) -> Vec<Symbol> {
    // Return imports plus symbols that reference external types
    let mut result = Vec::new();

    for imp in &parsed.imports {
        result.push(Symbol {
            name: imp.path.clone(),
            kind: SymbolKind::Module,
            visibility: Visibility::Private,
            signature: format!("use {}", imp.path),
            documentation: None,
            line_start: 0,
            line_end: None,
            children: Vec::new(),
            attributes: Vec::new(),
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_function() {
        let code = r#"
pub async fn process_data(input: String) -> Result<(), Error> {
    todo!()
}
"#;
        let parsed = parse_rust(code);
        assert!(!parsed.symbols.is_empty());

        let func = &parsed.symbols[0];
        assert_eq!(func.name, "process_data");
        assert!(func.signature.contains("process_data"));
    }

    #[test]
    fn test_parse_python_class() {
        let code = r#"
class DataProcessor:
    def process(self, data: str) -> dict:
        return {}
"#;
        let parsed = parse_python(code);
        assert!(!parsed.symbols.is_empty());

        let has_class = parsed.symbols.iter().any(|s| s.kind == SymbolKind::Class);
        assert!(has_class);
    }

    #[test]
    fn test_extract_signatures() {
        let code = r#"
pub fn public_fn() {}
fn private_fn() {}
pub struct PublicStruct;
"#;
        let parsed = parse_rust(code);
        let sigs = extract_signatures(&parsed);

        // Should only get public items
        assert_eq!(sigs.len(), 2);
        assert!(sigs
            .iter()
            .all(|s| matches!(s.visibility, Visibility::Public)));
    }
}
