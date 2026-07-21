use super::*;

#[test]
fn test_parse_rust_function() {
    // Sample Rust input for parser coverage — the body content is irrelevant.
    let code = r#"
pub async fn process_data(input: String) -> Result<(), Error> {
    Ok(())
}
"#;
    let parsed = parse_rust(code);
    assert!(!parsed.symbols.is_empty());

    let func = &parsed.symbols[0];
    assert_eq!(func.name, "process_data");
    assert!(func.signature.contains("process_data"));
}

#[test]
fn test_parse_python_class() {
    let code = r#"
class DataProcessor:
    def process(self, data: str) -> dict:
        return {}
"#;
    let parsed = parse_python(code);
    assert!(!parsed.symbols.is_empty());

    let has_class = parsed.symbols.iter().any(|s| s.kind == SymbolKind::Class);
    assert!(has_class);
}

#[test]
fn test_extract_signatures() {
    let code = r#"
pub fn public_fn() {}
fn private_fn() {}
pub struct PublicStruct;
"#;
    let parsed = parse_rust(code);
    let sigs = extract_signatures(&parsed);

    // Should only get public items
    assert_eq!(sigs.len(), 2);
    assert!(sigs
        .iter()
        .all(|s| matches!(s.visibility, Visibility::Public)));
}
