use serde_json::Value;

pub fn merge_json(base: &Value, patch: &Value) -> Value {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            let mut merged = base_map.clone();
            for (key, patch_value) in patch_map {
                if merged.contains_key(key) {
                    let base_value = merged.get(key).expect("key exists after contains_key check");
                    merged.insert(key.clone(), merge_json(base_value, patch_value));
                } else {
                    merged.insert(key.clone(), patch_value.clone());
                }
            }
            Value::Object(merged)
        }
        // Non-object patch replaces base.
        (_, other) => other.clone(),
    }
}
