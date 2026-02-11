//! Input Validation
//!
//! Validates input before submission, including bracket matching
//! and JSON validation.


use reedline::{ValidationResult, Validator};

/// Bracket-aware validator that ensures matching pairs
pub struct BracketValidator {
    /// Whether to allow incomplete input (for multiline)
    allow_incomplete: bool,
}

impl BracketValidator {
    /// Create a new bracket validator
    pub fn new() -> Self {
        Self {
            allow_incomplete: true,
        }
    }

    /// Create a strict validator that requires complete input
    pub fn strict() -> Self {
        Self {
            allow_incomplete: false,
        }
    }

    /// Check if brackets are balanced
    fn check_brackets(&self, input: &str) -> BracketState {
        let mut stack: Vec<char> = Vec::new();
        let mut in_string = false;
        let mut string_char = '"';
        let mut escape_next = false;

        for ch in input.chars() {
            if escape_next {
                escape_next = false;
                continue;
            }

            if ch == '\\' {
                escape_next = true;
                continue;
            }

            if in_string {
                if ch == string_char {
                    in_string = false;
                }
                continue;
            }

            match ch {
                '"' | '\'' => {
                    in_string = true;
                    string_char = ch;
                }
                '(' => stack.push(')'),
                '[' => stack.push(']'),
                '{' => stack.push('}'),
                ')' | ']' | '}' => {
                    if let Some(expected) = stack.pop() {
                        if expected != ch {
                            return BracketState::Mismatched(expected, ch);
                        }
                    } else {
                        return BracketState::ExtraClosing(ch);
                    }
                }
                _ => {}
            }
        }

        if in_string {
            BracketState::UnclosedString
        } else if stack.is_empty() {
            BracketState::Balanced
        } else {
            BracketState::Incomplete(stack)
        }
    }
}

impl Default for BracketValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// State of bracket matching
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BracketState {
    /// All brackets are balanced
    Balanced,
    /// Missing closing brackets
    Incomplete(Vec<char>),
    /// String is not closed
    UnclosedString,
    /// Mismatched bracket pair
    Mismatched(char, char),
    /// Extra closing bracket without opener
    ExtraClosing(char),
}

impl Validator for BracketValidator {
    fn validate(&self, line: &str) -> ValidationResult {
        match self.check_brackets(line) {
            BracketState::Balanced => ValidationResult::Complete,
            BracketState::Incomplete(_) if self.allow_incomplete => ValidationResult::Incomplete,
            BracketState::Incomplete(_) => ValidationResult::Complete,
            BracketState::UnclosedString if self.allow_incomplete => ValidationResult::Incomplete,
            BracketState::UnclosedString => ValidationResult::Complete,
            BracketState::Mismatched(_, _) => ValidationResult::Complete,
            BracketState::ExtraClosing(_) => ValidationResult::Complete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let v = BracketValidator::new();
        assert!(v.allow_incomplete);
    }

    #[test]
    fn test_validator_strict() {
        let v = BracketValidator::strict();
        assert!(!v.allow_incomplete);
    }

    #[test]
    fn test_validator_default() {
        let v = BracketValidator::default();
        assert!(v.allow_incomplete);
    }

    #[test]
    fn test_balanced_brackets() {
        let v = BracketValidator::new();

        assert_eq!(v.check_brackets("()"), BracketState::Balanced);
        assert_eq!(v.check_brackets("[]"), BracketState::Balanced);
        assert_eq!(v.check_brackets("{}"), BracketState::Balanced);
        assert_eq!(v.check_brackets("({[]})"), BracketState::Balanced);
        assert_eq!(v.check_brackets("hello world"), BracketState::Balanced);
        assert_eq!(v.check_brackets(""), BracketState::Balanced);
    }

    #[test]
    fn test_incomplete_brackets() {
        let v = BracketValidator::new();

        assert!(matches!(v.check_brackets("("), BracketState::Incomplete(_)));
        assert!(matches!(v.check_brackets("["), BracketState::Incomplete(_)));
        assert!(matches!(v.check_brackets("{"), BracketState::Incomplete(_)));
        assert!(matches!(
            v.check_brackets("({"),
            BracketState::Incomplete(_)
        ));
    }

    #[test]
    fn test_mismatched_brackets() {
        let v = BracketValidator::new();

        assert!(matches!(
            v.check_brackets("(]"),
            BracketState::Mismatched(_, _)
        ));
        assert!(matches!(
            v.check_brackets("[)"),
            BracketState::Mismatched(_, _)
        ));
        assert!(matches!(
            v.check_brackets("{)"),
            BracketState::Mismatched(_, _)
        ));
    }

    #[test]
    fn test_extra_closing() {
        let v = BracketValidator::new();

        assert!(matches!(
            v.check_brackets(")"),
            BracketState::ExtraClosing(_)
        ));
        assert!(matches!(
            v.check_brackets("]"),
            BracketState::ExtraClosing(_)
        ));
        assert!(matches!(
            v.check_brackets("}"),
            BracketState::ExtraClosing(_)
        ));
    }

    #[test]
    fn test_strings_ignored() {
        let v = BracketValidator::new();

        // Brackets inside strings should be ignored
        assert_eq!(
            v.check_brackets(r#""hello (world)""#),
            BracketState::Balanced
        );
        assert_eq!(
            v.check_brackets(r#"'hello [world]'"#),
            BracketState::Balanced
        );
    }

    #[test]
    fn test_unclosed_string() {
        let v = BracketValidator::new();

        assert_eq!(v.check_brackets(r#""hello"#), BracketState::UnclosedString);
        assert_eq!(v.check_brackets("'hello"), BracketState::UnclosedString);
    }

    #[test]
    fn test_escaped_quotes() {
        let v = BracketValidator::new();

        // Escaped quotes shouldn't end the string
        assert_eq!(
            v.check_brackets(r#""hello \"world\"""#),
            BracketState::Balanced
        );
    }

    #[test]
    fn test_validation_complete() {
        let v = BracketValidator::new();

        assert!(matches!(
            v.validate("hello world"),
            ValidationResult::Complete
        ));
        assert!(matches!(v.validate("()[]{}"), ValidationResult::Complete));
    }

    #[test]
    fn test_validation_incomplete() {
        let v = BracketValidator::new();

        assert!(matches!(v.validate("("), ValidationResult::Incomplete));
        assert!(matches!(v.validate("{["), ValidationResult::Incomplete));
    }

    #[test]
    fn test_strict_validation() {
        let v = BracketValidator::strict();

        // Strict mode returns Complete even for incomplete
        assert!(matches!(v.validate("("), ValidationResult::Complete));
    }

    #[test]
    fn test_nested_brackets() {
        let v = BracketValidator::new();

        assert_eq!(v.check_brackets("((()))"), BracketState::Balanced);
        assert_eq!(v.check_brackets("([{()}])"), BracketState::Balanced);
        assert_eq!(v.check_brackets("{[()]}"), BracketState::Balanced);
    }

    #[test]
    fn test_complex_input() {
        let v = BracketValidator::new();

        let json = r#"{"key": "value", "array": [1, 2, 3]}"#;
        assert_eq!(v.check_brackets(json), BracketState::Balanced);

        let code = r#"fn main() { println!("Hello"); }"#;
        assert_eq!(v.check_brackets(code), BracketState::Balanced);
    }

    #[test]
    fn test_incomplete_returns_expected_closing() {
        let v = BracketValidator::new();

        match v.check_brackets("(") {
            BracketState::Incomplete(stack) => {
                assert_eq!(stack, vec![')']);
            }
            _ => panic!("Expected Incomplete"),
        }

        match v.check_brackets("([{") {
            BracketState::Incomplete(stack) => {
                assert_eq!(stack, vec![')', ']', '}']);
            }
            _ => panic!("Expected Incomplete"),
        }
    }

    #[test]
    fn test_mismatched_returns_details() {
        let v = BracketValidator::new();

        match v.check_brackets("(]") {
            BracketState::Mismatched(expected, got) => {
                assert_eq!(expected, ')');
                assert_eq!(got, ']');
            }
            _ => panic!("Expected Mismatched"),
        }
    }

    #[test]
    fn test_extra_closing_returns_char() {
        let v = BracketValidator::new();

        match v.check_brackets("}") {
            BracketState::ExtraClosing(ch) => {
                assert_eq!(ch, '}');
            }
            _ => panic!("Expected ExtraClosing"),
        }
    }
}
