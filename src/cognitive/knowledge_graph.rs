//! Semantic import extraction for cross-file reference analysis

use std::collections::HashMap;

/// Semantic linker for cross-file references
pub struct SemanticLinker {
    /// Pre-compiled import regex patterns by language
    import_patterns: HashMap<String, Vec<regex::Regex>>,
}

impl std::fmt::Debug for SemanticLinker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let readable: HashMap<_, Vec<_>> = self
            .import_patterns
            .iter()
            .map(|(k, v)| (k.as_str(), v.iter().map(|r| r.as_str()).collect()))
            .collect();
        f.debug_struct("SemanticLinker")
            .field("import_patterns", &readable)
            .finish()
    }
}

impl Default for SemanticLinker {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticLinker {
    /// Compile a regex pattern. All patterns here are compile-time literals,
    /// so failure indicates a programming error.
    fn compile(pattern: &str) -> regex::Regex {
        regex::Regex::new(pattern).expect("invalid regex literal in SemanticLinker")
    }

    /// Create new linker with pre-compiled regex patterns
    pub fn new() -> Self {
        let mut patterns: HashMap<String, Vec<regex::Regex>> = HashMap::new();

        patterns.insert(
            "rust".to_string(),
            vec![
                Self::compile(r"use\s+(\w+(?:::\w+)*)"),
                Self::compile(r"crate::(\w+(?:::\w+)*)"),
                Self::compile(r"super::(\w+(?:::\w+)*)"),
            ],
        );

        patterns.insert(
            "javascript".to_string(),
            vec![
                Self::compile(r#"import\s+.*\s+from\s+['"](.+)['"]"#),
                Self::compile(r#"require\(['"](.+)['"]\)"#),
            ],
        );

        patterns.insert(
            "python".to_string(),
            vec![
                Self::compile(r"from\s+(\w+(?:\.\w+)*)\s+import"),
                Self::compile(r"import\s+(\w+(?:\.\w+)*)"),
            ],
        );

        Self {
            import_patterns: patterns,
        }
    }

    /// Extract imports from code using pre-compiled regex patterns
    pub fn extract_imports(&self, content: &str, language: &str) -> Vec<String> {
        let patterns = match self.import_patterns.get(language) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut imports = Vec::new();

        for re in patterns {
            for cap in re.captures_iter(content) {
                if let Some(m) = cap.get(1) {
                    imports.push(m.as_str().to_string());
                }
            }
        }

        imports
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/knowledge_graph/knowledge_graph_test.rs"]
mod tests;
