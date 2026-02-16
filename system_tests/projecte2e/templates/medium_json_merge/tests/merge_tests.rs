use medium_json_merge::merge_json;
use serde_json::json;

#[test]
fn nested_objects_are_merged_recursively() {
    let base = json!({
        "db": {"host": "localhost", "port": 5432},
        "features": {"a": true, "b": false},
        "list": [1, 2],
        "unchanged": "keep"
    });

    let patch = json!({
        "db": {"port": 5433},
        "features": {"b": true},
        "list": [9],
        "new_key": "added"
    });

    let merged = merge_json(&base, &patch);

    assert_eq!(merged["db"]["host"], "localhost");
    assert_eq!(merged["db"]["port"], 5433);
    assert_eq!(merged["features"]["a"], true);
    assert_eq!(merged["features"]["b"], true);
    assert_eq!(merged["list"], json!([9]));
    assert_eq!(merged["unchanged"], "keep");
    assert_eq!(merged["new_key"], "added");
}

#[test]
fn scalar_patch_replaces_base() {
    let base = json!({"x": 1});
    let patch = json!(42);
    let merged = merge_json(&base, &patch);
    assert_eq!(merged, json!(42));
}

#[test]
fn object_patch_replaces_non_object_base() {
    let base = json!("text");
    let patch = json!({"ok": true});
    let merged = merge_json(&base, &patch);
    assert_eq!(merged, json!({"ok": true}));
}
