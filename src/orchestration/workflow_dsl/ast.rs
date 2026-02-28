//! AST -- Abstract Syntax Tree node definitions for the workflow DSL.

/// AST Node types
#[derive(Debug, Clone)]
pub enum AstNode {
    /// Workflow definition
    Workflow {
        name: String,
        body: Vec<AstNode>,
    },

    /// Step definition
    Step {
        name: String,
        command: Box<AstNode>,
    },

    /// Parallel execution block
    Parallel {
        body: Vec<AstNode>,
    },

    /// Sequence block
    Sequence {
        body: Vec<AstNode>,
    },

    /// If statement
    If {
        condition: Box<AstNode>,
        then_branch: Vec<AstNode>,
        else_branch: Option<Vec<AstNode>>,
    },

    /// For loop
    For {
        variable: String,
        iterable: Box<AstNode>,
        body: Vec<AstNode>,
    },

    /// While loop
    While {
        condition: Box<AstNode>,
        body: Vec<AstNode>,
    },

    /// Variable assignment
    Let {
        name: String,
        value: Box<AstNode>,
    },

    /// Function definition
    FnDef {
        name: String,
        params: Vec<String>,
        body: Vec<AstNode>,
    },

    /// Function call
    Call {
        name: String,
        args: Vec<AstNode>,
    },

    /// Binary expression
    Binary {
        left: Box<AstNode>,
        operator: String,
        right: Box<AstNode>,
    },

    /// Unary expression
    Unary {
        operator: String,
        operand: Box<AstNode>,
    },

    /// Property access
    Property {
        object: Box<AstNode>,
        property: String,
    },

    /// Pipeline
    Pipeline {
        stages: Vec<AstNode>,
    },

    /// Return statement
    Return {
        value: Option<Box<AstNode>>,
    },

    /// Error handler
    OnError {
        handler: Box<AstNode>,
    },

    /// Literals
    Identifier(String),
    StringLit(String),
    IntegerLit(i64),
    FloatLit(f64),
    BooleanLit(bool),
    ArrayLit(Vec<AstNode>),

    /// Command (for shell execution)
    Command(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_nodes_construction() {
        let id = AstNode::Identifier("my_var".into());
        let s = AstNode::StringLit("hello".into());
        let i = AstNode::IntegerLit(42);
        let f = AstNode::FloatLit(3.14);
        let b = AstNode::BooleanLit(true);
        let cmd = AstNode::Command("echo hello".into());

        // Verify Debug output contains expected values
        assert!(format!("{:?}", id).contains("my_var"));
        assert!(format!("{:?}", s).contains("hello"));
        assert!(format!("{:?}", i).contains("42"));
        assert!(format!("{:?}", f).contains("3.14"));
        assert!(format!("{:?}", b).contains("true"));
        assert!(format!("{:?}", cmd).contains("echo hello"));
    }

    #[test]
    fn test_workflow_node() {
        let wf = AstNode::Workflow {
            name: "build".into(),
            body: vec![
                AstNode::Step {
                    name: "check".into(),
                    command: Box::new(AstNode::Command("cargo check".into())),
                },
            ],
        };

        if let AstNode::Workflow { name, body } = &wf {
            assert_eq!(name, "build");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Workflow node");
        }
    }

    #[test]
    fn test_step_node() {
        let step = AstNode::Step {
            name: "test".into(),
            command: Box::new(AstNode::Command("cargo test".into())),
        };

        if let AstNode::Step { name, command } = &step {
            assert_eq!(name, "test");
            assert!(matches!(command.as_ref(), AstNode::Command(c) if c == "cargo test"));
        } else {
            panic!("Expected Step node");
        }
    }

    #[test]
    fn test_parallel_and_sequence_blocks() {
        let par = AstNode::Parallel {
            body: vec![
                AstNode::Command("cargo clippy".into()),
                AstNode::Command("cargo fmt".into()),
            ],
        };

        if let AstNode::Parallel { body } = &par {
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected Parallel node");
        }

        let seq = AstNode::Sequence {
            body: vec![
                AstNode::Command("cargo build".into()),
                AstNode::Command("cargo test".into()),
            ],
        };

        if let AstNode::Sequence { body } = &seq {
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected Sequence node");
        }
    }

    #[test]
    fn test_if_node_with_else() {
        let if_node = AstNode::If {
            condition: Box::new(AstNode::BooleanLit(true)),
            then_branch: vec![AstNode::Command("echo yes".into())],
            else_branch: Some(vec![AstNode::Command("echo no".into())]),
        };

        if let AstNode::If { condition, then_branch, else_branch } = &if_node {
            assert!(matches!(condition.as_ref(), AstNode::BooleanLit(true)));
            assert_eq!(then_branch.len(), 1);
            assert!(else_branch.is_some());
            assert_eq!(else_branch.as_ref().unwrap().len(), 1);
        } else {
            panic!("Expected If node");
        }
    }

    #[test]
    fn test_if_node_without_else() {
        let if_node = AstNode::If {
            condition: Box::new(AstNode::BooleanLit(false)),
            then_branch: vec![AstNode::Command("echo yes".into())],
            else_branch: None,
        };

        if let AstNode::If { else_branch, .. } = &if_node {
            assert!(else_branch.is_none());
        } else {
            panic!("Expected If node");
        }
    }

    #[test]
    fn test_for_loop_node() {
        let for_node = AstNode::For {
            variable: "item".into(),
            iterable: Box::new(AstNode::ArrayLit(vec![
                AstNode::IntegerLit(1),
                AstNode::IntegerLit(2),
                AstNode::IntegerLit(3),
            ])),
            body: vec![AstNode::Call {
                name: "process".into(),
                args: vec![AstNode::Identifier("item".into())],
            }],
        };

        if let AstNode::For { variable, iterable, body } = &for_node {
            assert_eq!(variable, "item");
            assert!(matches!(iterable.as_ref(), AstNode::ArrayLit(items) if items.len() == 3));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected For node");
        }
    }

    #[test]
    fn test_binary_and_unary_expressions() {
        let binary = AstNode::Binary {
            left: Box::new(AstNode::IntegerLit(1)),
            operator: "+".into(),
            right: Box::new(AstNode::IntegerLit(2)),
        };

        if let AstNode::Binary { operator, .. } = &binary {
            assert_eq!(operator, "+");
        } else {
            panic!("Expected Binary node");
        }

        let unary = AstNode::Unary {
            operator: "!".into(),
            operand: Box::new(AstNode::BooleanLit(false)),
        };

        if let AstNode::Unary { operator, .. } = &unary {
            assert_eq!(operator, "!");
        } else {
            panic!("Expected Unary node");
        }
    }

    #[test]
    fn test_let_and_fn_def_nodes() {
        let let_node = AstNode::Let {
            name: "x".into(),
            value: Box::new(AstNode::IntegerLit(42)),
        };

        if let AstNode::Let { name, .. } = &let_node {
            assert_eq!(name, "x");
        } else {
            panic!("Expected Let node");
        }

        let fn_node = AstNode::FnDef {
            name: "greet".into(),
            params: vec!["name".into()],
            body: vec![AstNode::Return {
                value: Some(Box::new(AstNode::StringLit("hello".into()))),
            }],
        };

        if let AstNode::FnDef { name, params, body } = &fn_node {
            assert_eq!(name, "greet");
            assert_eq!(params, &["name"]);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected FnDef node");
        }
    }

    #[test]
    fn test_property_access_node() {
        let prop = AstNode::Property {
            object: Box::new(AstNode::Identifier("step1".into())),
            property: "success".into(),
        };

        if let AstNode::Property { object, property } = &prop {
            assert!(matches!(object.as_ref(), AstNode::Identifier(id) if id == "step1"));
            assert_eq!(property, "success");
        } else {
            panic!("Expected Property node");
        }
    }

    #[test]
    fn test_pipeline_node() {
        let pipe = AstNode::Pipeline {
            stages: vec![
                AstNode::Command("cargo build".into()),
                AstNode::Command("cargo test".into()),
                AstNode::Command("cargo clippy".into()),
            ],
        };

        if let AstNode::Pipeline { stages } = &pipe {
            assert_eq!(stages.len(), 3);
        } else {
            panic!("Expected Pipeline node");
        }
    }

    #[test]
    fn test_return_node_with_and_without_value() {
        let ret_with = AstNode::Return {
            value: Some(Box::new(AstNode::IntegerLit(0))),
        };
        if let AstNode::Return { value } = &ret_with {
            assert!(value.is_some());
        } else {
            panic!("Expected Return node");
        }

        let ret_without = AstNode::Return { value: None };
        if let AstNode::Return { value } = &ret_without {
            assert!(value.is_none());
        } else {
            panic!("Expected Return node");
        }
    }

    #[test]
    fn test_on_error_node() {
        let on_err = AstNode::OnError {
            handler: Box::new(AstNode::Call {
                name: "rollback".into(),
                args: vec![],
            }),
        };

        if let AstNode::OnError { handler } = &on_err {
            assert!(matches!(handler.as_ref(), AstNode::Call { name, .. } if name == "rollback"));
        } else {
            panic!("Expected OnError node");
        }
    }

    #[test]
    fn test_array_literal_node() {
        let arr = AstNode::ArrayLit(vec![
            AstNode::StringLit("a".into()),
            AstNode::StringLit("b".into()),
        ]);

        if let AstNode::ArrayLit(items) = &arr {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected ArrayLit node");
        }

        // Empty array
        let empty = AstNode::ArrayLit(vec![]);
        if let AstNode::ArrayLit(items) = &empty {
            assert!(items.is_empty());
        } else {
            panic!("Expected ArrayLit node");
        }
    }

    #[test]
    fn test_clone_preserves_deep_structure() {
        let original = AstNode::Workflow {
            name: "test".into(),
            body: vec![
                AstNode::If {
                    condition: Box::new(AstNode::Binary {
                        left: Box::new(AstNode::Identifier("x".into())),
                        operator: ">".into(),
                        right: Box::new(AstNode::IntegerLit(0)),
                    }),
                    then_branch: vec![AstNode::Command("echo positive".into())],
                    else_branch: Some(vec![AstNode::Command("echo non-positive".into())]),
                },
            ],
        };

        let cloned = original.clone();

        // Verify structural equivalence via Debug output
        assert_eq!(format!("{:?}", original), format!("{:?}", cloned));
    }

    #[test]
    fn test_while_loop_node() {
        let while_node = AstNode::While {
            condition: Box::new(AstNode::BooleanLit(true)),
            body: vec![AstNode::Command("do_work".into())],
        };

        if let AstNode::While { condition, body } = &while_node {
            assert!(matches!(condition.as_ref(), AstNode::BooleanLit(true)));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected While node");
        }
    }

    #[test]
    fn test_call_node_with_args() {
        let call = AstNode::Call {
            name: "build_project".into(),
            args: vec![
                AstNode::StringLit("release".into()),
                AstNode::BooleanLit(true),
            ],
        };

        if let AstNode::Call { name, args } = &call {
            assert_eq!(name, "build_project");
            assert_eq!(args.len(), 2);
        } else {
            panic!("Expected Call node");
        }
    }
}
