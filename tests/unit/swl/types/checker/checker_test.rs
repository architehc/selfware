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
