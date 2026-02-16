use serde_json::Value;

pub fn merge_json(base: &Value, patch: &Value) -> Value {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            let mut merged = base_map.clone();
            // BUG: this is shallow merge only; nested objects are overwritten.
            for (key, patch_value) in patch_map {
                merged.insert(key.clone(), patch_value.clone());
            }
            Value::Object(merged)
        }
        // Non-object patch replaces base.
        (_, other) => other.clone(),
    }
}
