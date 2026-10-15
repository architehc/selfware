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
        Self { verbose: false }
    }

    /// Create a verbose engine with detailed logging
    pub fn new_verbose() -> Self {
        Self { verbose: true }
    }

    /// Evaluate a single condition against a context
    pub fn evaluate_condition(
        &self,
        condition: &Condition,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        if self.verbose {
            debug!("Evaluating guardrail condition: {:?}", condition);
        }
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
        let mut expr = expr.trim().trim_end_matches(';').trim();

        // Strip outer balanced parentheses
        while expr.starts_with('(') && expr.ends_with(')') && Self::has_outer_parens(expr) {
            expr = expr[1..expr.len() - 1].trim();
        }

        // Handle if / else expression blocks
        if expr.starts_with("if ") || expr.starts_with("if(") {
            if let Some(res) = self.evaluate_if_else(expr, context) {
                return res;
            }
        }

        // Handle .map_or(default, |x| body)
        if let Some(val) = self.try_evaluate_map_or(expr, context) {
            return if Self::is_truthy(&val) {
                EvaluationResult::Pass
            } else {
                EvaluationResult::Fail {
                    reason: format!("'{expr}' evaluated to false"),
                }
            };
        }

        // Top-level logical OR (`||`) has lowest precedence
        if let Some(parts) = Self::split_top_level_logical(expr, "||") {
            let mut fail_reasons = Vec::new();
            for part in parts {
                match self.evaluate_inline_expression(part, context) {
                    EvaluationResult::Pass => return EvaluationResult::Pass,
                    EvaluationResult::Fail { reason } => fail_reasons.push(reason),
                    error @ EvaluationResult::Error { .. } => return error,
                }
            }
            return EvaluationResult::Fail {
                reason: format!("All OR conditions failed: {}", fail_reasons.join("; ")),
            };
        }

        // Top-level logical AND (`&&`)
        if let Some(parts) = Self::split_top_level_logical(expr, "&&") {
            for part in parts {
                match self.evaluate_inline_expression(part, context) {
                    EvaluationResult::Pass => continue,
                    fail @ EvaluationResult::Fail { .. } => return fail,
                    error @ EvaluationResult::Error { .. } => return error,
                }
            }
            return EvaluationResult::Pass;
        }

        // Top-level equality (`==` or `!=`) outside quotes and parentheses
        if let Some((left, op, right)) = Self::split_top_level_equality(expr) {
            return self.evaluate_equality(left, op, right, context);
        }

        // Handle negation
        if let Some(stripped) = expr.strip_prefix('!') {
            let inner = stripped.trim();
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
        // Handle starts_with check: "output.starts_with('prefix')"
        else if let Some(caps) = get_starts_with_pattern().captures(expr) {
            let source = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let pattern = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            self.evaluate_starts_with(source, pattern, context)
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
                None if Self::looks_like_identifier_or_path(expr) => EvaluationResult::Fail {
                    reason: format!("Variable '{expr}' is not set"),
                },
                None => EvaluationResult::Error {
                    message: format!("Could not resolve expression: '{expr}'"),
                },
            }
        }
    }

    fn looks_like_identifier_or_path(s: &str) -> bool {
        let s = s.trim();
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '(' | ')' | '"' | '\''))
    }

    /// Check if `expr` is wrapped in a single pair of outer parentheses.
    fn has_outer_parens(expr: &str) -> bool {
        let mut depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        let chars: Vec<char> = expr.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' && (in_single || in_double) {
                escaped = true;
                continue;
            }
            match c {
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '(' if !in_single && !in_double => depth += 1,
                ')' if !in_single && !in_double => {
                    depth -= 1;
                    if depth == 0 && i + 1 < chars.len() {
                        return false;
                    }
                }
                _ => {}
            }
        }
        depth == 0
    }

    /// Split `expr` on a top-level binary logical operator (`&&` or `||`),
    /// ignoring occurrences inside quotes, parentheses, or braces.
    fn split_top_level_logical<'a>(expr: &'a str, op: &str) -> Option<Vec<&'a str>> {
        let mut parts = Vec::new();
        let mut depth = 0i32;
        let mut brace_depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        let mut last_idx = 0;
        let bytes = expr.as_bytes();
        let op_bytes = op.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if b == b'\\' && (in_single || in_double) {
                escaped = true;
                i += 1;
                continue;
            }
            match b {
                b'\'' if !in_double => in_single = !in_single,
                b'"' if !in_single => in_double = !in_double,
                b'(' if !in_single && !in_double => depth += 1,
                b')' if !in_single && !in_double => depth -= 1,
                b'{' if !in_single && !in_double => brace_depth += 1,
                b'}' if !in_single && !in_double => brace_depth -= 1,
                _ => {}
            }
            if !in_single
                && !in_double
                && depth == 0
                && brace_depth == 0
                && i + op_bytes.len() <= bytes.len()
                && &bytes[i..i + op_bytes.len()] == op_bytes
            {
                parts.push(expr[last_idx..i].trim());
                i += op_bytes.len();
                last_idx = i;
                continue;
            }
            i += 1;
        }
        if parts.is_empty() {
            None
        } else {
            parts.push(expr[last_idx..].trim());
            Some(parts)
        }
    }

    /// Split `expr` on a top-level `==` or `!=` operator outside quotes/parens/braces.
    fn split_top_level_equality(expr: &str) -> Option<(&str, &str, &str)> {
        let mut depth = 0i32;
        let mut brace_depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        let bytes = expr.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if b == b'\\' && (in_single || in_double) {
                escaped = true;
                i += 1;
                continue;
            }
            match b {
                b'\'' if !in_double => in_single = !in_single,
                b'"' if !in_single => in_double = !in_double,
                b'(' if !in_single && !in_double => depth += 1,
                b')' if !in_single && !in_double => depth -= 1,
                b'{' if !in_single && !in_double => brace_depth += 1,
                b'}' if !in_single && !in_double => brace_depth -= 1,
                _ => {}
            }
            if !in_single && !in_double && depth == 0 && brace_depth == 0 && i + 2 <= bytes.len() {
                let slice = &bytes[i..i + 2];
                if slice == b"==" || slice == b"!=" {
                    let op = if slice == b"==" { "==" } else { "!=" };
                    let left = expr[..i].trim();
                    let right = expr[i + 2..].trim();
                    if !left.is_empty() && !right.is_empty() {
                        return Some((left, op, right));
                    }
                }
            }
            i += 1;
        }
        None
    }

    /// Evaluate starts_with check
    fn evaluate_starts_with(
        &self,
        source: &str,
        pattern: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let source_value = match self.resolve_value(source, context) {
            Some(v) if !v.is_null() => v,
            _ if Self::looks_like_identifier_or_path(source) => {
                serde_json::Value::String(String::new())
            }
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve source: '{source}'"),
                }
            }
            Some(v) => v,
        };

        let source_string = source_value.to_string();
        let source_str = source_value.as_str().unwrap_or(&source_string);
        let pattern_str = pattern.trim_matches('\'').trim_matches('"');

        if source_str.starts_with(pattern_str) {
            EvaluationResult::Pass
        } else {
            EvaluationResult::Fail {
                reason: format!("'{source}' does not start with '{pattern}'"),
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
            Some(v) if !v.is_null() => v,
            _ if Self::looks_like_identifier_or_path(source) => {
                serde_json::Value::String(String::new())
            }
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve source: '{source}'"),
                }
            }
            Some(v) => v,
        };

        let source_string = source_value.to_string();
        let source_str = source_value.as_str().unwrap_or(&source_string);
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
            Some(v) if !v.is_null() => v,
            _ if Self::looks_like_identifier_or_path(source) => {
                serde_json::Value::String(String::new())
            }
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve source: '{source}'"),
                }
            }
            Some(v) => v,
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
            Some(v) if !v.is_null() => v,
            _ if Self::looks_like_identifier_or_path(left) => serde_json::Value::Number(0.into()),
            None => {
                return EvaluationResult::Error {
                    message: format!("Could not resolve left operand: '{left}'"),
                }
            }
            Some(v) => v,
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

    fn resolve_operand_value(
        &self,
        operand: &str,
        context: &GuardrailContext,
    ) -> serde_json::Value {
        let operand = operand.trim();
        if (operand.starts_with('\'') && operand.ends_with('\''))
            || (operand.starts_with('"') && operand.ends_with('"'))
        {
            return serde_json::Value::String(operand[1..operand.len() - 1].to_string());
        }
        if operand == "true" {
            return serde_json::Value::Bool(true);
        }
        if operand == "false" {
            return serde_json::Value::Bool(false);
        }
        if let Ok(n) = operand.parse::<i64>() {
            return serde_json::Value::Number(n.into());
        }
        if let Ok(n) = operand.parse::<f64>() {
            if let Some(num) = serde_json::Number::from_f64(n) {
                return serde_json::Value::Number(num);
            }
        }
        if let Some(v) = self.resolve_value(operand, context) {
            return v;
        }
        if operand.contains(".contains(")
            || operand.contains(".starts_with(")
            || operand.contains(".matches(")
            || operand.contains(".map_or(")
            || operand.starts_with('!')
        {
            match self.evaluate_inline_expression(operand, context) {
                EvaluationResult::Pass => return serde_json::Value::Bool(true),
                EvaluationResult::Fail { .. } => return serde_json::Value::Bool(false),
                EvaluationResult::Error { .. } => {}
            }
        }
        if Self::looks_like_identifier_or_path(operand) {
            return serde_json::Value::Null;
        }
        serde_json::Value::String(operand.to_string())
    }

    /// Evaluate equality check
    fn evaluate_equality(
        &self,
        left: &str,
        op: &str,
        right: &str,
        context: &GuardrailContext,
    ) -> EvaluationResult {
        let left_val = self.resolve_operand_value(left, context);
        let right_val = self.resolve_operand_value(right, context);

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

    /// Strip `//` line comments outside of string literals.
    fn strip_rust_comments(code: &str) -> String {
        let mut out = String::with_capacity(code.len());
        for line in code.lines() {
            let mut in_single = false;
            let mut in_double = false;
            let mut escaped = false;
            let bytes = line.as_bytes();
            let mut cut = line.len();
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                if b == b'\\' && (in_single || in_double) {
                    escaped = true;
                    i += 1;
                    continue;
                }
                match b {
                    b'\'' if !in_double => in_single = !in_single,
                    b'"' if !in_single => in_double = !in_double,
                    b'/' if !in_single
                        && !in_double
                        && i + 1 < bytes.len()
                        && bytes[i + 1] == b'/' =>
                    {
                        cut = i;
                        break;
                    }
                    _ => {}
                }
                i += 1;
            }
            let trimmed = line[..cut].trim_end();
            if !trimmed.trim().is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(trimmed);
            }
        }
        out
    }

    /// Split Rust code into top-level statements by `;` (outside quotes, parens, and braces).
    fn split_rust_statements(code: &str) -> Vec<&str> {
        let mut stmts = Vec::new();
        let mut paren_depth = 0i32;
        let mut brace_depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        let mut last_idx = 0;
        let bytes = code.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            if b == b'\\' && (in_single || in_double) {
                escaped = true;
                continue;
            }
            match b {
                b'\'' if !in_double => in_single = !in_single,
                b'"' if !in_single => in_double = !in_double,
                b'(' if !in_single && !in_double => paren_depth += 1,
                b')' if !in_single && !in_double => paren_depth -= 1,
                b'{' if !in_single && !in_double => brace_depth += 1,
                b'}' if !in_single && !in_double => brace_depth -= 1,
                b';' if !in_single && !in_double && paren_depth == 0 && brace_depth == 0 => {
                    let s = code[last_idx..i].trim();
                    if !s.is_empty() {
                        stmts.push(s);
                    }
                    last_idx = i + 1;
                }
                _ => {}
            }
        }
        let tail = code[last_idx..].trim();
        if !tail.is_empty() {
            stmts.push(tail);
        }
        stmts
    }

    /// Evaluate `.map_or(default, |ident| body)` expression.
    fn try_evaluate_map_or(
        &self,
        expr: &str,
        context: &GuardrailContext,
    ) -> Option<serde_json::Value> {
        let expr = expr.trim();
        if !expr.ends_with(')') {
            return None;
        }
        let map_or_idx = expr.find(".map_or(")?;
        let receiver = expr[..map_or_idx].trim();
        let inner = expr[map_or_idx + ".map_or(".len()..expr.len() - 1].trim();

        // Find top-level comma separating default and closure
        let mut depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut comma_idx = None;
        for (i, c) in inner.char_indices() {
            match c {
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '(' if !in_single && !in_double => depth += 1,
                ')' if !in_single && !in_double => depth -= 1,
                ',' if !in_single && !in_double && depth == 0 => {
                    comma_idx = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let comma_idx = comma_idx?;
        let default_str = inner[..comma_idx].trim();
        let closure_str = inner[comma_idx + 1..].trim();

        if !closure_str.starts_with('|') {
            return None;
        }
        let end_pipe = closure_str[1..].find('|')? + 1;
        let param = closure_str[1..end_pipe].trim();
        let body = closure_str[end_pipe + 1..].trim();

        match self.resolve_value(receiver, context) {
            Some(val) if !val.is_null() => {
                let mut local_ctx = context.clone();
                local_ctx.state.insert(param.to_string(), val);
                match self.evaluate_inline_expression(body, &local_ctx) {
                    EvaluationResult::Pass => Some(serde_json::Value::Bool(true)),
                    EvaluationResult::Fail { .. } => Some(serde_json::Value::Bool(false)),
                    EvaluationResult::Error { .. } => self
                        .resolve_value(body, &local_ctx)
                        .or_else(|| Some(self.resolve_operand_value(default_str, context))),
                }
            }
            _ => Some(self.resolve_operand_value(default_str, context)),
        }
    }

    /// Evaluate `if <cond> { <then_expr> } else { <else_expr> }`.
    fn evaluate_if_else(&self, expr: &str, context: &GuardrailContext) -> Option<EvaluationResult> {
        let expr = expr.trim();
        let rest = expr.strip_prefix("if")?.trim_start();

        // Find first `{` outside quotes and parens
        let mut paren_depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        let mut open_brace = None;
        for (i, c) in rest.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' && (in_single || in_double) {
                escaped = true;
                continue;
            }
            match c {
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '(' if !in_single && !in_double => paren_depth += 1,
                ')' if !in_single && !in_double => paren_depth -= 1,
                '{' if !in_single && !in_double && paren_depth == 0 => {
                    open_brace = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let open_brace = open_brace?;
        let cond_str = rest[..open_brace].trim();
        let close_brace = Self::find_matching_brace(rest, open_brace)?;
        let then_str = rest[open_brace + 1..close_brace].trim();
        let after_then = rest[close_brace + 1..].trim();

        let else_str = if after_then.is_empty() {
            "true"
        } else if let Some(after_else) = after_then.strip_prefix("else") {
            let after_else = after_else.trim();
            if after_else.starts_with('{') {
                let else_close = Self::find_matching_brace(after_else, 0)?;
                after_else[1..else_close].trim()
            } else if after_else.starts_with("if ") || after_else.starts_with("if(") {
                after_else
            } else {
                return None;
            }
        } else {
            return None;
        };

        match self.evaluate_inline_expression(cond_str, context) {
            EvaluationResult::Pass => Some(self.evaluate_rust_code(then_str, context)),
            EvaluationResult::Fail { .. } => Some(self.evaluate_rust_code(else_str, context)),
            err @ EvaluationResult::Error { .. } => Some(err),
        }
    }

    fn find_matching_brace(s: &str, open_idx: usize) -> Option<usize> {
        let mut depth = 0i32;
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;
        for (i, c) in s[open_idx..].char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' && (in_single || in_double) {
                escaped = true;
                continue;
            }
            match c {
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '{' if !in_single && !in_double => depth += 1,
                '}' if !in_single && !in_double => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(open_idx + i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Evaluate the RHS of a `let <ident> = <rhs>` statement.
    fn evaluate_let_rhs(&self, rhs: &str, context: &GuardrailContext) -> serde_json::Value {
        let rhs = rhs.trim();
        if let Some(val) = self.try_evaluate_map_or(rhs, context) {
            return val;
        }
        // Check for top-level `+` string concatenation
        if let Some(parts) = Self::split_top_level_logical(rhs, "+") {
            let mut combined = String::new();
            for part in parts {
                let part = part.trim().trim_start_matches('&').trim();
                if let Some(v) = self.resolve_value(part, context) {
                    if let Some(s) = v.as_str() {
                        combined.push_str(s);
                    } else if !v.is_null() {
                        combined.push_str(&v.to_string());
                    }
                } else if (part.starts_with('"') && part.ends_with('"'))
                    || (part.starts_with('\'') && part.ends_with('\''))
                {
                    combined.push_str(&part[1..part.len() - 1]);
                }
            }
            return serde_json::Value::String(combined);
        }
        if let Some(val) = self.resolve_value(rhs, context) {
            return val;
        }
        // Check if RHS is a boolean expression (e.g. `a == b`, `!x`, `x && y`, `x.contains(...)`)
        if rhs.contains("==")
            || rhs.contains("!=")
            || rhs.contains("&&")
            || rhs.contains("||")
            || rhs.contains(".contains(")
            || rhs.contains(".starts_with(")
            || rhs.contains(".matches(")
            || rhs.starts_with('!')
        {
            match self.evaluate_inline_expression(rhs, context) {
                EvaluationResult::Pass => return serde_json::Value::Bool(true),
                EvaluationResult::Fail { .. } => return serde_json::Value::Bool(false),
                EvaluationResult::Error { .. } => {}
            }
        }
        self.resolve_operand_value(rhs, context)
    }

    /// Evaluate Rust-like pseudocode
    fn evaluate_rust_code(&self, code: &str, context: &GuardrailContext) -> EvaluationResult {
        let cleaned = Self::strip_rust_comments(code);
        if cleaned.trim().is_empty() {
            return EvaluationResult::Error {
                message: "Empty Rust code block".to_string(),
            };
        }

        let mut local_ctx = context.clone();
        let statements = Self::split_rust_statements(&cleaned);

        for stmt in statements {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            if let Some(rest) = stmt.strip_prefix("let ") {
                let rest = rest.trim_start();
                let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
                if let Some(eq_idx) = rest.find('=') {
                    let lhs = rest[..eq_idx].trim();
                    let ident = lhs.split(':').next().unwrap_or(lhs).trim();
                    let rhs = rest[eq_idx + 1..].trim();
                    let val = self.evaluate_let_rhs(rhs, &local_ctx);
                    local_ctx.state.insert(ident.to_string(), val);
                }
                continue;
            }
            if stmt.starts_with("if ") || stmt.starts_with("if(") {
                if let Some(result) = self.evaluate_if_else(stmt, &local_ctx) {
                    if !result.is_pass() {
                        return result;
                    }
                    continue;
                }
            }
            let result = self.evaluate_inline_expression(stmt, &local_ctx);
            if !result.is_pass() {
                return result;
            }
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
        enum Transform<'a> {
            Len,
            ToLowercase,
            Clone,
            Unwrap,
            UnwrapOrDefault,
            UnwrapOr(&'a str),
            IsSome,
            IsNone,
            IsEmpty,
            ContainsKey(&'a str),
            Get(&'a str),
        }

        let mut path = path.trim().trim_start_matches('&').trim();
        let mut transforms: Vec<Transform<'_>> = Vec::new();
        loop {
            if let Some(stripped) = path.strip_suffix(".len()") {
                transforms.push(Transform::Len);
                path = stripped.trim();
            } else if let Some(stripped) = path.strip_suffix(".to_lowercase()") {
                transforms.push(Transform::ToLowercase);
                path = stripped.trim();
            } else if let Some(stripped) = path.strip_suffix(".clone()") {
                transforms.push(Transform::Clone);
                path = stripped.trim();
            } else if let Some(stripped) = path.strip_suffix(".unwrap()") {
                transforms.push(Transform::Unwrap);
                path = stripped.trim();
            } else if let Some(stripped) = path.strip_suffix(".unwrap_or_default()") {
                transforms.push(Transform::UnwrapOrDefault);
                path = stripped.trim();
            } else if let Some(stripped) = path.strip_suffix(".is_some()") {
                transforms.push(Transform::IsSome);
                path = stripped.trim();
            } else if let Some(stripped) = path.strip_suffix(".is_none()") {
                transforms.push(Transform::IsNone);
                path = stripped.trim();
            } else if let Some(stripped) = path.strip_suffix(".is_empty()") {
                transforms.push(Transform::IsEmpty);
                path = stripped.trim();
            } else if path.ends_with(')') {
                if let Some(idx) = path.rfind(".unwrap_or(") {
                    let arg = &path[idx + ".unwrap_or(".len()..path.len() - 1];
                    if !arg.contains('(') {
                        transforms.push(Transform::UnwrapOr(arg.trim()));
                        path = path[..idx].trim();
                        continue;
                    }
                }
                if let Some(idx) = path.rfind(".contains_key(") {
                    let arg = &path[idx + ".contains_key(".len()..path.len() - 1];
                    if !arg.contains('(') {
                        transforms.push(Transform::ContainsKey(arg.trim()));
                        path = path[..idx].trim();
                        continue;
                    }
                }
                if let Some(idx) = path.rfind(".get(") {
                    let arg = &path[idx + ".get(".len()..path.len() - 1];
                    if !arg.contains('(') {
                        transforms.push(Transform::Get(arg.trim()));
                        path = path[..idx].trim();
                        continue;
                    }
                }
                break;
            } else {
                break;
            }
        }
        transforms.reverse();

        let mut current = self.resolve_base_value(path, context);
        if matches!(&current, Some(serde_json::Value::Null)) {
            current = None;
        }

        for transform in transforms {
            match transform {
                Transform::Clone => {}
                Transform::Unwrap => {
                    if current.is_none() {
                        current = Some(serde_json::Value::String(String::new()));
                    }
                }
                Transform::UnwrapOrDefault => {
                    if current.is_none() {
                        current = Some(serde_json::Value::String(String::new()));
                    }
                }
                Transform::UnwrapOr(default_str) => {
                    if current.is_none() {
                        current = Some(self.resolve_operand_value(default_str, context));
                    }
                }
                Transform::IsSome => {
                    current = Some(serde_json::Value::Bool(current.is_some()));
                }
                Transform::IsNone => {
                    current = Some(serde_json::Value::Bool(current.is_none()));
                }
                Transform::IsEmpty => {
                    let is_empty = match &current {
                        None | Some(serde_json::Value::Null) => true,
                        Some(serde_json::Value::String(s)) => s.is_empty(),
                        Some(serde_json::Value::Array(a)) => a.is_empty(),
                        Some(serde_json::Value::Object(o)) => o.is_empty(),
                        _ => false,
                    };
                    current = Some(serde_json::Value::Bool(is_empty));
                }
                Transform::ContainsKey(key_raw) => {
                    let key = key_raw.trim_matches('\'').trim_matches('"');
                    let has_key = match &current {
                        Some(serde_json::Value::Object(o)) => o.contains_key(key),
                        _ => false,
                    };
                    current = Some(serde_json::Value::Bool(has_key));
                }
                Transform::Get(key_raw) => {
                    let key = key_raw.trim_matches('\'').trim_matches('"');
                    current = match current {
                        Some(serde_json::Value::Object(mut o)) => o.remove(key),
                        _ => None,
                    };
                    if matches!(&current, Some(serde_json::Value::Null)) {
                        current = None;
                    }
                }
                Transform::Len => {
                    let len = match &current {
                        Some(serde_json::Value::String(s)) => s.len(),
                        Some(serde_json::Value::Array(a)) => a.len(),
                        Some(serde_json::Value::Object(o)) => o.len(),
                        None | Some(serde_json::Value::Null) => 0,
                        _ => return None,
                    };
                    current = Some(serde_json::Value::Number((len as u64).into()));
                }
                Transform::ToLowercase => {
                    let s = match &current {
                        Some(serde_json::Value::String(s)) => s.to_lowercase(),
                        Some(other) => other.to_string().to_lowercase(),
                        None => String::new(),
                    };
                    current = Some(serde_json::Value::String(s));
                }
            }
        }

        current
    }

    fn resolve_base_value(
        &self,
        path: &str,
        context: &GuardrailContext,
    ) -> Option<serde_json::Value> {
        let path = path.trim();
        let (had_args_prefix, stripped_path) = if let Some(rest) = path.strip_prefix("args.") {
            (true, rest.trim())
        } else {
            (false, path)
        };

        let parts: Vec<&str> = stripped_path.split('.').collect();
        if parts.is_empty() || parts[0].is_empty() {
            return None;
        }

        let (root, consumed) = match parts[0] {
            "state" => {
                if parts.len() == 1 {
                    let map: serde_json::Map<String, serde_json::Value> = context
                        .state
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    (Some(serde_json::Value::Object(map)), 1)
                } else {
                    (context.state.get(parts[1]).cloned(), 2)
                }
            }
            "agent_outputs" => {
                if parts.len() == 1 {
                    let mut map: serde_json::Map<String, serde_json::Value> = context
                        .agent_outputs
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                        .collect();
                    for (k, v) in &context.state {
                        map.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                    (Some(serde_json::Value::Object(map)), 1)
                } else {
                    let val = context
                        .agent_outputs
                        .get(parts[1])
                        .map(|s| serde_json::Value::String(s.clone()))
                        .or_else(|| context.state.get(parts[1]).cloned());
                    (val, 2)
                }
            }
            "workflow_inputs" => {
                if parts.len() == 1 {
                    let map: serde_json::Map<String, serde_json::Value> = context
                        .workflow_inputs
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    (Some(serde_json::Value::Object(map)), 1)
                } else {
                    (context.workflow_inputs.get(parts[1]).cloned(), 2)
                }
            }
            "agent_output" | "output" => {
                let val = context
                    .agent_output
                    .clone()
                    .map(serde_json::Value::String)
                    .or_else(|| {
                        if had_args_prefix {
                            Some(serde_json::Value::String(String::new()))
                        } else {
                            None
                        }
                    });
                (val, 1)
            }
            "tool_input" => {
                let val = context
                    .tool_input
                    .clone()
                    .map(serde_json::Value::String)
                    .or_else(|| {
                        if had_args_prefix {
                            Some(serde_json::Value::String(String::new()))
                        } else {
                            None
                        }
                    });
                (val, 1)
            }
            "tool_output" => {
                let val = context
                    .tool_output
                    .clone()
                    .map(serde_json::Value::String)
                    .or_else(|| {
                        if had_args_prefix {
                            Some(serde_json::Value::String(String::new()))
                        } else {
                            None
                        }
                    });
                (val, 1)
            }
            "agent_name" | "current_agent" => (
                Some(serde_json::Value::String(
                    context.current_agent.clone().unwrap_or_default(),
                )),
                1,
            ),
            "tool_name" | "current_tool" => (
                Some(serde_json::Value::String(
                    context.current_tool.clone().unwrap_or_default(),
                )),
                1,
            ),
            "workflow_name" => {
                let wf = context
                    .state
                    .get("workflow_name")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::String(String::new()));
                (Some(wf), 1)
            }
            "args" if had_args_prefix => {
                // `args.args.<field>` accesses tool call arguments
                if parts.len() >= 2 {
                    let key = parts[1];
                    let from_tool_input = context
                        .tool_input
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .and_then(|v| v.get(key).cloned());
                    let val = from_tool_input
                        .or_else(|| context.state.get(key).cloned())
                        .unwrap_or_else(|| serde_json::Value::String(String::new()));
                    (Some(val), 2)
                } else {
                    (None, 1)
                }
            }
            ident => {
                let val = context
                    .state
                    .get(ident)
                    .cloned()
                    .or_else(|| {
                        context
                            .agent_outputs
                            .get(ident)
                            .map(|s| serde_json::Value::String(s.clone()))
                    })
                    .or_else(|| context.workflow_inputs.get(ident).cloned());
                (val, 1)
            }
        };

        let mut current = root?;
        for part in &parts[consumed..] {
            current = current.get(part)?.clone();
        }
        Some(current)
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
static STARTS_WITH_PATTERN: OnceLock<Regex> = OnceLock::new();
static MATCHES_PATTERN: OnceLock<Regex> = OnceLock::new();
static COMPARISON_PATTERN: OnceLock<Regex> = OnceLock::new();
static EQUALITY_PATTERN: OnceLock<Regex> = OnceLock::new();

fn get_contains_pattern() -> &'static Regex {
    CONTAINS_PATTERN.get_or_init(|| {
        Regex::new(r#"^([\w.()]+)\.contains\((["'])(.*?)["']\)$"#).expect("Invalid regex")
    })
}

fn get_starts_with_pattern() -> &'static Regex {
    STARTS_WITH_PATTERN.get_or_init(|| {
        Regex::new(r#"^([\w.()]+)\.starts_with\((["'])(.*?)["']\)$"#).expect("Invalid regex")
    })
}

fn get_matches_pattern() -> &'static Regex {
    MATCHES_PATTERN.get_or_init(|| {
        Regex::new(r#"^([\w.()]+)\.matches\((["'])(.*?)["']\)$"#).expect("Invalid regex")
    })
}

fn get_comparison_pattern() -> &'static Regex {
    COMPARISON_PATTERN
        .get_or_init(|| Regex::new(r"^([\w.()]+)\s*([><]=?)\s*(.+)$").expect("Invalid regex"))
}

fn get_equality_pattern() -> &'static Regex {
    EQUALITY_PATTERN
        .get_or_init(|| Regex::new(r"^([\w.()]+)\s*([!=]=)\s*(.+)$").expect("Invalid regex"))
}

#[cfg(test)]
#[path = "../../../tests/unit/swl/guardrails/engine/engine_test.rs"]
mod tests;
