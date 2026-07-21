use super::*;
use crate::swl::types::schema::{FieldType, SchemaBuilder};

fn create_test_schema() -> StateSchema {
    SchemaBuilder::new()
        .string("name")
        .integer("count")
        .boolean_with_default("active", true)
        .float("score")
        .array("items", FieldType::String)
        .build()
}

#[test]
fn test_validate_string() {
    assert!(validate_value_type(&serde_json::json!("hello"), &FieldType::String).is_ok());
    assert!(validate_value_type(&serde_json::json!(42), &FieldType::String).is_err());
}

#[test]
fn test_validate_integer() {
    assert!(validate_value_type(&serde_json::json!(42), &FieldType::Integer).is_ok());
    assert!(validate_value_type(&serde_json::json!(-10), &FieldType::Integer).is_ok());
    assert!(validate_value_type(&serde_json::json!(3.15), &FieldType::Integer).is_err());
    assert!(validate_value_type(&serde_json::json!("42"), &FieldType::Integer).is_err());
}

#[test]
fn test_validate_float() {
    assert!(validate_value_type(&serde_json::json!(3.15), &FieldType::Float).is_ok());
    assert!(validate_value_type(&serde_json::json!(42), &FieldType::Float).is_ok()); // Integers are valid floats
    assert!(validate_value_type(&serde_json::json!("3.14"), &FieldType::Float).is_err());
}

#[test]
fn test_validate_boolean() {
    assert!(validate_value_type(&serde_json::json!(true), &FieldType::Boolean).is_ok());
    assert!(validate_value_type(&serde_json::json!(false), &FieldType::Boolean).is_ok());
    assert!(validate_value_type(&serde_json::json!(1), &FieldType::Boolean).is_err());
}

#[test]
fn test_validate_array() {
    let string_array = FieldType::Array(Box::new(FieldType::String));
    assert!(validate_value_type(&serde_json::json!(["a", "b", "c"]), &string_array).is_ok());
    assert!(validate_value_type(&serde_json::json!([]), &string_array).is_ok());
    assert!(validate_value_type(&serde_json::json!(["a", 1, "c"]), &string_array).is_err());

    let int_array = FieldType::Array(Box::new(FieldType::Integer));
    assert!(validate_value_type(&serde_json::json!([1, 2, 3]), &int_array).is_ok());
}

#[test]
fn test_validate_object() {
    assert!(validate_value_type(
        &serde_json::json!({"key": "value"}),
        &FieldType::Object(std::collections::HashMap::new())
    )
    .is_ok());
    assert!(validate_value_type(
        &serde_json::json!({}),
        &FieldType::Object(std::collections::HashMap::new())
    )
    .is_ok());
    assert!(validate_value_type(
        &serde_json::json!("not an object"),
        &FieldType::Object(std::collections::HashMap::new())
    )
    .is_err());
}

#[test]
fn test_validate_state_against_schema() {
    let schema = create_test_schema();

    // Valid state
    let mut valid_state = HashMap::new();
    valid_state.insert("name".to_string(), serde_json::json!("test"));
    valid_state.insert("count".to_string(), serde_json::json!(42));
    valid_state.insert("score".to_string(), serde_json::json!(95.5));
    valid_state.insert("items".to_string(), serde_json::json!(["a", "b"]));
    // "active" has default, so not required

    assert!(validate_state_against_schema(&valid_state, &schema).is_ok());

    // Missing required field
    let mut invalid_state = HashMap::new();
    invalid_state.insert("name".to_string(), serde_json::json!("test"));
    // missing "count" which is required (no default)

    assert!(validate_state_against_schema(&invalid_state, &schema).is_err());

    // Wrong type
    let mut wrong_type_state = HashMap::new();
    wrong_type_state.insert("name".to_string(), serde_json::json!("test"));
    wrong_type_state.insert("count".to_string(), serde_json::json!("not a number"));

    assert!(validate_state_against_schema(&wrong_type_state, &schema).is_err());

    // Unknown field
    let mut unknown_field_state = HashMap::new();
    unknown_field_state.insert("name".to_string(), serde_json::json!("test"));
    unknown_field_state.insert("count".to_string(), serde_json::json!(42));
    unknown_field_state.insert("unknown".to_string(), serde_json::json!("value"));

    assert!(validate_state_against_schema(&unknown_field_state, &schema).is_err());
}

#[test]
fn test_apply_defaults() {
    let schema = create_test_schema();
    let mut state = HashMap::new();

    apply_defaults(&mut state, &schema);

    // "active" has default true
    assert_eq!(state.get("active"), Some(&serde_json::json!(true)));
    // "name" has no default
    assert!(!state.contains_key("name"));
}

#[test]
fn test_validate_state_transition() {
    let schema = create_test_schema();

    let mut old_state = HashMap::new();
    old_state.insert("name".to_string(), serde_json::json!("test"));
    old_state.insert("count".to_string(), serde_json::json!(42));
    old_state.insert("score".to_string(), serde_json::json!(95.5));

    // Valid transition: adding a field
    let mut new_state = old_state.clone();
    new_state.insert("active".to_string(), serde_json::json!(true));
    assert!(validate_state_transition(&old_state, &new_state, &schema).is_ok());

    // Invalid transition: removing a required field
    let mut bad_state = old_state.clone();
    bad_state.remove("count");
    assert!(validate_state_transition(&old_state, &bad_state, &schema).is_err());

    // Invalid transition: still missing a required field from the schema
    let mut partial_state = HashMap::new();
    partial_state.insert("name".to_string(), serde_json::json!("test"));
    partial_state.insert("count".to_string(), serde_json::json!(42));
    assert!(validate_state_transition(&old_state, &partial_state, &schema).is_err());

    // Valid transition: removing a field that has a default
    let mut ok_state = old_state.clone();
    ok_state.insert("active".to_string(), serde_json::json!(true));
    ok_state.remove("active");
    assert!(validate_state_transition(&old_state, &ok_state, &schema).is_ok());
}

#[test]
fn test_get_validation_issues() {
    let schema = create_test_schema();
    let mut state = HashMap::new();
    state.insert("name".to_string(), serde_json::json!("test"));
    // missing "count" which is required
    state.insert("unknown".to_string(), serde_json::json!("value"));

    let issues = get_validation_issues(&state, &schema);
    assert_eq!(issues.len(), 3); // Missing required fields + unknown field
}
