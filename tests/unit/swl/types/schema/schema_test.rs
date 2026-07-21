use super::*;

#[test]
fn test_state_schema_new() {
    let schema = StateSchema::new();
    assert!(schema.fields.is_empty());
}

#[test]
fn test_state_schema_add_field() {
    let mut schema = StateSchema::new();
    schema.add_field(StateField {
        name: "test".to_string(),
        field_type: FieldType::String,
        default: None,
        description: None,
    });
    assert_eq!(schema.fields.len(), 1);
}

#[test]
fn test_state_schema_get_field() {
    let mut schema = StateSchema::new();
    schema.add_field(StateField {
        name: "test".to_string(),
        field_type: FieldType::String,
        default: None,
        description: None,
    });

    assert!(schema.get_field("test").is_some());
    assert!(schema.get_field("missing").is_none());
}

#[test]
fn test_state_schema_has_field() {
    let mut schema = StateSchema::new();
    schema.add_field(StateField {
        name: "test".to_string(),
        field_type: FieldType::String,
        default: None,
        description: None,
    });

    assert!(schema.has_field("test"));
    assert!(!schema.has_field("missing"));
}

#[test]
fn test_state_schema_field_names() {
    let mut schema = StateSchema::new();
    schema.add_field(StateField {
        name: "a".to_string(),
        field_type: FieldType::String,
        default: None,
        description: None,
    });
    schema.add_field(StateField {
        name: "b".to_string(),
        field_type: FieldType::Integer,
        default: None,
        description: None,
    });

    let names = schema.field_names();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
}

#[test]
fn test_state_schema_validate_required() {
    let mut schema = StateSchema::new();
    schema.add_field(StateField {
        name: "required".to_string(),
        field_type: FieldType::String,
        default: None,
        description: None,
    });
    schema.add_field(StateField {
        name: "optional".to_string(),
        field_type: FieldType::String,
        default: Some(serde_yaml::Value::String("default".to_string())),
        description: None,
    });

    // Missing required field
    let empty = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    assert!(schema.validate_required(&empty).is_err());

    // Has required field
    let mut map = serde_yaml::Mapping::new();
    map.insert(
        serde_yaml::Value::String("required".to_string()),
        serde_yaml::Value::String("value".to_string()),
    );
    let valid = serde_yaml::Value::Mapping(map);
    assert!(schema.validate_required(&valid).is_ok());
}

#[test]
fn test_state_schema_apply_defaults() {
    let mut schema = StateSchema::new();
    schema.add_field(StateField {
        name: "with_default".to_string(),
        field_type: FieldType::String,
        default: Some(serde_yaml::Value::String("default".to_string())),
        description: None,
    });

    let mut value = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    schema.apply_defaults(&mut value);

    if let serde_yaml::Value::Mapping(map) = value {
        let key = serde_yaml::Value::String("with_default".to_string());
        assert_eq!(
            map.get(&key),
            Some(&serde_yaml::Value::String("default".to_string()))
        );
    }
}

#[test]
fn test_field_type_type_name() {
    assert_eq!(FieldType::String.type_name(), "string");
    assert_eq!(FieldType::Integer.type_name(), "integer");
    assert_eq!(FieldType::Float.type_name(), "float");
    assert_eq!(FieldType::Boolean.type_name(), "boolean");
    assert_eq!(
        FieldType::Array(Box::new(FieldType::String)).type_name(),
        "array"
    );
    assert_eq!(FieldType::AgentRef.type_name(), "agent_ref");
    assert_eq!(FieldType::ToolRef.type_name(), "tool_ref");
}

#[test]
fn test_field_type_is_scalar() {
    assert!(FieldType::String.is_scalar());
    assert!(FieldType::Integer.is_scalar());
    assert!(FieldType::Float.is_scalar());
    assert!(FieldType::Boolean.is_scalar());
    assert!(!FieldType::Array(Box::new(FieldType::String)).is_scalar());
    assert!(!FieldType::Object(HashMap::new()).is_scalar());
    assert!(!FieldType::AgentRef.is_scalar());
}

#[test]
fn test_field_type_is_collection() {
    assert!(!FieldType::String.is_collection());
    assert!(FieldType::Array(Box::new(FieldType::String)).is_collection());
    assert!(FieldType::Object(HashMap::new()).is_collection());
}

#[test]
fn test_field_type_is_reference() {
    assert!(!FieldType::String.is_reference());
    assert!(FieldType::AgentRef.is_reference());
    assert!(FieldType::ToolRef.is_reference());
}

#[test]
fn test_schema_builder_string() {
    let schema = SchemaBuilder::new()
        .string("name")
        .string_with_default("status", "pending")
        .build();

    assert_eq!(schema.fields.len(), 2);
    assert!(matches!(schema.fields[0].field_type, FieldType::String));
    assert!(schema.fields[1].default.is_some());
}

#[test]
fn test_schema_builder_integer() {
    let schema = SchemaBuilder::new()
        .integer("count")
        .integer_with_default("limit", 100)
        .build();

    assert_eq!(schema.fields.len(), 2);
    assert!(matches!(schema.fields[0].field_type, FieldType::Integer));
    assert_eq!(
        schema.fields[1].default.as_ref().unwrap().as_i64(),
        Some(100)
    );
}

#[test]
fn test_schema_builder_float() {
    let schema = SchemaBuilder::new().float("ratio").build();

    assert!(matches!(schema.fields[0].field_type, FieldType::Float));
}

#[test]
fn test_schema_builder_boolean() {
    let schema = SchemaBuilder::new()
        .boolean("active")
        .boolean_with_default("enabled", true)
        .build();

    assert!(matches!(schema.fields[0].field_type, FieldType::Boolean));
    assert_eq!(
        schema.fields[1].default.as_ref().unwrap().as_bool(),
        Some(true)
    );
}

#[test]
fn test_schema_builder_array() {
    let schema = SchemaBuilder::new()
        .array("items", FieldType::String)
        .build();

    assert!(matches!(schema.fields[0].field_type, FieldType::Array(_)));
}

#[test]
fn test_schema_builder_refs() {
    let schema = SchemaBuilder::new()
        .agent_ref("reviewer")
        .tool_ref("file_tool")
        .build();

    assert!(matches!(schema.fields[0].field_type, FieldType::AgentRef));
    assert!(matches!(schema.fields[1].field_type, FieldType::ToolRef));
}

#[test]
fn test_schema_builder_with_description() {
    let schema = SchemaBuilder::new()
        .string("name")
        .with_description("The name field")
        .build();

    assert_eq!(
        schema.fields[0].description,
        Some("The name field".to_string())
    );
}

#[test]
fn test_schema_builder_chaining() {
    let schema = SchemaBuilder::new()
        .string("name")
        .integer("age")
        .boolean("active")
        .float("score")
        .build();

    assert_eq!(schema.fields.len(), 4);
}

#[test]
fn test_field_type_serialization_roundtrip() {
    let types = vec![
        FieldType::String,
        FieldType::Integer,
        FieldType::Float,
        FieldType::Boolean,
        FieldType::AgentRef,
        FieldType::ToolRef,
    ];

    for original in types {
        let yaml = serde_yaml::to_string(&original).unwrap();
        let deserialized: FieldType = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(original, deserialized);
    }
}

#[test]
fn test_array_type_serialization() {
    let yaml = r#"
name: items
type: array
element_type: integer
"#;

    let field: StateField = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        field.field_type,
        FieldType::Array(Box::new(FieldType::Integer))
    );
}

#[test]
fn test_object_type_serialization() {
    let yaml = r#"
name: payload
type: object
"#;

    let field: StateField = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(field.field_type, FieldType::Object(_)));
}
