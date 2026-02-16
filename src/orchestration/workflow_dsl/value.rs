//! Value -- runtime value representation and conversions.

use std::collections::HashMap;

use super::ast::AstNode;

/// Runtime value
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
    Function {
        params: Vec<String>,
        body: Vec<AstNode>,
    },
    StepResult {
        name: String,
        success: bool,
        output: String,
        error: Option<String>,
    },
}

impl Value {
    /// Convert to boolean
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Boolean(b) => *b,
            Value::Integer(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            Value::Function { .. } => true,
            Value::StepResult { success, .. } => *success,
        }
    }

    /// Convert to string
    pub fn as_string(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Integer(n) => n.to_string(),
            Value::Float(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(a) => format!(
                "[{}]",
                a.iter()
                    .map(|v| v.as_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Object(_) => "[object]".to_string(),
            Value::Function { .. } => "[function]".to_string(),
            Value::StepResult { output, .. } => output.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_as_bool() {
        assert!(!Value::Null.as_bool());
        assert!(Value::Boolean(true).as_bool());
        assert!(!Value::Boolean(false).as_bool());
        assert!(Value::Integer(1).as_bool());
        assert!(!Value::Integer(0).as_bool());
        assert!(Value::String("hello".to_string()).as_bool());
        assert!(!Value::String(String::new()).as_bool());
    }

    #[test]
    fn test_value_as_string() {
        assert_eq!(Value::Integer(42).as_string(), "42");
        assert_eq!(Value::Boolean(true).as_string(), "true");
        assert_eq!(Value::String("test".to_string()).as_string(), "test");
    }
}
