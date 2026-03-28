use crate::swl::parser::SwlDocument;

pub fn generate_rust_stub(doc: &SwlDocument) -> String {
    let agents = doc
        .agents
        .keys()
        .map(|name| format!("    \"{name}\","))
        .collect::<Vec<_>>()
        .join("\n");

    let workflows = doc
        .workflows
        .keys()
        .map(|name| format!("    \"{name}\","))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "// Generated SWL stub for {name}\n\
         pub const SWL_NAME: &str = \"{name}\";\n\
         pub const SWL_VERSION: &str = \"{version}\";\n\
         pub const SWL_AGENTS: &[&str] = &[\n{agents}\n];\n\
         pub const SWL_WORKFLOWS: &[&str] = &[\n{workflows}\n];\n",
        name = doc.name,
        version = doc.version,
        agents = agents,
        workflows = workflows
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swl::parser::parse_document;

    #[test]
    fn generate_rust_stub_includes_agents_and_workflows() {
        let doc = parse_document(
            r#"
version: "2.0"
name: review
agents:
  architect:
    model: mock-model
workflows:
  parallel_review:
    type: parallel
"#,
        )
        .unwrap();

        let generated = generate_rust_stub(&doc);
        assert!(generated.contains("SWL_NAME"));
        assert!(generated.contains("\"architect\""));
        assert!(generated.contains("\"parallel_review\""));
    }
}
