use crate::swl::parser::{CodeLanguage, ReduceStage, SwlDocument};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeIssue {
    pub path: String,
    pub message: String,
}

impl TypeIssue {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

pub fn check_codegen_compatibility(doc: &SwlDocument) -> Vec<TypeIssue> {
    let mut issues = Vec::new();

    for (workflow_name, workflow) in &doc.workflows {
        if let Some(ReduceStage::Code(code)) = &workflow.reduce {
            if code.language != CodeLanguage::Rust {
                issues.push(TypeIssue::new(
                    format!("workflows.{workflow_name}.reduce.language"),
                    "current Rust codegen only supports rust reduce blocks",
                ));
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swl::parser::parse_document;

    #[test]
    fn check_codegen_compatibility_accepts_rust_reduce_blocks() {
        let doc = parse_document(
            r#"
version: "2.0"
name: review
agents:
  architect:
    model: mock-model
workflows:
  parallel_review:
    type: map-reduce
    map:
      targets: [architect]
    reduce:
      language: rust
      code: |
        fn merge() {}
"#,
        )
        .unwrap();

        assert!(check_codegen_compatibility(&doc).is_empty());
    }

    #[test]
    fn check_codegen_compatibility_flags_python_reduce_blocks() {
        let doc = parse_document(
            r#"
version: "2.0"
name: review
agents:
  architect:
    model: mock-model
workflows:
  parallel_review:
    type: map-reduce
    map:
      targets: [architect]
    reduce:
      language: python
      code: |
        def merge(results):
            return results
"#,
        )
        .unwrap();

        let issues = check_codegen_compatibility(&doc);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("only supports rust"));
    }
}
