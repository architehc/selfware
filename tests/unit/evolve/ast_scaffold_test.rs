use super::*;

#[test]
fn test_validate_rust_syntax_accepts_valid_rust() {
    let valid_code = r#"
        pub struct Worker {
            pub id: usize,
            pub name: String,
        }

        impl Worker {
            pub fn new(id: usize, name: impl Into<String>) -> Self {
                Self { id, name: name.into() }
            }
        }
    "#;

    assert!(validate_rust_syntax(valid_code).is_ok());
}

#[test]
fn test_validate_rust_syntax_rejects_broken_syntax_with_coordinates() {
    let broken_code = r#"
        pub struct Worker {
            pub id: usize,
            pub name: String
        // Missing closing brace!
        fn broken() {}
    "#;

    let res = validate_rust_syntax(broken_code);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.line.is_some_and(|l| l > 0));
    assert!(err.column.is_some_and(|c| c > 0));
    assert!(err.to_string().contains("at line"));
    assert!(!err.message.is_empty());
}

#[test]
fn test_catalog_items_identifies_all_constructs() {
    let code = r#"
        //! Module doc comment

        /// A worker struct
        pub struct Worker {
            pub id: usize,
        }

        /// An action enum
        pub enum Action {
            Start,
            Stop,
        }

        /// Trait for executing actions
        pub trait Executable {
            fn execute(&self);
        }

        impl Executable for Worker {
            fn execute(&self) {}
        }

        pub fn process_worker(w: &Worker) -> bool {
            true
        }

        pub type WorkerId = usize;
    "#;

    let items = catalog_items(code).expect("should parse catalog");
    assert_eq!(items.len(), 6);

    let struct_item = items.iter().find(|i| i.name == "Worker").unwrap();
    assert_eq!(struct_item.kind, AstItemKind::Struct);
    assert!(struct_item.is_public);
    assert!(struct_item
        .doc_comment
        .as_deref()
        .unwrap()
        .contains("A worker struct"));

    let enum_item = items.iter().find(|i| i.name == "Action").unwrap();
    assert_eq!(enum_item.kind, AstItemKind::Enum);
    assert!(enum_item.is_public);

    let trait_item = items.iter().find(|i| i.name == "Executable").unwrap();
    assert_eq!(trait_item.kind, AstItemKind::Trait);
    assert!(trait_item.is_public);

    let impl_item = items
        .iter()
        .find(|i| i.kind == AstItemKind::ImplBlock)
        .unwrap();
    assert_eq!(impl_item.name, "impl Executable for Worker");

    let fn_item = items.iter().find(|i| i.name == "process_worker").unwrap();
    assert_eq!(fn_item.kind, AstItemKind::Function);
    assert!(fn_item.is_public);

    let type_item = items.iter().find(|i| i.name == "WorkerId").unwrap();
    assert_eq!(type_item.kind, AstItemKind::TypeAlias);
}

#[test]
fn test_add_or_update_derive_appends_to_existing_derives() {
    let source = r#"
#[derive(Debug, Clone)]
pub struct Task {
    pub id: u64,
}
"#;

    let updated = add_or_update_derive(source, "Task", "Serialize").unwrap();
    assert!(updated.contains("#[derive(Debug, Clone, Serialize)]"));
    assert!(validate_rust_syntax(&updated).is_ok());

    // Idempotent: adding existing derive does nothing
    let unchanged = add_or_update_derive(&updated, "Task", "Serialize").unwrap();
    assert_eq!(unchanged, updated);
}

#[test]
fn test_add_or_update_derive_creates_new_derive_if_none_present() {
    let source = r#"
pub struct PlainItem {
    pub value: i32,
}
"#;

    let updated = add_or_update_derive(source, "PlainItem", "Default").unwrap();
    assert!(updated.contains("#[derive(Default)]"));
    assert!(validate_rust_syntax(&updated).is_ok());
}

#[test]
fn test_scaffold_trait_impl_generates_valid_syntax() {
    let stub = scaffold_trait_impl(
        "Runnable",
        "Pipeline",
        &[
            ("run", "&mut self, timeout_secs: u64", "Result<()>"),
            ("cancel", "&self", "()"),
        ],
    );

    assert!(stub.contains("impl Runnable for Pipeline {"));
    assert!(stub.contains("fn run(&mut self, timeout_secs: u64) -> Result<()> {"));
    assert!(stub.contains("fn cancel(&self) {"));
    assert!(stub.contains("todo!(\"implement Runnable.run\")"));
    assert!(stub.contains("todo!(\"implement Runnable.cancel\")"));
}

#[test]
fn a_restricted_visibility_item_is_not_public() {
    let items =
        catalog_items("pub(crate) fn inner() {}\npub fn outer() {}\nfn private() {}\n").unwrap();
    let public: Vec<_> = items
        .iter()
        .filter(|i| i.is_public)
        .map(|i| i.name.as_str())
        .collect();
    assert_eq!(public, vec!["outer"]);
}

#[test]
fn serialize_is_added_next_to_deserialize() {
    // The old text check `line.contains("Serialize")` matched inside
    // `Deserialize` and silently did nothing.
    let source = "#[derive(Deserialize)]\npub struct Task {\n    pub id: u64,\n}\n";
    let updated = add_or_update_derive(source, "Task", "Serialize").unwrap();
    assert_eq!(
        updated,
        "#[derive(Deserialize, Serialize)]\npub struct Task {\n    pub id: u64,\n}\n"
    );
    // A path whose last segment is already listed is a no-op.
    let same = add_or_update_derive(&updated, "Task", "serde::Serialize").unwrap();
    assert_eq!(same, updated);
}

#[test]
fn derive_targets_the_named_type_not_a_prefix_or_a_comment() {
    let source = "// struct Task is described below\n\
                  #[derive(Debug)]\n\
                  pub struct TaskList;\n\
                  \n\
                  pub struct Task;\n";
    let updated = add_or_update_derive(source, "Task", "Clone").unwrap();
    assert!(
        updated.contains("#[derive(Debug)]\npub struct TaskList;"),
        "{updated}"
    );
    assert!(
        updated.contains("#[derive(Clone)]\npub struct Task;\n"),
        "{updated}"
    );
    assert!(updated.ends_with('\n'), "the trailing newline is kept");
}

#[test]
fn multi_line_derive_with_trailing_comma_is_extended_in_place() {
    let source = "#[derive(\n    Debug,\n    Clone,\n)]\nstruct S;\n";
    let updated = add_or_update_derive(source, "S", "Default").unwrap();
    assert_eq!(updated.matches("#[derive(").count(), 1, "{updated}");
    assert!(updated.contains("Clone, Default,"), "{updated}");
    assert!(validate_rust_syntax(&updated).is_ok());
}

#[test]
fn new_derive_goes_above_helper_attributes_and_keeps_indentation() {
    let source = "mod m {\n    /// Doc\n    #[serde(rename_all = \"snake_case\")]\n    pub enum E { A }\n}\n";
    let updated = add_or_update_derive(source, "E", "Serialize").unwrap();
    assert!(
        updated.contains("    /// Doc\n    #[derive(Serialize)]\n    #[serde(rename_all"),
        "{updated}"
    );
}

#[test]
fn derive_injection_rejects_non_paths_and_ambiguous_names() {
    let source = "struct S;\n";
    let err = add_or_update_derive(source, "S", "Debug)] fn evil() {} #[derive(Clone").unwrap_err();
    assert!(err.contains("not a derive path"), "{err}");

    let twice = "mod a { pub struct S; }\nmod b { pub struct S; }\n";
    let err = add_or_update_derive(twice, "S", "Debug").unwrap_err();
    assert!(err.contains("defined 2 times"), "{err}");

    let err = add_or_update_derive("struct S {", "S", "Debug").unwrap_err();
    assert!(err.contains("syntax error"), "{err}");
}

#[test]
fn scaffolded_trait_impl_is_valid_rust() {
    let stub = scaffold_trait_impl(
        "Runnable",
        "Pipeline",
        &[("run", "&mut self", "Result<(), String>")],
    );
    assert!(validate_rust_syntax(&stub).is_ok(), "{stub}");
}
