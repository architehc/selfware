//! Guardrail Engine
//!
//! Core engine for evaluating guardrail conditions and enforcing policies.

use super::types::*;
use crate::errors::{SafetyError, SelfwareError};
use crate::observability::telemetry;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

/// Engine for evaluating guardrail conditions
pub struct GuardrailEngine {
    /// Compiled regex patterns for condition evaluation
    patterns: HashMap<String, Regex>,
    /// Enable detailed logging
    verbose: bool,
}

impl Default for GuardrailEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GuardrailEngine {
    /// Create a new guardrail engine
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
            verbose: false,
        }
    }

    /// Create a verbose engine with detailed logging
    pub fn new_verbose() -> Self {
        Self {
            patterns: HashMap::new(),
            verbose: true,
        }
    }

    /// Evaluate a single condition against a context
    pub fn evaluate_condition(
        &self,
        condition: &Condition,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        match condition {
            Condition::Inline(expr) => self.evaluate_inline_expression(expr, context),
            Condition::Code { language, content } => {
                self.evaluate_code_condition(language, content, context)
            }
            Condition::Composite {
                operator,
                conditions,
            } => self.evaluate_composite_condition(*operator, conditions, context),
        }
    }

    /// Evaluate an inline expression (simple pattern matching)
    pub fn evaluate_inline_expression(
        &self,
        expr: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let expr = expr.trim();

        // Handle negation
        if let Some(stripped) = expr.strip_prefix('!') {
            let inner = &stripped.trim();
            match self.evaluate_inline_expression(inner, context) {
                EvaluationResult::Pass => EvaluationResult::Fail {
                    reason: format!("Negated condition '{inner}' was true"),
                },
                EvaluationResult::Fail { .. } => EvaluationResult::Pass,
                error => error,
            }
        }
        // Handle contains check: "output.contains('pattern')"
        else if let Some(caps) = get_contains_pattern().captures(expr) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let pattern = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            self.evaluate_contains(source, pattern, context)
        }
        // Handle regex match: "output.matches('regex')"
        else if let Some(caps) = get_matches_pattern().captures(expr) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let pattern = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            self.evaluate_matches(source, pattern, context)
        }
        // Handle comparison: "value > 10"
        else if let Some(caps) = get_comparison_pattern().captures(expr) {
            let left = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let op = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let right = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            self.evaluate_comparison(left, op, right, context)
        }
        // Handle equality check
        else if let Some(caps) = get_equality_pattern().captures(expr) {
            let left = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let op = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let right = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            self.evaluate_equality(left, op, right, context)
        }
        // Handle simple boolean: "true" or "false"
        else if expr == "true" {
            EvaluationResult::Pass
        } else if expr == "false" {
            EvaluationResult::Fail {
                reason: "Condition is false".to_string(),
            }
        }
        // Handle variable existence check
        else if expr.starts_with("exists:") {
            let var_path = expr.strip_prefix("exists:").unwrap_or("").trim();
            self.evaluate_exists(var_path, context)
        }
        // Unknown expression format - try as truthy check
        else {
            match self.resolve_value(expr, context) {
                Some(val) => {
                    if Self::is_truthy(&val) {
                        EvaluationResult::Pass
                    } else {
                        EvaluationResult::Fail {
                            reason: format!("'{expr}' evaluated to falsy value: {val}"),
                        }
                    }
                }
                None => EvaluationResult::Error {
                    message: format!("Could not resolve expression: '{expr}'"),
                },
            }
        }
    }

    /// Evaluate a code block condition
    pub fn evaluate_code_condition(
        &self,
        language: &str,
        content: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        match language.to_lowercase().as_str() {
            "rust" | "rs" => self.evaluate_rust_code(content, context),
            "json" => self.evaluate_json_logic(content, context),
            "regex" | "regexp" => self.evaluate_regex_pattern(content, context),
            _ => EvaluationResult::Error {
                message: format!("Unsupported condition language: '{language}'"),
            },
        }
    }

    /// Evaluate composite conditions with AND/OR logic
    fn evaluate_composite_condition(
        &self,
        operator: LogicalOperator,
        conditions: &[Condition],
        context: &GuardrailContext,
    ) -> EvaluationResult {
        if conditions.is_empty() {
            return EvaluationResult::Pass; // Empty composite passes
        }

        let mut fail_reasons = Vec::new();

        match operator {
            LogicalOperator::And => {
                // All conditions must pass
                for cond in conditions {
                    match self.evaluate_condition(cond, context) {
                        EvaluationResult::Pass => continue,
                        fail @ EvaluationResult::Fail { .. } => return fail,
                        error @ EvaluationResult::Error { .. } => return error,
                    }
                }
                EvaluationResult::Pass
            }
            LogicalOperator::Or => {
                // At least one condition must pass
                for cond in conditions {
                    match self.evaluate_condition(cond, context) {
                        EvaluationResult::Pass => return EvaluationResult::Pass,
                        EvaluationResult::Fail { reason } => fail_reasons.push(reason),
                        error @ EvaluationResult::Error { .. } => return error,
                    }
                }
                EvaluationResult::Fail {
                    reason: format!("All OR conditions failed: {}", fail_reasons.join("; ")),
                }
            }
        }
    }

    /// Evaluate contains check
    fn evaluate_contains(
        &self,
        source: &str,
        pattern: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let source_value = match self.resolve_value(source, context) {
            Some(v) => v,
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve source: '{source}'"),
                }
            }
        };

        let source_string = source_value.to_string();
        let source_str = source_value.as_str().unwrap_or(&source_string);
        let pattern_owned = pattern.to_string();
        let pattern_str = pattern.trim_matches('\'').trim_matches('"');

        if source_str.contains(pattern_str) {
            EvaluationResult::Pass
        } else {
            EvaluationResult::Fail {
                reason: format!("'{source}' does not contain '{pattern}'"),
            }
        }
    }

    /// Evaluate regex matches
    fn evaluate_matches(
        &self,
        source: &str,
        pattern: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let source_value = match self.resolve_value(source, context) {
            Some(v) => v,
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve source: '{source}'"),
                }
            }
        };

        let source_string = source_value.to_string();
        let source_str = source_value.as_str().unwrap_or(&source_string);
        let pattern = pattern.trim_matches('\'').trim_matches('"');

        match Regex::new(pattern) {
            Ok(regex) => {
                if regex.is_match(source_str) {
                    EvaluationResult::Pass
                } else {
                    EvaluationResult::Fail {
                        reason: format!("'{source}' does not match pattern '{pattern}'"),
                    }
                }
            }
            Err(e) => EvaluationResult::Error {
                message: format!("Invalid regex pattern '{pattern}': {e}"),
            },
        }
    }

    /// Evaluate numeric comparison
    fn evaluate_comparison(
        &self,
        left: &str,
        op: &str,
        right: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let left_val = match self.resolve_value(left, context) {
            Some(v) => v,
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve left operand: '{left}'"),
                }
            }
        };

        let left_num = match left_val.as_f64() {
            Some(n) => n,
            None => {
                return EvaluationResult::Error {
                    message: format!("Left operand '{left}' is not a number"),
                }
            }
        };

        let right_num: f64 = match right.trim().parse() {
            Ok(n) => n,
            Err(_) => {
                // Try to resolve as a variable
                match self.resolve_value(right, context) {
                    Some(v) => match v.as_f64() {
                        Some(n) => n,
                        None => {
                            return EvaluationResult::Error {
                                message: format!("Right operand '{right}' is not a number"),
                            }
                        }
                    },
                    None => {
                        return EvaluationResult::Error {
                            message: format!("Could not resolve right operand: '{right}'"),
                        }
                    }
                }
            }
        };

        let result = match op {
            ">" => left_num > right_num,
            ">=" => left_num >= right_num,
            "<" => left_num < right_num,
            "<=" => left_num <= right_num,
            _ => {
                return EvaluationResult::Error {
                    message: format!("Unknown comparison operator: '{op}'"),
                }
            }
        };

        if result {
            EvaluationResult::Pass
        } else {
            EvaluationResult::Fail {
                reason: format!("{left_num} {op} {right_num} is false"),
            }
        }
    }

    /// Evaluate equality check
    fn evaluate_equality(
        &self,
        left: &str,
        op: &str,
        right: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let left_val = match self.resolve_value(left, context) {
            Some(v) => v,
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve left operand: '{left}'"),
                }
            }
        };

        let right_val = if right.starts_with('\'') || right.starts_with('"') {
            serde_json::Value::String(right.trim_matches('\'').trim_matches('"').to_string())
        } else {
            // Try to resolve as variable first
            match self.resolve_value(right, context) {
                Some(v) => v,
                None => {
                    // Try parsing as number
                    if let Ok(n) = right.trim().parse::<i64>() {
                        serde_json::Value::Number(n.into())
                    } else if let Ok(n) = right.trim().parse::<f64>() {
                        serde_json::Number::from_f64(n)
                            .map(serde_json::Value::Number)
                            .unwrap_or_else(|| serde_json::Value::String(right.to_string()))
                    } else {
                        serde_json::Value::String(right.to_string())
                    }
                }
            }
        };

        let equal = left_val == right_val;
        let result = match op {
            "==" => equal,
            "!=" => !equal,
            _ => {
                return EvaluationResult::Error {
                    message: format!("Unknown equality operator: '{op}'"),
                }
            }
        };

        if result {
            EvaluationResult::Pass
        } else {
            EvaluationResult::Fail {
                reason: format!("'{left}' ({left_val}) {op} '{right}' ({right_val}) is false"),
            }
        }
    }

    /// Evaluate variable existence
    fn evaluate_exists(&self, var_path: &str, context: &GuardrailContext) -> EvaluationResult {
        match self.resolve_value(var_path, context) {
            Some(_) => EvaluationResult::Pass,
            None => EvaluationResult::Fail {
                reason: format!("Variable '{var_path}' does not exist"),
            },
        }
    }

    /// Evaluate Rust-like pseudocode
    fn evaluate_rust_code(&self, code: &str, context: &GuardrailContext) -> EvaluationResult {
        // Convert Rust-like code to inline expressions
        // This is a simplified evaluator for common patterns
        let code = code.trim();

        // Handle let bindings and expressions
        // For now, we evaluate the last expression line
        let lines: Vec<&str> = code.lines().collect();
        if lines.is_empty() {
            return EvaluationResult::Error {
                message: "Empty Rust code block".to_string(),
            };
        }

        // Find the last expression (not a let binding or comment)
        for line in lines.iter().rev() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            if line.starts_with("let ") {
                // Evaluate the assignment
                continue;
            }
            // Evaluate as inline expression
            return self.evaluate_inline_expression(line, context);
        }

        EvaluationResult::Pass
    }

    /// Evaluate JSON logic
    ///
    /// Supports a subset of JSON Logic operators:
    /// - Comparison: "==", "!=", ">", "<", ">=", "<="
    /// - Logic: "and", "or", "!" (not)
    /// - Existence: "exists" (check if value exists in context)
    /// - Regex: "match" (regex pattern matching)
    /// - Context access: "var" (access context variables)
    pub fn evaluate_json_logic(&self, logic: &str, context: &GuardrailContext) -> EvaluationResult {
        // Try to parse as JSON
        let json_value: serde_json::Value = match serde_json::from_str(logic.trim()) {
            Ok(v) => v,
            Err(e) => {
                // Not valid JSON - try to evaluate as inline expression
                return self.evaluate_inline_expression(logic, context);
            }
        };

        self.evaluate_json_value(&json_value, context)
    }

    /// Recursively evaluate a JSON value as JSON Logic
    fn evaluate_json_value(
        &self,
        value: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        match value {
            serde_json::Value::Object(map) => {
                if map.is_empty() {
                    return EvaluationResult::Pass;
                }
                // JSON Logic operators are single-key objects
                if let Some((op, args)) = map.iter().next() {
                    return self.evaluate_json_operator(op, args, context);
                }
                EvaluationResult::Pass
            }
            serde_json::Value::Bool(b) => {
                if *b {
                    EvaluationResult::Pass
                } else {
                    EvaluationResult::Fail {
                        reason: "Boolean condition is false".to_string(),
                    }
                }
            }
            serde_json::Value::String(s) => {
                // Treat as inline expression
                self.evaluate_inline_expression(s, context)
            }
            _ => EvaluationResult::Error {
                message: format!("Unsupported JSON Logic value: {:?}", value),
            },
        }
    }

    /// Evaluate a JSON Logic operator
    fn evaluate_json_operator(
        &self,
        op: &str,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        match op {
            // Comparison operators
            "==" | "===" => self.evaluate_json_comparison(args, context, |a, b| a == b, "=="),
            "!=" | "!==" => self.evaluate_json_comparison(args, context, |a, b| a != b, "!="),
            ">" => self.evaluate_json_numeric_comparison(args, context, |a, b| a > b, ">"),
            ">=" => self.evaluate_json_numeric_comparison(args, context, |a, b| a >= b, ">="),
            "<" => self.evaluate_json_numeric_comparison(args, context, |a, b| a < b, "<"),
            "<=" => self.evaluate_json_numeric_comparison(args, context, |a, b| a <= b, "<="),

            // Logic operators
            "and" | "&&" => self.evaluate_json_and(args, context),
            "or" | "||" => self.evaluate_json_or(args, context),
            "!" | "not" => self.evaluate_json_not(args, context),

            // String operators
            "contains" => self.evaluate_json_contains(args, context),
            "match" => self.evaluate_json_match(args, context),

            // Existence check
            "exists" => self.evaluate_json_exists(args, context),
            "var" => self.evaluate_json_var(args, context),

            // Unknown operator
            _ => EvaluationResult::Error {
                message: format!("Unknown JSON Logic operator: '{}'", op),
            },
        }
    }

    /// Evaluate JSON Logic comparison (== and !=)
    fn evaluate_json_comparison(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
        compare: fn(&serde_json::Value, &serde_json::Value) -> bool,
        op_name: &str,
    ) -> EvaluationResult {
        let arr = match args.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => {
                return EvaluationResult::Error {
                    message: format!("'{}' requires 2 arguments", op_name),
                }
            }
        };

        let left = self.resolve_json_value(&arr[0], context);
        let right = self.resolve_json_value(&arr[1], context);

        if compare(&left, &right) {
            EvaluationResult::Pass
        } else {
            EvaluationResult::Fail {
                reason: format!("{} {} {} is false", left, op_name, right),
            }
        }
    }

    /// Evaluate JSON Logic numeric comparison (>, >=, <, <=)
    fn evaluate_json_numeric_comparison(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
        compare: fn(f64, f64) -> bool,
        op_name: &str,
    ) -> EvaluationResult {
        let arr = match args.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => {
                return EvaluationResult::Error {
                    message: format!("'{}' requires 2 numeric arguments", op_name),
                }
            }
        };

        let left_val = self.resolve_json_value(&arr[0], context);
        let right_val = self.resolve_json_value(&arr[1], context);

        let left_num = match left_val.as_f64() {
            Some(n) => n,
            None => {
                return EvaluationResult::Error {
                    message: format!("Left operand '{}' is not a number", left_val),
                }
            }
        };

        let right_num = match right_val.as_f64() {
            Some(n) => n,
            None => {
                return EvaluationResult::Error {
                    message: format!("Right operand '{}' is not a number", right_val),
                }
            }
        };

        if compare(left_num, right_num) {
            EvaluationResult::Pass
        } else {
            EvaluationResult::Fail {
                reason: format!("{} {} {} is false", left_num, op_name, right_num),
            }
        }
    }

    /// Evaluate JSON Logic AND
    fn evaluate_json_and(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let arr = match args.as_array() {
            Some(a) => a,
            None => {
                // Single argument
                return self.evaluate_json_value(args, context);
            }
        };

        for (i, arg) in arr.iter().enumerate() {
            match self.evaluate_json_value(arg, context) {
                EvaluationResult::Pass => continue,
                fail @ EvaluationResult::Fail { .. } => {
                    return EvaluationResult::Fail {
                        reason: format!("AND condition {} failed: {:?}", i, fail),
                    }
                }
                error => return error,
            }
        }
        EvaluationResult::Pass
    }

    /// Evaluate JSON Logic OR
    fn evaluate_json_or(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let arr = match args.as_array() {
            Some(a) => a,
            None => {
                return self.evaluate_json_value(args, context);
            }
        };

        let mut fail_reasons = Vec::new();
        for arg in arr.iter() {
            match self.evaluate_json_value(arg, context) {
                EvaluationResult::Pass => return EvaluationResult::Pass,
                EvaluationResult::Fail { reason } => fail_reasons.push(reason),
                error => return error,
            }
        }
        EvaluationResult::Fail {
            reason: format!("All OR conditions failed: {}", fail_reasons.join("; ")),
        }
    }

    /// Evaluate JSON Logic NOT
    fn evaluate_json_not(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let result = self.evaluate_json_value(args, context);
        match result {
            EvaluationResult::Pass => EvaluationResult::Fail {
                reason: "NOT condition: inner was true".to_string(),
            },
            EvaluationResult::Fail { .. } => EvaluationResult::Pass,
            error => error,
        }
    }

    /// Evaluate JSON Logic contains
    fn evaluate_json_contains(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let arr = match args.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => {
                return EvaluationResult::Error {
                    message: "'contains' requires [source, pattern] arguments".to_string(),
                }
            }
        };

        let source = self.resolve_json_value(&arr[0], context);
        let pattern = self.resolve_json_value(&arr[1], context);

        let source_owned = source.to_string();
        let pattern_owned = pattern.to_string();
        let source_str = source.as_str().unwrap_or(&source_owned);
        let pattern_str = pattern.as_str().unwrap_or(&pattern_owned);

        if source_str.contains(pattern_str) {
            EvaluationResult::Pass
        } else {
            EvaluationResult::Fail {
                reason: format!("'{}' does not contain '{}'", source_str, pattern_str),
            }
        }
    }

    /// Evaluate JSON Logic regex match
    fn evaluate_json_match(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let arr = match args.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => {
                return EvaluationResult::Error {
                    message: "'match' requires [source, pattern] arguments".to_string(),
                }
            }
        };

        let source = self.resolve_json_value(&arr[0], context);
        let pattern = self.resolve_json_value(&arr[1], context);

        let source_owned = source.to_string();
        let pattern_owned = pattern.to_string();
        let source_str = source.as_str().unwrap_or(&source_owned);
        let pattern_str = pattern.as_str().unwrap_or(&pattern_owned);

        match Regex::new(pattern_str) {
            Ok(regex) => {
                if regex.is_match(source_str) {
                    EvaluationResult::Pass
                } else {
                    EvaluationResult::Fail {
                        reason: format!(
                            "'{}' does not match pattern '{}'",
                            source_str, pattern_str
                        ),
                    }
                }
            }
            Err(e) => EvaluationResult::Error {
                message: format!("Invalid regex pattern '{}': {}", pattern_str, e),
            },
        }
    }

    /// Evaluate JSON Logic exists
    fn evaluate_json_exists(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let var_path = match args {
            serde_json::Value::String(s) => s.as_str(),
            _ => {
                return EvaluationResult::Error {
                    message: "'exists' requires a variable path string".to_string(),
                }
            }
        };

        match self.resolve_value(var_path, context) {
            Some(_) => EvaluationResult::Pass,
            None => EvaluationResult::Fail {
                reason: format!("Variable '{}' does not exist", var_path),
            },
        }
    }

    /// Evaluate JSON Logic var (resolve value from context)
    fn evaluate_json_var(
        &self,
        args: &serde_json::Value,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let var_path = match args {
            serde_json::Value::String(s) => s.as_str(),
            _ => {
                return EvaluationResult::Error {
                    message: "'var' requires a variable path string".to_string(),
                }
            }
        };

        // Try standard resolution, then fallback to state lookup
        let value = self
            .resolve_value(var_path, context)
            .or_else(|| context.state.get(var_path).cloned());

        match value {
            Some(val) => {
                if Self::is_truthy(&val) {
                    EvaluationResult::Pass
                } else {
                    EvaluationResult::Fail {
                        reason: format!("Variable '{}' is falsy", var_path),
                    }
                }
            }
            None => EvaluationResult::Fail {
                reason: format!("Variable '{}' does not exist", var_path),
            },
        }
    }

    /// Resolve a JSON value (either literal or context reference)
    fn resolve_json_value(
        &self,
        value: &serde_json::Value,
        context: &GuardrailContext,
    ) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) if map.contains_key("var") => {
                // It's a var reference
                let var_path = match map.get("var") {
                    Some(serde_json::Value::String(s)) => s.as_str(),
                    Some(v) => return v.clone(),
                    None => return serde_json::Value::Null,
                };
                // First try standard resolution, then fallback to state lookup
                self.resolve_value(var_path, context)
                    .or_else(|| context.state.get(var_path).cloned())
                    .unwrap_or(serde_json::Value::Null)
            }
            serde_json::Value::Object(map) if map.len() == 1 => {
                // Might be another operator - evaluate it
                match self.evaluate_json_value(value, context) {
                    EvaluationResult::Pass => serde_json::Value::Bool(true),
                    EvaluationResult::Fail { .. } => serde_json::Value::Bool(false),
                    EvaluationResult::Error { message } => {
                        serde_json::Value::String(format!("error: {}", message))
                    }
                }
            }
            serde_json::Value::String(s) => {
                // Try to resolve as variable (standard and state fallback), otherwise return as string literal
                self.resolve_value(s, context)
                    .or_else(|| context.state.get(s).cloned())
                    .unwrap_or_else(|| serde_json::Value::String(s.clone()))
            }
            _ => value.clone(),
        }
    }

    /// Evaluate regex pattern directly
    fn evaluate_regex_pattern(
        &self,
        pattern: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        // Use agent_output as default source for regex patterns
        let source = context
            .agent_output
            .as_ref()
            .or(context.tool_output.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("");

        match Regex::new(pattern.trim()) {
            Ok(regex) => {
                if regex.is_match(source) {
                    EvaluationResult::Pass
                } else {
                    EvaluationResult::Fail {
                        reason: format!("Pattern '{pattern}' did not match"),
                    }
                }
            }
            Err(e) => EvaluationResult::Error {
                message: format!("Invalid regex pattern: {e}"),
            },
        }
    }

    /// Resolve a value from the context using dot notation (e.g., "state.count")
    fn resolve_value(&self, path: &str, context: &GuardrailContext) -> Option<serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        let root = match parts[0] {
            "state" => parts.get(1).and_then(|k| context.state.get(*k)).cloned(),
            "agent_outputs" => parts
                .get(1)
                .and_then(|k| context.agent_outputs.get(*k))
                .map(|s| serde_json::Value::String(s.clone())),
            "workflow_inputs" => parts
                .get(1)
                .and_then(|k| context.workflow_inputs.get(*k))
                .cloned(),
            "args" => {
                // Special 'args' accessor for common patterns
                if parts.len() >= 2 {
                    match parts[1] {
                        "state" => parts.get(2).and_then(|k| context.state.get(*k)).cloned(),
                        "agent_outputs" => parts
                            .get(2)
                            .and_then(|k| context.agent_outputs.get(*k))
                            .map(|s| serde_json::Value::String(s.clone())),
                        "agent_output" => {
                            context.agent_output.clone().map(serde_json::Value::String)
                        }
                        "tool_input" => context.tool_input.clone().map(serde_json::Value::String),
                        "tool_output" => context.tool_output.clone().map(serde_json::Value::String),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            "agent_output" => context.agent_output.clone().map(serde_json::Value::String),
            "tool_input" => context.tool_input.clone().map(serde_json::Value::String),
            "tool_output" => context.tool_output.clone().map(serde_json::Value::String),
            _ => None,
        };

        // Handle nested access for remaining parts
        if parts.len() > 2 && parts[0] != "args" {
            let mut current = root?;
            for part in &parts[2..] {
                current = current.get(part)?.clone();
            }
            Some(current)
        } else {
            root
        }
    }

    /// Check if a JSON value is truthy
    fn is_truthy(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
            serde_json::Value::String(s) => !s.is_empty(),
            serde_json::Value::Array(a) => !a.is_empty(),
            serde_json::Value::Object(o) => !o.is_empty(),
            serde_json::Value::Null => false,
        }
    }
}

// Static regex patterns (compiled once)
static CONTAINS_PATTERN: OnceLock<Regex> = OnceLock::new();
static MATCHES_PATTERN: OnceLock<Regex> = OnceLock::new();
static COMPARISON_PATTERN: OnceLock<Regex> = OnceLock::new();
static EQUALITY_PATTERN: OnceLock<Regex> = OnceLock::new();

fn get_contains_pattern() -> &'static Regex {
    CONTAINS_PATTERN.get_or_init(|| {
        Regex::new(r#"([\w.]+)\.contains\((["'])(.+?)["']\)"#).expect("Invalid regex")
    })
}

fn get_matches_pattern() -> &'static Regex {
    MATCHES_PATTERN.get_or_init(|| {
        Regex::new(r#"([\w.]+)\.matches\((["'])(.+?)["']\)"#).expect("Invalid regex")
    })
}

fn get_comparison_pattern() -> &'static Regex {
    COMPARISON_PATTERN
        .get_or_init(|| Regex::new(r"(\w+(?:\.\w+)*)\s*([><]=?)\s*(.+)$").expect("Invalid regex"))
}

fn get_equality_pattern() -> &'static Regex {
    EQUALITY_PATTERN
        .get_or_init(|| Regex::new(r"(\w+(?:\.\w+)*)\s*([!=]=)\s*(.+)$").expect("Invalid regex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_simple_boolean() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new();

        assert!(engine.evaluate_inline_expression("true", &ctx).is_pass());
        assert!(engine.evaluate_inline_expression("false", &ctx).is_fail());
    }

    #[test]
    fn test_evaluate_contains() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new()
            .with_agent_output("agent1", "This is a test output with [CRITICAL] issue");

        let result = engine.evaluate_inline_expression("agent_output.contains('[CRITICAL]')", &ctx);
        assert!(result.is_pass(), "Should detect CRITICAL in output");

        let result = engine.evaluate_inline_expression("agent_output.contains('[LOW]')", &ctx);
        assert!(result.is_fail(), "Should not detect LOW in output");
    }

    #[test]
    fn test_evaluate_negation() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new().with_agent_output("agent1", "safe output");

        let result = engine.evaluate_inline_expression("!agent_output.contains('dangerous')", &ctx);
        assert!(result.is_pass(), "Negation should work");
    }

    #[test]
    fn test_evaluate_comparison() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new().with_state("count", 10);

        let result = engine.evaluate_inline_expression("state.count > 5", &ctx);
        assert!(result.is_pass(), "10 > 5 should pass");

        let result = engine.evaluate_inline_expression("state.count < 5", &ctx);
        assert!(result.is_fail(), "10 < 5 should fail");
    }

    #[test]
    fn test_composite_and() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new()
            .with_agent_output("agent1", "test output")
            .with_state("count", 5);

        let conditions = vec![
            Condition::Inline("agent_output.contains('test')".to_string()),
            Condition::Inline("state.count > 3".to_string()),
        ];

        let result = engine.evaluate_composite_condition(LogicalOperator::And, &conditions, &ctx);
        assert!(result.is_pass());
    }

    #[test]
    fn test_composite_or() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new().with_agent_output("agent1", "test output");

        let conditions = vec![
            Condition::Inline("agent_output.contains('missing')".to_string()),
            Condition::Inline("agent_output.contains('test')".to_string()),
        ];

        let result = engine.evaluate_composite_condition(LogicalOperator::Or, &conditions, &ctx);
        assert!(result.is_pass());
    }

    #[test]
    fn test_evaluate_rust_code() {
        let engine = GuardrailEngine::new();
        let ctx =
            GuardrailContext::new().with_agent_output("agent1", "[CRITICAL] Security issue found");

        // Use a direct inline expression that the simplified evaluator can handle.
        // The evaluator resolves agent_output from context and checks .contains().
        let code = r#"
            // Check for critical issues
            !agent_output.contains("[CRITICAL]")
        "#;

        let result = engine.evaluate_rust_code(code, &ctx);
        assert!(
            result.is_fail(),
            "Should detect CRITICAL in code evaluation"
        );
    }

    #[test]
    fn test_evaluate_equality() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new().with_state("count", 42);

        // Test numeric equality
        let result = engine.evaluate_inline_expression("state.count == 42", &ctx);
        assert!(
            result.is_pass(),
            "state.count == 42 should pass, got: {:?}",
            result
        );

        // Test inequality
        let result = engine.evaluate_inline_expression("state.count != 10", &ctx);
        assert!(
            result.is_pass(),
            "state.count != 10 should pass, got: {:?}",
            result
        );

        // Test string equality
        let ctx2 = GuardrailContext::new().with_state("name", "test");
        let result = engine.evaluate_inline_expression("state.name == 'test'", &ctx2);
        assert!(
            result.is_pass(),
            "state.name == 'test' should pass, got: {:?}",
            result
        );
    }

    #[test]
    fn test_json_logic_comparison_operators() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new().with_state("count", 10);

        // Test >= operator
        let json_logic = r#"{">=": [{"var": "count"}, 5]}"#;
        let result = engine.evaluate_json_logic(json_logic, &ctx);
        assert!(
            result.is_pass(),
            ">= should pass when count is 10, got: {:?}",
            result
        );

        // Test < operator
        let json_logic = r#"{"<": [{"var": "count"}, 20]}"#;
        let result = engine.evaluate_json_logic(json_logic, &ctx);
        assert!(
            result.is_pass(),
            "< should pass when count is 10 and comparing to 20, got: {:?}",
            result
        );
    }

    #[test]
    fn test_json_logic_and_operator() {
        let engine = GuardrailEngine::new();
        let ctx = GuardrailContext::new()
            .with_state("count", 10)
            .with_state("enabled", true);

        // Test AND with two true conditions
        let json_logic = r#"{"and": [{"var": "count"}, {"var": "enabled"}]}"#;
        let result = engine.evaluate_json_logic(json_logic, &ctx);
        assert!(
            result.is_pass(),
            "AND should pass when both conditions are true, got: {:?}",
            result
        );
    }
}
