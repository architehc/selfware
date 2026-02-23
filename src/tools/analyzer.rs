use crate::tools::cargo::{CompilerError, Severity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Analyzer for compiler errors with fix suggestions
pub struct ErrorAnalyzer;

/// A group of related errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorGroup {
    pub primary_error: CompilerError,
    pub related_errors: Vec<CompilerError>,
    pub likely_cause: String,
}

impl ErrorAnalyzer {
    /// Suggest a fix for a compiler error based on its error code
    pub fn suggest_fix(error: &CompilerError) -> Option<String> {
        let code = error.code.as_deref()?;

        match code {
            // Cannot find value
            "E0425" => Some(format!(
                "Cannot find value '{}'. Check for:\n  - Typos in the variable name\n  - Missing import (add `use` statement)\n  - Variable not in scope (declared in different block)",
                extract_identifier(&error.message).unwrap_or("unknown")
            )),

            // Unresolved import
            "E0433" => Some("Unresolved import. Try:\n  - Adding the crate to Cargo.toml dependencies\n  - Fixing the module path\n  - Using `crate::` prefix for local modules".to_string()),

            // Use of moved value
            "E0382" => Some("Value has been moved. Fix options:\n  - Add `.clone()` before the first use\n  - Use a reference `&` instead of moving\n  - Implement `Copy` trait if applicable".to_string()),

            // No method found
            "E0599" => Some("Method not found. Check:\n  - Is the trait imported? (add `use TraitName;`)\n  - Is the method spelled correctly?\n  - Does the type implement this method?".to_string()),

            // Type mismatch
            "E0308" => Some("Type mismatch. Consider:\n  - Converting types explicitly (e.g., `.into()`, `as`)\n  - Using `.to_string()` for String conversions\n  - Checking function return types".to_string()),

            // Missing lifetime specifier
            "E0106" => Some("Missing lifetime specifier. Try:\n  - Adding `'_` for inferred lifetime\n  - Adding explicit lifetime `'a`\n  - Using owned types instead of references".to_string()),

            // Borrow checker - cannot borrow as mutable
            "E0502" => Some("Cannot borrow as mutable while borrowed. Fix:\n  - Reduce scope of immutable borrow\n  - Clone data if possible\n  - Restructure to avoid simultaneous borrows".to_string()),

            // Cannot borrow as mutable more than once
            "E0499" => Some("Cannot have multiple mutable borrows. Try:\n  - Use a single mutable reference\n  - Use `Cell` or `RefCell` for interior mutability\n  - Restructure the code".to_string()),

            // Missing trait implementation
            "E0277" => Some("Trait bound not satisfied. Options:\n  - Implement the required trait\n  - Use a wrapper type that implements it\n  - Add derive macro if available".to_string()),

            // Private field/method
            "E0616" | "E0624" => Some("Cannot access private field/method. Consider:\n  - Making it `pub` or `pub(crate)`\n  - Using a public getter/setter method\n  - Creating a constructor function".to_string()),

            // Unused variable/import
            "unused_variables" | "unused_imports" => Some("Unused item. Options:\n  - Remove it if not needed\n  - Prefix with `_` to silence warning\n  - Add `#[allow(dead_code)]` attribute".to_string()),

            // Missing field in struct
            "E0063" => Some("Missing field(s) in struct. Make sure to:\n  - Initialize all required fields\n  - Use `..Default::default()` for defaults\n  - Check struct definition for required fields".to_string()),

            // Duplicate definitions
            "E0428" => Some("Duplicate definition. Fix:\n  - Rename one of the items\n  - Remove the duplicate\n  - Use different modules".to_string()),

            // Wrong number of arguments
            "E0061" => Some("Wrong number of arguments. Check:\n  - Function signature for expected arguments\n  - If using default arguments, ensure correct syntax\n  - Method vs associated function call".to_string()),

            // Return type mismatch
            "E0317" => Some("Return type mismatch. Ensure:\n  - All branches return the same type\n  - Add explicit return if needed\n  - Convert types appropriately".to_string()),

            _ => None,
        }
    }

    /// Prioritize errors by importance (type errors before style issues)
    pub fn prioritize(errors: &[CompilerError]) -> Vec<&CompilerError> {
        let mut sorted: Vec<&CompilerError> = errors.iter().collect();

        sorted.sort_by(|a, b| {
            // First by severity
            let sev_ord = severity_priority(&a.severity).cmp(&severity_priority(&b.severity));
            if sev_ord != std::cmp::Ordering::Equal {
                return sev_ord;
            }

            // Then by error code priority
            let code_ord =
                error_code_priority(a.code.as_deref()).cmp(&error_code_priority(b.code.as_deref()));
            if code_ord != std::cmp::Ordering::Equal {
                return code_ord;
            }

            // Then by file and line
            let file_ord = a.file.cmp(&b.file);
            if file_ord != std::cmp::Ordering::Equal {
                return file_ord;
            }

            a.line.cmp(&b.line)
        });

        sorted
    }

    /// Group related errors by likely common cause
    pub fn group_by_cause(errors: &[CompilerError]) -> Vec<ErrorGroup> {
        let mut groups: Vec<ErrorGroup> = Vec::new();
        let mut used: Vec<bool> = vec![false; errors.len()];

        for (i, error) in errors.iter().enumerate() {
            if used[i] {
                continue;
            }

            used[i] = true;
            let mut related = Vec::new();

            // Find related errors
            for (j, other) in errors.iter().enumerate() {
                if used[j] || i == j {
                    continue;
                }

                if are_related(error, other) {
                    used[j] = true;
                    related.push(other.clone());
                }
            }

            let cause = determine_cause(error, &related);

            groups.push(ErrorGroup {
                primary_error: error.clone(),
                related_errors: related,
                likely_cause: cause,
            });
        }

        groups
    }

    /// Get a summary of errors by category
    pub fn summarize_by_category(errors: &[CompilerError]) -> HashMap<String, usize> {
        let mut categories: HashMap<String, usize> = HashMap::new();

        for error in errors {
            let category = categorize_error(error);
            *categories.entry(category).or_default() += 1;
        }

        categories
    }

    /// Get the most actionable error to fix first
    pub fn most_actionable(errors: &[CompilerError]) -> Option<&CompilerError> {
        Self::prioritize(errors).into_iter().next()
    }
}

/// Extract an identifier from an error message
fn extract_identifier(message: &str) -> Option<&str> {
    // Look for backtick-quoted identifiers like `foo`
    if let Some(start) = message.find('`') {
        if let Some(end) = message[start + 1..].find('`') {
            return Some(&message[start + 1..start + 1 + end]);
        }
    }
    None
}

/// Get priority value for severity (lower = higher priority)
fn severity_priority(severity: &Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Note => 2,
        Severity::Help => 3,
    }
}

/// Get priority value for error codes (lower = higher priority)
fn error_code_priority(code: Option<&str>) -> u8 {
    match code {
        // Syntax/Parse errors - fix first
        Some(c) if c.starts_with("E0") && c.len() == 5 => {
            let num: u32 = c[1..].parse().unwrap_or(9999);
            match num {
                // Type errors
                308 | 277 | 106 => 1,
                // Borrow checker
                382 | 499 | 502 => 2,
                // Name resolution
                425 | 433 => 3,
                // Method resolution
                599 => 4,
                _ => 5,
            }
        }
        // Clippy warnings
        Some(c) if c.starts_with("clippy::") => 8,
        // Other warnings
        Some(_) => 7,
        None => 9,
    }
}

/// Check if two errors are likely related
fn are_related(a: &CompilerError, b: &CompilerError) -> bool {
    // Same file, within 10 lines
    if a.file == b.file && (a.line as i32 - b.line as i32).abs() < 10 {
        return true;
    }

    // Same error code (cascading errors)
    if a.code.is_some() && a.code == b.code {
        return true;
    }

    // Related error codes
    let related_codes = [
        ("E0382", "E0505"), // Move related
        ("E0499", "E0502"), // Borrow related
        ("E0425", "E0433"), // Name resolution
    ];

    for (code1, code2) in related_codes {
        if (a.code.as_deref() == Some(code1) && b.code.as_deref() == Some(code2))
            || (a.code.as_deref() == Some(code2) && b.code.as_deref() == Some(code1))
        {
            return true;
        }
    }

    false
}

/// Determine the likely cause of a group of errors
fn determine_cause(primary: &CompilerError, related: &[CompilerError]) -> String {
    // If there are many related errors with the same code
    if related.len() > 2 && related.iter().all(|e| e.code == primary.code) {
        return format!(
            "Multiple {} errors - likely a single root cause",
            primary.code.as_deref().unwrap_or("unknown")
        );
    }

    // Check for common patterns
    if let Some(code) = &primary.code {
        match code.as_str() {
            "E0433" => return "Missing import or incorrect module path".to_string(),
            "E0425" => return "Undefined variable or function".to_string(),
            "E0382" | "E0505" => return "Ownership/borrowing issue".to_string(),
            "E0499" | "E0502" => return "Multiple borrow issue".to_string(),
            "E0308" => return "Type mismatch".to_string(),
            _ => {}
        }
    }

    // Default cause based on location
    if !related.is_empty() && related.iter().all(|e| e.file == primary.file) {
        return format!("Multiple issues in {}", primary.file);
    }

    "See error message for details".to_string()
}

/// Categorize an error for summary purposes
fn categorize_error(error: &CompilerError) -> String {
    if let Some(code) = &error.code {
        if code.starts_with("clippy::") {
            return "Clippy lints".to_string();
        }

        match code.as_str() {
            c if c.starts_with("E0") => {
                let num: u32 = c[1..].parse().unwrap_or(9999);
                match num {
                    106 | 107 | 109 | 110 | 228 | 230 | 243 | 621 => "Lifetime errors",
                    277 | 308 | 326 | 369 | 618 => "Type errors",
                    382 | 499 | 502 | 505 | 507 | 515 => "Borrow checker errors",
                    425 | 433 => "Name resolution errors",
                    599 => "Method resolution errors",
                    61..=63 => "Function/struct errors",
                    _ => "Compiler errors",
                }
            }
            _ => "Other errors",
        }
        .to_string()
    } else {
        match error.severity {
            Severity::Error => "Errors".to_string(),
            Severity::Warning => "Warnings".to_string(),
            _ => "Notes".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_error(code: Option<&str>, message: &str, file: &str, line: u32) -> CompilerError {
        CompilerError {
            code: code.map(|s| s.to_string()),
            message: message.to_string(),
            file: file.to_string(),
            line,
            column: 1,
            snippet: String::new(),
            suggestion: None,
            severity: Severity::Error,
        }
    }

    #[test]
    fn test_suggest_fix_e0425() {
        let error = make_error(Some("E0425"), "cannot find value `foo`", "src/main.rs", 10);
        let suggestion = ErrorAnalyzer::suggest_fix(&error);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("Cannot find value"));
    }

    #[test]
    fn test_suggest_fix_e0382() {
        let error = make_error(Some("E0382"), "use of moved value", "src/main.rs", 10);
        let suggestion = ErrorAnalyzer::suggest_fix(&error);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("clone"));
    }

    #[test]
    fn test_suggest_fix_unknown_code() {
        let error = make_error(Some("E9999"), "unknown error", "src/main.rs", 10);
        let suggestion = ErrorAnalyzer::suggest_fix(&error);
        assert!(suggestion.is_none());
    }

    #[test]
    fn test_prioritize_errors() {
        let errors = vec![
            make_error(Some("clippy::unwrap_used"), "warning", "src/main.rs", 10),
            make_error(Some("E0308"), "type mismatch", "src/main.rs", 5),
            make_error(Some("E0425"), "cannot find value", "src/main.rs", 1),
        ];

        let sorted = ErrorAnalyzer::prioritize(&errors);
        // Type errors (E0308) should come before name resolution (E0425)
        assert_eq!(sorted[0].code, Some("E0308".to_string()));
    }

    #[test]
    fn test_group_by_cause() {
        let errors = vec![
            make_error(Some("E0382"), "use of moved value", "src/main.rs", 10),
            make_error(Some("E0382"), "use of moved value", "src/main.rs", 12),
            make_error(Some("E0425"), "cannot find value", "src/other.rs", 5),
        ];

        let groups = ErrorAnalyzer::group_by_cause(&errors);
        // Should group the two E0382 errors together
        assert!(groups.len() <= 2);
    }

    #[test]
    fn test_summarize_by_category() {
        let errors = vec![
            make_error(Some("E0308"), "type mismatch", "src/main.rs", 5),
            make_error(Some("E0308"), "type mismatch", "src/main.rs", 10),
            make_error(Some("E0382"), "use of moved value", "src/main.rs", 15),
        ];

        let summary = ErrorAnalyzer::summarize_by_category(&errors);
        assert_eq!(*summary.get("Type errors").unwrap_or(&0), 2);
    }

    #[test]
    fn test_most_actionable() {
        let errors = vec![
            make_error(Some("clippy::unwrap_used"), "warning", "src/main.rs", 10),
            make_error(Some("E0308"), "type mismatch", "src/main.rs", 5),
        ];

        let actionable = ErrorAnalyzer::most_actionable(&errors);
        assert!(actionable.is_some());
        assert_eq!(actionable.unwrap().code, Some("E0308".to_string()));
    }

    #[test]
    fn test_extract_identifier() {
        assert_eq!(extract_identifier("cannot find value `foo`"), Some("foo"));
        assert_eq!(
            extract_identifier("cannot find value `bar_baz`"),
            Some("bar_baz")
        );
        assert_eq!(extract_identifier("some message without identifier"), None);
    }

    #[test]
    fn test_are_related_same_file_nearby() {
        let a = make_error(Some("E0308"), "error", "src/main.rs", 10);
        let b = make_error(Some("E0425"), "error", "src/main.rs", 12);
        assert!(are_related(&a, &b));
    }

    #[test]
    fn test_are_related_same_code() {
        let a = make_error(Some("E0308"), "error", "src/main.rs", 10);
        let b = make_error(Some("E0308"), "error", "src/other.rs", 100);
        assert!(are_related(&a, &b));
    }

    #[test]
    fn test_not_related() {
        let a = make_error(Some("E0308"), "error", "src/main.rs", 10);
        let b = make_error(Some("E0425"), "error", "src/other.rs", 100);
        assert!(!are_related(&a, &b));
    }

    #[test]
    fn test_categorize_clippy() {
        let error = make_error(Some("clippy::unwrap_used"), "unwrap", "src/main.rs", 10);
        assert_eq!(categorize_error(&error), "Clippy lints");
    }

    #[test]
    fn test_categorize_type_error() {
        let error = make_error(Some("E0308"), "type mismatch", "src/main.rs", 10);
        assert_eq!(categorize_error(&error), "Type errors");
    }

    #[test]
    fn test_categorize_borrow_error() {
        let error = make_error(Some("E0382"), "use of moved value", "src/main.rs", 10);
        assert_eq!(categorize_error(&error), "Borrow checker errors");
    }
}
