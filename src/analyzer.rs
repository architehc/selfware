//! Error Analysis & Recovery - Automatic fix suggestions for common errors
//!
//! Analyzes compiler errors and provides:
//! - Categorization by error type
//! - Prioritization (type errors before unused warnings)
//! - Automatic fix suggestions for common patterns
//! - Grouping of related errors

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type for raw error input to batch analysis
pub type RawError<'a> = (Option<&'a str>, &'a str, &'a str, Option<u32>, Option<u32>);

/// Analyzed error with fix suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedError {
    /// Original error code (e.g., E0425)
    pub code: Option<String>,
    /// Error message
    pub message: String,
    /// File path
    pub file: String,
    /// Line number
    pub line: Option<u32>,
    /// Column number
    pub column: Option<u32>,
    /// Error category
    pub category: ErrorCategory,
    /// Severity/priority (lower = fix first)
    pub priority: u8,
    /// Suggested fix
    pub suggestion: Option<FixSuggestion>,
    /// Related errors that may be caused by this one
    pub related_errors: Vec<String>,
}

/// Categories of errors for prioritization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Type errors (highest priority)
    TypeError,
    /// Unresolved imports/paths
    UnresolvedImport,
    /// Borrow checker errors
    BorrowError,
    /// Lifetime errors
    LifetimeError,
    /// Trait not implemented
    TraitError,
    /// Missing or extra arguments
    ArgumentError,
    /// Pattern matching errors
    PatternError,
    /// Unused code warnings
    UnusedWarning,
    /// Style/lint warnings
    StyleWarning,
    /// Other errors
    Other,
}

impl ErrorCategory {
    /// Get the priority for this category (lower = fix first)
    pub fn priority(&self) -> u8 {
        match self {
            Self::TypeError => 1,
            Self::UnresolvedImport => 2,
            Self::BorrowError => 3,
            Self::LifetimeError => 4,
            Self::TraitError => 5,
            Self::ArgumentError => 6,
            Self::PatternError => 7,
            Self::UnusedWarning => 20,
            Self::StyleWarning => 30,
            Self::Other => 10,
        }
    }
}

/// A suggested fix for an error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixSuggestion {
    /// Description of what the fix does
    pub description: String,
    /// The actual code change (if deterministic)
    pub fix_code: Option<String>,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f32,
    /// Whether this fix can be applied automatically
    pub auto_fixable: bool,
    /// Additional context or instructions
    pub notes: Option<String>,
}

/// Error pattern matcher
pub struct ErrorAnalyzer {
    /// Known error patterns and their fixes
    patterns: Vec<ErrorPattern>,
}

/// A pattern that matches errors and suggests fixes
struct ErrorPattern {
    /// Error code(s) this pattern matches
    codes: Vec<&'static str>,
    /// Message substring to match (optional)
    message_contains: Option<&'static str>,
    /// Category for matched errors
    category: ErrorCategory,
    /// Function to generate fix suggestion
    suggest_fix: fn(&str, &str) -> Option<FixSuggestion>,
}

impl Default for ErrorAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorAnalyzer {
    pub fn new() -> Self {
        Self {
            patterns: Self::build_patterns(),
        }
    }

    /// Analyze an error and provide suggestions
    pub fn analyze(
        &self,
        code: Option<&str>,
        message: &str,
        file: &str,
        line: Option<u32>,
        column: Option<u32>,
    ) -> AnalyzedError {
        let (category, suggestion) = self.match_error(code, message);

        AnalyzedError {
            code: code.map(|s| s.to_string()),
            message: message.to_string(),
            file: file.to_string(),
            line,
            column,
            category,
            priority: category.priority(),
            suggestion,
            related_errors: vec![],
        }
    }

    /// Analyze multiple errors and prioritize them
    pub fn analyze_batch(&self, errors: &[RawError<'_>]) -> Vec<AnalyzedError> {
        let mut analyzed: Vec<AnalyzedError> = errors
            .iter()
            .map(|(code, msg, file, line, col)| self.analyze(*code, msg, file, *line, *col))
            .collect();

        // Sort by priority (lower first)
        analyzed.sort_by_key(|e| e.priority);

        // Mark related errors
        self.find_related_errors(&mut analyzed);

        analyzed
    }

    /// Group errors by category
    pub fn group_by_category<'a>(
        &self,
        errors: &'a [AnalyzedError],
    ) -> HashMap<ErrorCategory, Vec<&'a AnalyzedError>> {
        let mut groups: HashMap<ErrorCategory, Vec<&AnalyzedError>> = HashMap::new();

        for error in errors {
            groups.entry(error.category).or_default().push(error);
        }

        groups
    }

    /// Get the most important error to fix first
    pub fn first_to_fix<'a>(&self, errors: &'a [AnalyzedError]) -> Option<&'a AnalyzedError> {
        errors.iter().min_by_key(|e| e.priority)
    }

    /// Match an error against known patterns
    fn match_error(
        &self,
        code: Option<&str>,
        message: &str,
    ) -> (ErrorCategory, Option<FixSuggestion>) {
        for pattern in &self.patterns {
            // Check code match
            let code_matches = code.map(|c| pattern.codes.contains(&c)).unwrap_or(false);

            // Check message match
            let message_matches = pattern
                .message_contains
                .map(|s| message.to_lowercase().contains(&s.to_lowercase()))
                .unwrap_or(true);

            if code_matches || (pattern.codes.is_empty() && message_matches) {
                let suggestion = (pattern.suggest_fix)(code.unwrap_or(""), message);
                return (pattern.category, suggestion);
            }
        }

        (ErrorCategory::Other, None)
    }

    /// Find and mark related errors
    fn find_related_errors(&self, errors: &mut [AnalyzedError]) {
        // E0433 (unresolved import) often causes E0412 (cannot find type)
        // E0425 (cannot find value) may be related to E0433
        // etc.

        let unresolved_modules: Vec<String> = errors
            .iter()
            .filter(|e| e.code.as_deref() == Some("E0433"))
            .filter_map(|e| extract_module_name(&e.message))
            .collect();

        for error in errors.iter_mut() {
            if let Some(ref code) = error.code {
                if code == "E0412" || code == "E0425" {
                    // Check if this might be caused by an unresolved import
                    for module in &unresolved_modules {
                        if error.message.contains(module) {
                            error
                                .related_errors
                                .push(format!("May be caused by unresolved import of {}", module));
                        }
                    }
                }
            }
        }
    }

    /// Build the pattern database
    fn build_patterns() -> Vec<ErrorPattern> {
        vec![
            // E0425: Cannot find value
            ErrorPattern {
                codes: vec!["E0425"],
                message_contains: Some("cannot find value"),
                category: ErrorCategory::UnresolvedImport,
                suggest_fix: |_code, message| {
                    let name = extract_identifier(message, "cannot find value `", "`");
                    Some(FixSuggestion {
                        description: format!("Cannot find value '{}'. Check for typos, missing imports, or scope issues.", name.unwrap_or("unknown")),
                        fix_code: None,
                        confidence: 0.7,
                        auto_fixable: false,
                        notes: Some("Common fixes:\n1. Check spelling of variable/function name\n2. Add 'use' statement if it's from another module\n3. Check if the item is public".to_string()),
                    })
                },
            },
            // E0433: Unresolved import
            ErrorPattern {
                codes: vec!["E0433"],
                message_contains: Some("unresolved import"),
                category: ErrorCategory::UnresolvedImport,
                suggest_fix: |_code, message| {
                    let module = extract_identifier(message, "unresolved import `", "`");
                    Some(FixSuggestion {
                        description: format!("Module or item '{}' not found.", module.unwrap_or("unknown")),
                        fix_code: None,
                        confidence: 0.8,
                        auto_fixable: false,
                        notes: Some("Check:\n1. Is the dependency in Cargo.toml?\n2. Is the module path correct?\n3. Is the item re-exported?".to_string()),
                    })
                },
            },
            // E0382: Use of moved value
            ErrorPattern {
                codes: vec!["E0382"],
                message_contains: Some("use of moved value"),
                category: ErrorCategory::BorrowError,
                suggest_fix: |_code, message| {
                    let var = extract_identifier(message, "value: `", "`");
                    Some(FixSuggestion {
                        description: format!("Value '{}' was moved and cannot be used again.", var.unwrap_or("unknown")),
                        fix_code: Some(".clone()".to_string()),
                        confidence: 0.6,
                        auto_fixable: false,
                        notes: Some("Options:\n1. Add .clone() before the move\n2. Use a reference instead\n3. Restructure to avoid the double use".to_string()),
                    })
                },
            },
            // E0502: Cannot borrow as mutable
            ErrorPattern {
                codes: vec!["E0502"],
                message_contains: Some("cannot borrow"),
                category: ErrorCategory::BorrowError,
                suggest_fix: |_code, _message| {
                    Some(FixSuggestion {
                        description: "Cannot have mutable and immutable borrows simultaneously.".to_string(),
                        fix_code: None,
                        confidence: 0.5,
                        auto_fixable: false,
                        notes: Some("Options:\n1. Use separate scopes for borrows\n2. Clone the data\n3. Use Cell/RefCell for interior mutability".to_string()),
                    })
                },
            },
            // E0599: No method found
            ErrorPattern {
                codes: vec!["E0599"],
                message_contains: Some("no method named"),
                category: ErrorCategory::TraitError,
                suggest_fix: |_code, message| {
                    let method = extract_identifier(message, "no method named `", "`");
                    Some(FixSuggestion {
                        description: format!("Method '{}' not found on this type.", method.unwrap_or("unknown")),
                        fix_code: None,
                        confidence: 0.7,
                        auto_fixable: false,
                        notes: Some("Check:\n1. Is the trait in scope? (add 'use' statement)\n2. Does the type implement this trait?\n3. Is the method name spelled correctly?".to_string()),
                    })
                },
            },
            // E0308: Mismatched types
            ErrorPattern {
                codes: vec!["E0308"],
                message_contains: Some("mismatched types"),
                category: ErrorCategory::TypeError,
                suggest_fix: |_code, message| {
                    let expected = extract_between(message, "expected `", "`");
                    let found = extract_between(message, "found `", "`");
                    Some(FixSuggestion {
                        description: format!(
                            "Type mismatch: expected '{}', found '{}'",
                            expected.as_deref().unwrap_or("?"),
                            found.as_deref().unwrap_or("?")
                        ),
                        fix_code: None,
                        confidence: 0.8,
                        auto_fixable: false,
                        notes: Some("Check:\n1. Return type annotations\n2. Variable type annotations\n3. Function argument types".to_string()),
                    })
                },
            },
            // E0277: Trait not satisfied
            ErrorPattern {
                codes: vec!["E0277"],
                message_contains: Some("the trait bound"),
                category: ErrorCategory::TraitError,
                suggest_fix: |_code, message| {
                    let trait_name = extract_between(message, ": `", "`");
                    Some(FixSuggestion {
                        description: format!("Trait '{}' is not implemented.", trait_name.as_deref().unwrap_or("unknown")),
                        fix_code: None,
                        confidence: 0.6,
                        auto_fixable: false,
                        notes: Some("Options:\n1. Derive the trait: #[derive(...)]\n2. Implement the trait manually\n3. Use a different type that implements the trait".to_string()),
                    })
                },
            },
            // E0412: Cannot find type
            ErrorPattern {
                codes: vec!["E0412"],
                message_contains: Some("cannot find type"),
                category: ErrorCategory::UnresolvedImport,
                suggest_fix: |_code, message| {
                    let type_name = extract_identifier(message, "cannot find type `", "`");
                    Some(FixSuggestion {
                        description: format!("Type '{}' not found.", type_name.unwrap_or("unknown")),
                        fix_code: None,
                        confidence: 0.8,
                        auto_fixable: false,
                        notes: Some("Check:\n1. Add 'use' statement for the type\n2. Check if the type is defined\n3. Check spelling".to_string()),
                    })
                },
            },
            // E0061: Wrong number of arguments
            ErrorPattern {
                codes: vec!["E0061"],
                message_contains: Some("argument"),
                category: ErrorCategory::ArgumentError,
                suggest_fix: |_code, message| {
                    Some(FixSuggestion {
                        description: "Function called with wrong number of arguments.".to_string(),
                        fix_code: None,
                        confidence: 0.9,
                        auto_fixable: false,
                        notes: Some(format!("Message: {}", message)),
                    })
                },
            },
            // E0106: Missing lifetime specifier
            ErrorPattern {
                codes: vec!["E0106"],
                message_contains: Some("missing lifetime"),
                category: ErrorCategory::LifetimeError,
                suggest_fix: |_code, _message| {
                    Some(FixSuggestion {
                        description: "Reference is missing a lifetime specifier.".to_string(),
                        fix_code: Some("<'a>".to_string()),
                        confidence: 0.7,
                        auto_fixable: false,
                        notes: Some("Add a lifetime parameter like 'a to the reference and surrounding struct/function.".to_string()),
                    })
                },
            },
            // Unused warnings
            ErrorPattern {
                codes: vec![],
                message_contains: Some("unused"),
                category: ErrorCategory::UnusedWarning,
                suggest_fix: |_code, message| {
                    let suggestion = if message.contains("unused variable") {
                        "Prefix with underscore: _variable"
                    } else if message.contains("unused import") {
                        "Remove the unused import"
                    } else {
                        "Remove or use the item"
                    };
                    Some(FixSuggestion {
                        description: suggestion.to_string(),
                        fix_code: None,
                        confidence: 0.9,
                        auto_fixable: true,
                        notes: None,
                    })
                },
            },
            // Dead code warning
            ErrorPattern {
                codes: vec![],
                message_contains: Some("dead_code"),
                category: ErrorCategory::UnusedWarning,
                suggest_fix: |_code, _message| {
                    Some(FixSuggestion {
                        description: "Code is never used.".to_string(),
                        fix_code: Some("#[allow(dead_code)]".to_string()),
                        confidence: 0.8,
                        auto_fixable: true,
                        notes: Some(
                            "Remove the code, or add #[allow(dead_code)] if intentional."
                                .to_string(),
                        ),
                    })
                },
            },
        ]
    }

    /// Generate a summary report
    pub fn summary(&self, errors: &[AnalyzedError]) -> String {
        let groups = self.group_by_category(errors);
        let mut lines = vec!["=== Error Analysis Summary ===".to_string()];

        let total = errors.len();
        let with_fix = errors.iter().filter(|e| e.suggestion.is_some()).count();
        let auto_fixable = errors
            .iter()
            .filter(|e| {
                e.suggestion
                    .as_ref()
                    .map(|s| s.auto_fixable)
                    .unwrap_or(false)
            })
            .count();

        lines.push(format!("Total errors: {}", total));
        lines.push(format!("With suggestions: {}", with_fix));
        lines.push(format!("Auto-fixable: {}", auto_fixable));
        lines.push(String::new());

        // By category
        lines.push("By category:".to_string());
        for (category, errs) in groups {
            lines.push(format!("  {:?}: {}", category, errs.len()));
        }

        // First to fix
        if let Some(first) = self.first_to_fix(errors) {
            lines.push(String::new());
            lines.push("Fix first:".to_string());
            lines.push(format!("  {} ({})", first.message, first.file));
            if let Some(ref suggestion) = first.suggestion {
                lines.push(format!("  Suggestion: {}", suggestion.description));
            }
        }

        lines.join("\n")
    }
}

/// Extract a module name from an error message
fn extract_module_name(message: &str) -> Option<String> {
    extract_between(message, "`", "`")
}

/// Extract an identifier from a message between markers
fn extract_identifier<'a>(message: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = message.find(prefix)? + prefix.len();
    let end = message[start..].find(suffix)? + start;
    Some(&message[start..end])
}

/// Extract text between two markers
fn extract_between(message: &str, start_marker: &str, end_marker: &str) -> Option<String> {
    let start = message.find(start_marker)? + start_marker.len();
    let end = message[start..].find(end_marker)? + start;
    Some(message[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_analyzer_new() {
        let analyzer = ErrorAnalyzer::new();
        assert!(!analyzer.patterns.is_empty());
    }

    #[test]
    fn test_analyze_e0425() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0425"),
            "cannot find value `foo` in this scope",
            "src/main.rs",
            Some(10),
            Some(5),
        );

        assert_eq!(error.category, ErrorCategory::UnresolvedImport);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_analyze_e0308() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0308"),
            "mismatched types: expected `String`, found `&str`",
            "src/lib.rs",
            Some(20),
            None,
        );

        assert_eq!(error.category, ErrorCategory::TypeError);
        assert_eq!(error.priority, 1); // Highest priority
    }

    #[test]
    fn test_analyze_e0382() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0382"),
            "use of moved value: `data`",
            "src/lib.rs",
            Some(15),
            None,
        );

        assert_eq!(error.category, ErrorCategory::BorrowError);
        assert!(error.suggestion.is_some());
        assert!(error.suggestion.unwrap().notes.unwrap().contains("clone"));
    }

    #[test]
    fn test_analyze_batch_prioritizes() {
        let analyzer = ErrorAnalyzer::new();
        let errors = analyzer.analyze_batch(&[
            (None, "unused variable: `x`", "src/main.rs", Some(5), None),
            (
                Some("E0308"),
                "mismatched types",
                "src/main.rs",
                Some(10),
                None,
            ),
            (
                Some("E0433"),
                "unresolved import",
                "src/main.rs",
                Some(1),
                None,
            ),
        ]);

        // Type error should be first
        assert_eq!(errors[0].code.as_deref(), Some("E0308"));
    }

    #[test]
    fn test_group_by_category() {
        let analyzer = ErrorAnalyzer::new();
        let errors = vec![
            analyzer.analyze(Some("E0308"), "mismatched types", "a.rs", None, None),
            analyzer.analyze(Some("E0308"), "mismatched types", "b.rs", None, None),
            analyzer.analyze(Some("E0433"), "unresolved import", "c.rs", None, None),
        ];

        let groups = analyzer.group_by_category(&errors);
        assert_eq!(
            groups.get(&ErrorCategory::TypeError).map(|v| v.len()),
            Some(2)
        );
        assert_eq!(
            groups
                .get(&ErrorCategory::UnresolvedImport)
                .map(|v| v.len()),
            Some(1)
        );
    }

    #[test]
    fn test_first_to_fix() {
        let analyzer = ErrorAnalyzer::new();
        let errors = vec![
            analyzer.analyze(None, "unused variable", "a.rs", None, None),
            analyzer.analyze(Some("E0308"), "mismatched types", "b.rs", None, None),
        ];

        let first = analyzer.first_to_fix(&errors);
        assert!(first.is_some());
        assert_eq!(first.unwrap().code.as_deref(), Some("E0308"));
    }

    #[test]
    fn test_error_category_priority() {
        assert!(ErrorCategory::TypeError.priority() < ErrorCategory::UnusedWarning.priority());
        assert!(ErrorCategory::BorrowError.priority() < ErrorCategory::StyleWarning.priority());
    }

    #[test]
    fn test_extract_identifier() {
        let message = "cannot find value `foo` in this scope";
        let result = extract_identifier(message, "cannot find value `", "`");
        assert_eq!(result, Some("foo"));
    }

    #[test]
    fn test_extract_between() {
        let message = "expected `String`, found `&str`";
        let result = extract_between(message, "expected `", "`");
        assert_eq!(result, Some("String".to_string()));
    }

    #[test]
    fn test_summary() {
        let analyzer = ErrorAnalyzer::new();
        let errors = vec![
            analyzer.analyze(Some("E0308"), "mismatched types", "a.rs", None, None),
            analyzer.analyze(None, "unused variable", "b.rs", None, None),
        ];

        let summary = analyzer.summary(&errors);
        assert!(summary.contains("Total errors: 2"));
        assert!(summary.contains("By category:"));
    }

    #[test]
    fn test_unused_warning_detection() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(None, "unused variable: `x`", "src/main.rs", Some(5), None);

        assert_eq!(error.category, ErrorCategory::UnusedWarning);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_e0599_no_method() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0599"),
            "no method named `foo` found for struct `Bar`",
            "src/main.rs",
            None,
            None,
        );

        assert_eq!(error.category, ErrorCategory::TraitError);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_e0106_lifetime() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0106"),
            "missing lifetime specifier",
            "src/lib.rs",
            None,
            None,
        );

        assert_eq!(error.category, ErrorCategory::LifetimeError);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_e0277_trait_bound() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0277"),
            "the trait bound `Foo: Clone` is not satisfied",
            "src/lib.rs",
            None,
            None,
        );

        assert_eq!(error.category, ErrorCategory::TraitError);
    }

    #[test]
    fn test_fix_suggestion_auto_fixable() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(None, "unused import: `std::fmt`", "src/lib.rs", None, None);

        assert!(error
            .suggestion
            .as_ref()
            .map(|s| s.auto_fixable)
            .unwrap_or(false));
    }

    #[test]
    fn test_dead_code_warning() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            None,
            "function is never used: `foo` [dead_code]",
            "src/lib.rs",
            None,
            None,
        );

        assert_eq!(error.category, ErrorCategory::UnusedWarning);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_analyzed_error_serialization() {
        let error = AnalyzedError {
            code: Some("E0308".to_string()),
            message: "test".to_string(),
            file: "test.rs".to_string(),
            line: Some(1),
            column: Some(1),
            category: ErrorCategory::TypeError,
            priority: 1,
            suggestion: None,
            related_errors: vec![],
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("E0308"));
    }

    #[test]
    fn test_error_category_all_priorities() {
        let categories = [
            ErrorCategory::TypeError,
            ErrorCategory::UnresolvedImport,
            ErrorCategory::BorrowError,
            ErrorCategory::LifetimeError,
            ErrorCategory::TraitError,
            ErrorCategory::ArgumentError,
            ErrorCategory::PatternError,
            ErrorCategory::UnusedWarning,
            ErrorCategory::StyleWarning,
            ErrorCategory::Other,
        ];

        for cat in categories {
            let priority = cat.priority();
            assert!(priority > 0);
        }
    }

    #[test]
    fn test_error_category_clone() {
        let cat = ErrorCategory::BorrowError;
        let cloned = cat.clone();
        assert_eq!(cat, cloned);
    }

    #[test]
    fn test_error_category_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(ErrorCategory::TypeError);
        set.insert(ErrorCategory::BorrowError);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_fix_suggestion_clone() {
        let fix = FixSuggestion {
            description: "Add clone()".to_string(),
            fix_code: Some(".clone()".to_string()),
            confidence: 0.8,
            auto_fixable: false,
            notes: Some("Note".to_string()),
        };

        let cloned = fix.clone();
        assert_eq!(fix.description, cloned.description);
        assert_eq!(fix.confidence, cloned.confidence);
    }

    #[test]
    fn test_fix_suggestion_serde() {
        let fix = FixSuggestion {
            description: "Fix it".to_string(),
            fix_code: None,
            confidence: 0.5,
            auto_fixable: true,
            notes: None,
        };

        let json = serde_json::to_string(&fix).unwrap();
        assert!(json.contains("Fix it"));

        let parsed: FixSuggestion = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.description, fix.description);
    }

    #[test]
    fn test_analyzed_error_clone() {
        let error = AnalyzedError {
            code: Some("E0001".to_string()),
            message: "error".to_string(),
            file: "test.rs".to_string(),
            line: Some(10),
            column: Some(5),
            category: ErrorCategory::Other,
            priority: 10,
            suggestion: None,
            related_errors: vec!["related".to_string()],
        };

        let cloned = error.clone();
        assert_eq!(error.code, cloned.code);
        assert_eq!(error.file, cloned.file);
    }

    #[test]
    fn test_analyzed_error_deserialize() {
        let json = r#"{
            "code": "E0425",
            "message": "cannot find value",
            "file": "main.rs",
            "line": 5,
            "column": null,
            "category": "unresolved_import",
            "priority": 2,
            "suggestion": null,
            "related_errors": []
        }"#;

        let error: AnalyzedError = serde_json::from_str(json).unwrap();
        assert_eq!(error.code, Some("E0425".to_string()));
        assert_eq!(error.category, ErrorCategory::UnresolvedImport);
    }

    #[test]
    fn test_analyzer_default() {
        let analyzer = ErrorAnalyzer::default();
        let error = analyzer.analyze(None, "test", "test.rs", None, None);
        assert_eq!(error.category, ErrorCategory::Other);
    }

    #[test]
    fn test_e0382_moved_value() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0382"),
            "use of moved value: `x`",
            "src/main.rs",
            Some(10),
            Some(5),
        );

        assert_eq!(error.category, ErrorCategory::BorrowError);
        assert!(error.suggestion.is_some());
        assert!(error.suggestion.as_ref().unwrap().fix_code.is_some());
    }

    #[test]
    fn test_e0502_borrow_conflict() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0502"),
            "cannot borrow `x` as mutable because it is also borrowed as immutable",
            "src/lib.rs",
            None,
            None,
        );

        assert_eq!(error.category, ErrorCategory::BorrowError);
    }

    #[test]
    fn test_e0425_cannot_find_value() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0425"),
            "cannot find value `undefined_var` in this scope",
            "src/main.rs",
            Some(5),
            None,
        );

        assert_eq!(error.category, ErrorCategory::UnresolvedImport);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_e0433_unresolved_import() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0433"),
            "unresolved import `foo::bar`",
            "src/lib.rs",
            Some(1),
            None,
        );

        assert_eq!(error.category, ErrorCategory::UnresolvedImport);
    }

    #[test]
    fn test_analyze_batch_empty() {
        let analyzer = ErrorAnalyzer::new();
        let errors = analyzer.analyze_batch(&[]);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_first_to_fix_empty() {
        let analyzer = ErrorAnalyzer::new();
        let errors: Vec<AnalyzedError> = vec![];
        let first = analyzer.first_to_fix(&errors);
        assert!(first.is_none());
    }

    #[test]
    fn test_group_by_category_empty() {
        let analyzer = ErrorAnalyzer::new();
        let errors: Vec<AnalyzedError> = vec![];
        let groups = analyzer.group_by_category(&errors);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_extract_identifier_not_found() {
        let message = "some other message";
        let result = extract_identifier(message, "cannot find `", "`");
        assert!(result.is_none());
    }

    #[test]
    fn test_error_category_serde() {
        let cat = ErrorCategory::LifetimeError;
        let json = serde_json::to_string(&cat).unwrap();
        assert!(json.contains("lifetime_error"));

        let parsed: ErrorCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cat);
    }

    #[test]
    fn test_analyze_with_all_fields() {
        let analyzer = ErrorAnalyzer::new();
        let error = analyzer.analyze(
            Some("E0308"),
            "expected `i32`, found `String`",
            "/path/to/file.rs",
            Some(42),
            Some(10),
        );

        assert_eq!(error.code, Some("E0308".to_string()));
        assert_eq!(error.file, "/path/to/file.rs");
        assert_eq!(error.line, Some(42));
        assert_eq!(error.column, Some(10));
    }
}
