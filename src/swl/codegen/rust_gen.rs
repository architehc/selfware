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
#[path = "../../../tests/unit/swl/codegen/rust_gen/rust_gen_test.rs"]
mod tests;
