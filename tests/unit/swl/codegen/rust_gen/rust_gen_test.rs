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
