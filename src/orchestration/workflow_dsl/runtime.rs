//! Runtime -- workflow executor that walks the AST and evaluates nodes.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use super::ast::AstNode;
use super::value::Value;

/// Shared command executor (Arc so it can be cloned across parallel threads).
type SharedCommandExecutor = Arc<dyn Fn(&str) -> (bool, String, String) + Send + Sync>;

/// Workflow runtime/executor
pub struct Runtime {
    /// Global scope
    pub globals: HashMap<String, Value>,
    /// Function definitions
    pub functions: HashMap<String, AstNode>,
    /// Execution history (bounded, O(1) front removal)
    pub history: VecDeque<ExecutionEvent>,
    /// Built-in command executor (Arc-wrapped for sharing across parallel tasks)
    command_executor: Option<SharedCommandExecutor>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("globals", &self.globals)
            .field("functions", &self.functions)
            .field("history_len", &self.history.len())
            .field("command_executor", &self.command_executor.is_some())
            .finish()
    }
}

/// Execution event for tracing
#[derive(Debug, Clone)]
pub struct ExecutionEvent {
    /// Event type
    pub event_type: String,
    /// Step/workflow name
    pub name: String,
    /// Result
    pub result: Option<Value>,
    /// Timestamp
    pub timestamp: u64,
}

/// Maximum number of execution events to retain.
const MAX_RUNTIME_HISTORY: usize = 1000;

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Create a new runtime
    pub fn new() -> Self {
        Self {
            globals: HashMap::new(),
            functions: HashMap::new(),
            history: VecDeque::new(),
            command_executor: None,
        }
    }

    /// Set command executor
    pub fn with_executor<F>(mut self, executor: F) -> Self
    where
        F: Fn(&str) -> (bool, String, String) + Send + Sync + 'static,
    {
        self.command_executor = Some(Arc::new(executor));
        self
    }

    /// Create a lightweight child runtime that shares the same executor
    /// but has its own copy of globals, functions, and history.
    fn fork(&self) -> Self {
        Self {
            globals: self.globals.clone(),
            functions: self.functions.clone(),
            history: VecDeque::new(),
            command_executor: self.command_executor.clone(),
        }
    }

    /// Execute AST nodes
    pub fn execute(&mut self, nodes: &[AstNode]) -> Result<Value, String> {
        let mut result = Value::Null;

        for node in nodes {
            result = self.eval(node)?;
        }

        Ok(result)
    }

    /// Evaluate a single node
    fn eval(&mut self, node: &AstNode) -> Result<Value, String> {
        match node {
            AstNode::Workflow { name, body } => {
                self.log_event("workflow_start", name);
                let result = self.execute(body)?;
                self.log_event("workflow_end", name);
                Ok(result)
            }

            AstNode::Step { name, command } => {
                self.log_event("step_start", name);
                let cmd_value = self.eval(command)?;
                let cmd_str = cmd_value.as_string();

                let (success, output, error) = if let Some(ref executor) = self.command_executor {
                    executor(&cmd_str)
                } else {
                    // Simulated execution
                    (true, format!("[Executed: {}]", cmd_str), String::new())
                };

                let result = Value::StepResult {
                    name: name.clone(),
                    success,
                    output,
                    error: if error.is_empty() { None } else { Some(error) },
                };

                self.globals.insert(name.clone(), result.clone());
                self.log_event("step_end", name);
                Ok(result)
            }

            AstNode::Parallel { body } => {
                self.log_event("parallel_start", "parallel");

                // Execute each node in its own thread using std::thread::scope,
                // which allows the threads to borrow `self` data through the
                // forked child runtimes.
                type ParallelResult = Result<(Value, HashMap<String, Value>), String>;
                let results: Vec<ParallelResult> = std::thread::scope(|scope| {
                    let handles: Vec<_> = body
                        .iter()
                        .map(|node| {
                            let mut child = self.fork();
                            let node = node.clone();
                            scope.spawn(move || {
                                let result = child.eval(&node)?;
                                // Return result together with any new/updated globals
                                Ok((result, child.globals))
                            })
                        })
                        .collect();

                    handles
                        .into_iter()
                        .map(|h| {
                            h.join()
                                .unwrap_or_else(|_| Err("Parallel step panicked".to_string()))
                        })
                        .collect()
                });

                // Collect values and merge globals from completed steps.
                // Report which steps failed, if any.
                let mut values = Vec::with_capacity(results.len());
                let mut errors = Vec::new();
                for (i, result) in results.into_iter().enumerate() {
                    match result {
                        Ok((value, child_globals)) => {
                            // Merge child globals into parent (later steps win on conflict)
                            for (k, v) in child_globals {
                                self.globals.insert(k, v);
                            }
                            values.push(value);
                        }
                        Err(e) => {
                            errors.push(format!("step {}: {}", i + 1, e));
                            values.push(Value::Null);
                        }
                    }
                }

                self.log_event("parallel_end", "parallel");

                if errors.is_empty() {
                    Ok(Value::Array(values))
                } else {
                    Err(format!("Parallel execution failed: {}", errors.join("; ")))
                }
            }

            AstNode::Sequence { body } => self.execute(body),

            AstNode::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.eval(condition)?;
                if cond.as_bool() {
                    self.execute(then_branch)
                } else if let Some(else_body) = else_branch {
                    self.execute(else_body)
                } else {
                    Ok(Value::Null)
                }
            }

            AstNode::For {
                variable,
                iterable,
                body,
            } => {
                let iter_value = self.eval(iterable)?;
                let items = match iter_value {
                    Value::Array(arr) => arr,
                    _ => return Err("For loop requires iterable".to_string()),
                };

                let mut result = Value::Null;
                for item in items {
                    self.globals.insert(variable.clone(), item);
                    result = self.execute(body)?;
                }

                Ok(result)
            }

            AstNode::While { condition, body } => {
                let mut result = Value::Null;
                let mut iterations = 0;
                const MAX_ITERATIONS: usize = 10000;

                while self.eval(condition)?.as_bool() {
                    result = self.execute(body)?;
                    iterations += 1;
                    if iterations >= MAX_ITERATIONS {
                        return Err("Maximum loop iterations exceeded".to_string());
                    }
                }

                Ok(result)
            }

            AstNode::Let { name, value } => {
                let val = self.eval(value)?;
                self.globals.insert(name.clone(), val.clone());
                Ok(val)
            }

            AstNode::FnDef { name, params, body } => {
                self.functions.insert(name.clone(), node.clone());
                Ok(Value::Function {
                    params: params.clone(),
                    body: body.clone(),
                })
            }

            AstNode::Call { name, args } => {
                // Check built-in functions first
                if let Some(result) = self.call_builtin(name, args)? {
                    return Ok(result);
                }

                // Check user-defined functions
                if let Some(AstNode::FnDef { params, body, .. }) = self.functions.get(name).cloned()
                {
                    if args.len() != params.len() {
                        return Err(format!(
                            "Function {} expects {} arguments",
                            name,
                            params.len()
                        ));
                    }

                    // Bind arguments
                    for (param, arg) in params.iter().zip(args.iter()) {
                        let value = self.eval(arg)?;
                        self.globals.insert(param.clone(), value);
                    }

                    return self.execute(&body);
                }

                Err(format!("Unknown function: {}", name))
            }

            AstNode::Binary {
                left,
                operator,
                right,
            } => {
                let lval = self.eval(left)?;
                let rval = self.eval(right)?;
                self.eval_binary(&lval, operator, &rval)
            }

            AstNode::Unary { operator, operand } => {
                let val = self.eval(operand)?;
                self.eval_unary(operator, &val)
            }

            AstNode::Property { object, property } => {
                let obj = self.eval(object)?;
                match obj {
                    Value::StepResult {
                        success,
                        output,
                        error,
                        ..
                    } => match property.as_str() {
                        "success" => Ok(Value::Boolean(success)),
                        "output" => Ok(Value::String(output)),
                        "error" => Ok(error.map(Value::String).unwrap_or(Value::Null)),
                        _ => Err(format!("Unknown property: {}", property)),
                    },
                    Value::Object(map) => Ok(map.get(property).cloned().unwrap_or(Value::Null)),
                    _ => Err(format!("Cannot access property on {:?}", obj)),
                }
            }

            AstNode::Pipeline { stages } => {
                let mut input = Value::Null;
                for stage in stages {
                    // Pass previous output as implicit input
                    self.globals.insert("_input".to_string(), input);
                    input = self.eval(stage)?;
                }
                Ok(input)
            }

            AstNode::Return { value } => {
                if let Some(v) = value {
                    self.eval(v)
                } else {
                    Ok(Value::Null)
                }
            }

            AstNode::OnError { handler } => {
                // Register error handler (simplified)
                self.eval(handler)
            }

            AstNode::Identifier(name) => Ok(self.globals.get(name).cloned().unwrap_or(Value::Null)),

            AstNode::StringLit(s) => Ok(Value::String(s.clone())),
            AstNode::IntegerLit(n) => Ok(Value::Integer(*n)),
            AstNode::FloatLit(n) => Ok(Value::Float(*n)),
            AstNode::BooleanLit(b) => Ok(Value::Boolean(*b)),
            AstNode::ArrayLit(elements) => {
                let values: Result<Vec<_>, _> = elements.iter().map(|e| self.eval(e)).collect();
                Ok(Value::Array(values?))
            }

            AstNode::Command(cmd) => Ok(Value::String(cmd.clone())),
        }
    }

    /// Evaluate binary operation
    fn eval_binary(&self, left: &Value, op: &str, right: &Value) -> Result<Value, String> {
        match op {
            "+" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                _ => Ok(Value::String(format!(
                    "{}{}",
                    left.as_string(),
                    right.as_string()
                ))),
            },
            "-" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                _ => Err("Cannot subtract non-numeric values".to_string()),
            },
            "*" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                _ => Err("Cannot multiply non-numeric values".to_string()),
            },
            "/" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) if *b != 0 => Ok(Value::Integer(a / b)),
                (Value::Float(a), Value::Float(b)) if *b != 0.0 => Ok(Value::Float(a / b)),
                _ => Err("Division by zero or invalid types".to_string()),
            },
            "==" => Ok(Value::Boolean(left.as_string() == right.as_string())),
            "!=" => Ok(Value::Boolean(left.as_string() != right.as_string())),
            "<" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a < b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a < b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            ">" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a > b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a > b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            "<=" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a <= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a <= b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            ">=" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a >= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a >= b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            "&&" => Ok(Value::Boolean(left.as_bool() && right.as_bool())),
            "||" => Ok(Value::Boolean(left.as_bool() || right.as_bool())),
            _ => Err(format!("Unknown operator: {}", op)),
        }
    }

    /// Evaluate unary operation
    fn eval_unary(&self, op: &str, val: &Value) -> Result<Value, String> {
        match op {
            "!" => Ok(Value::Boolean(!val.as_bool())),
            "-" => match val {
                Value::Integer(n) => Ok(Value::Integer(-n)),
                Value::Float(n) => Ok(Value::Float(-n)),
                _ => Err("Cannot negate non-numeric value".to_string()),
            },
            _ => Err(format!("Unknown unary operator: {}", op)),
        }
    }

    /// Call built-in function
    fn call_builtin(&mut self, name: &str, args: &[AstNode]) -> Result<Option<Value>, String> {
        match name {
            "print" => {
                let values: Result<Vec<_>, _> = args.iter().map(|a| self.eval(a)).collect();
                let strings: Vec<String> = values?.iter().map(|v| v.as_string()).collect();
                println!("{}", strings.join(" "));
                Ok(Some(Value::Null))
            }
            "len" => {
                if args.len() != 1 {
                    return Err("len expects 1 argument".to_string());
                }
                let val = self.eval(&args[0])?;
                match val {
                    Value::String(s) => Ok(Some(Value::Integer(s.len() as i64))),
                    Value::Array(a) => Ok(Some(Value::Integer(a.len() as i64))),
                    _ => Err("len requires string or array".to_string()),
                }
            }
            "range" => {
                let (start, end) = match args.len() {
                    1 => {
                        let end = match self.eval(&args[0])? {
                            Value::Integer(n) => n,
                            _ => return Err("range expects integer".to_string()),
                        };
                        (0, end)
                    }
                    2 => {
                        let start = match self.eval(&args[0])? {
                            Value::Integer(n) => n,
                            _ => return Err("range expects integers".to_string()),
                        };
                        let end = match self.eval(&args[1])? {
                            Value::Integer(n) => n,
                            _ => return Err("range expects integers".to_string()),
                        };
                        (start, end)
                    }
                    _ => return Err("range expects 1 or 2 arguments".to_string()),
                };
                let values: Vec<Value> = (start..end).map(Value::Integer).collect();
                Ok(Some(Value::Array(values)))
            }
            "env" => {
                if args.len() != 1 {
                    return Err("env expects 1 argument".to_string());
                }
                let key = self.eval(&args[0])?.as_string();
                let value = std::env::var(&key).unwrap_or_default();
                Ok(Some(Value::String(value)))
            }
            _ => Ok(None),
        }
    }

    /// Log execution event
    fn log_event(&mut self, event_type: &str, name: &str) {
        self.history.push_back(ExecutionEvent {
            event_type: event_type.to_string(),
            name: name.to_string(),
            result: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
        while self.history.len() > MAX_RUNTIME_HISTORY {
            self.history.pop_front();
        }
    }

    /// Get variable value
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.globals.get(name)
    }

    /// Set variable value
    pub fn set(&mut self, name: &str, value: Value) {
        self.globals.insert(name.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::workflow_dsl::{run, Lexer, Parser};

    #[test]
    fn test_runtime_literals() {
        let result = run("42").unwrap();
        assert!(matches!(result, Value::Integer(42)));

        let result = run("\"hello\"").unwrap();
        assert!(matches!(result, Value::String(s) if s == "hello"));

        let result = run("true").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_arithmetic() {
        let result = run("1 + 2").unwrap();
        assert!(matches!(result, Value::Integer(3)));

        let result = run("10 - 3").unwrap();
        assert!(matches!(result, Value::Integer(7)));

        let result = run("4 * 5").unwrap();
        assert!(matches!(result, Value::Integer(20)));

        let result = run("15 / 3").unwrap();
        assert!(matches!(result, Value::Integer(5)));
    }

    #[test]
    fn test_runtime_comparison() {
        let result = run("5 > 3").unwrap();
        assert!(matches!(result, Value::Boolean(true)));

        let result = run("5 < 3").unwrap();
        assert!(matches!(result, Value::Boolean(false)));

        let result = run("5 == 5").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_logic() {
        let result = run("true && false").unwrap();
        assert!(matches!(result, Value::Boolean(false)));

        let result = run("true || false").unwrap();
        assert!(matches!(result, Value::Boolean(true)));

        let result = run("!false").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_let() {
        let result = run("let x = 10; x").unwrap();
        assert!(matches!(result, Value::Integer(10)));
    }

    #[test]
    fn test_runtime_if() {
        let result = run("if true { 1 } else { 2 }").unwrap();
        assert!(matches!(result, Value::Integer(1)));

        let result = run("if false { 1 } else { 2 }").unwrap();
        assert!(matches!(result, Value::Integer(2)));
    }

    #[test]
    fn test_runtime_for() {
        let result = run("let sum = 0\n for i in [1, 2, 3] { let sum = sum + i }\n sum").unwrap();
        assert!(matches!(result, Value::Integer(6)));
    }

    #[test]
    fn test_runtime_while() {
        let result = run("let x = 0\n while x < 5 { let x = x + 1 }\n x").unwrap();
        assert!(matches!(result, Value::Integer(5)));
    }

    #[test]
    fn test_runtime_function() {
        let result = run("fn add(a, b) { a + b }\n add(2, 3)").unwrap();
        assert!(matches!(result, Value::Integer(5)));
    }

    #[test]
    fn test_runtime_step() {
        let result = run("step test = \"cargo test\"\n test.success").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_workflow() {
        let result = run("workflow build { let x = 1; let y = 2; x + y }").unwrap();
        assert!(matches!(result, Value::Integer(3)));
    }

    #[test]
    fn test_runtime_array() {
        let result = run("[1, 2, 3]").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 3);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_runtime_builtin_len() {
        let result = run("len(\"hello\")").unwrap();
        assert!(matches!(result, Value::Integer(5)));

        let result = run("len([1, 2, 3])").unwrap();
        assert!(matches!(result, Value::Integer(3)));
    }

    #[test]
    fn test_runtime_builtin_range() {
        let result = run("range(5)").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 5);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_runtime_parallel() {
        let result = run("parallel { let a = 1; let b = 2 }").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 2);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_runtime_parallel_logs_parallel_events() {
        let tokens = Lexer::new("parallel { let a = 1; let b = 2 }").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();
        let mut runtime = Runtime::new();
        let _ = runtime.execute(&ast).unwrap();

        assert!(runtime
            .history
            .iter()
            .any(|e| e.event_type == "parallel_start"));
        assert!(runtime
            .history
            .iter()
            .any(|e| e.event_type == "parallel_end"));
    }

    #[test]
    fn test_runtime_with_executor() {
        let mut runtime = Runtime::new().with_executor(|cmd| {
            if cmd.contains("fail") {
                (false, String::new(), "Command failed".to_string())
            } else {
                (true, format!("Output of: {}", cmd), String::new())
            }
        });

        let tokens = Lexer::new("step test = \"echo hello\"").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        let result = runtime.execute(&ast).unwrap();
        if let Value::StepResult {
            success, output, ..
        } = result
        {
            assert!(success);
            assert!(output.contains("echo hello"));
        } else {
            panic!("Expected step result");
        }
    }

    #[test]
    fn test_complex_workflow() {
        let source = r#"
            workflow build {
                step check = cargo check;
                if check.success {
                    parallel {
                        step test = cargo test;
                        step lint = cargo clippy;
                    }
                }
            }
        "#;

        let result = run(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_runtime_get_set() {
        let mut rt = Runtime::new();
        assert!(rt.get("x").is_none());
        rt.set("x", Value::Integer(42));
        assert!(matches!(rt.get("x"), Some(Value::Integer(42))));
    }

    #[test]
    fn test_runtime_divide_by_zero() {
        let result = run("10 / 0");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("zero") || err.contains("invalid"));
    }

    #[test]
    fn test_runtime_subtract_strings() {
        let result = run("\"a\" - \"b\"");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("subtract") || err.contains("non-numeric"));
    }

    #[test]
    fn test_runtime_negate_string() {
        let result = run("-\"hello\"");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("negate") || err.contains("non-numeric"));
    }

    #[test]
    fn test_runtime_multiply_strings() {
        let result = run("\"a\" * \"b\"");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("multiply") || err.contains("non-numeric"));
    }

    #[test]
    fn test_runtime_float_arithmetic() {
        let result = run("3.14 + 2.86").unwrap();
        if let Value::Float(f) = result {
            assert!((f - 6.0).abs() < 0.001);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_runtime_string_concatenation() {
        let result = run("\"hello\" + \" world\"").unwrap();
        if let Value::String(s) = result {
            assert_eq!(s, "hello world");
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_runtime_unknown_function() {
        let result = run("nonexistent_func()");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown function"));
    }

    #[test]
    fn test_runtime_for_non_array() {
        let result = run("for x in 42 { x }");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("iterable"));
    }

    #[test]
    fn test_runtime_len_error() {
        let result = run("len(42)");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("string or array"));
    }

    #[test]
    fn test_runtime_range_two_args() {
        let result = run("range(2, 5)").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 3);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_runtime_comparison_lte_gte() {
        let result = run("5 <= 5").unwrap();
        assert!(matches!(result, Value::Boolean(true)));

        let result = run("5 >= 6").unwrap();
        assert!(matches!(result, Value::Boolean(false)));

        let result = run("3 != 4").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_if_no_else() {
        let result = run("if false { 1 }").unwrap();
        assert!(matches!(result, Value::Null));
    }
}
