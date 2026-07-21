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
#[path = "../../../tests/unit/swl/types/checker/checker_test.rs"]
mod tests;
