# Selfware Agentic Benchmark Suite (SAB) Report

## Summary

| Metric | Value |
|--------|-------|
| Date | 20260301-035609 |
| Model | Qwen/Qwen3-Coder-Next-FP8 |
| Endpoint | https://crazyshit.ngrok.io/v1 |
| Max Context | 1,010,000 tokens |
| Total Scenarios | 12 |
| Completed | 12 |
| Passed (tests green) | 12/12 |
| Average Score | 95/100 |
| Overall Rating | **🌸 BLOOM** |
| Total Duration | 12m 4s |

### Rating Distribution

| Rating | Count | Description |
|--------|-------|-------------|
| 🌸 BLOOM | 10 | Ship it. Model handles this reliably. |
| 🌿 GROW | 2 | Usable with occasional human review. |
| 🥀 WILT | 0 | Model struggles. Needs prompt tuning. |
| ❄️ FROST | 0 | Not ready for this task class. |

## Detailed Results

| Scenario | Difficulty | Score | Rating | Duration | Baseline | Post | Agent Exit | Timeout | Changed | Errors |
|----------|-----------|-------|--------|----------|----------|------|------------|---------|---------|--------|
| `codegen_task_runner` | hard | 100/100 | 🌸 BLOOM | 85s | 101 | 0 | 0 | 0 | 0 | 4 |
| `easy_calculator` | easy | 100/100 | 🌸 BLOOM | 55s | 101 | 0 | 0 | 0 | 0 | 5 |
| `easy_string_ops` | easy | 100/100 | 🌸 BLOOM | 78s | 101 | 0 | 0 | 0 | 0 | 3 |
| `expert_async_race` | expert | 100/100 | 🌸 BLOOM | 77s | 101 | 0 | 0 | 0 | 0 | 7 |
| `hard_event_bus` | hard | 100/100 | 🌸 BLOOM | 309s | 101 | 0 | 0 | 0 | 0 | 8 |
| `hard_scheduler` | hard | 100/100 | 🌸 BLOOM | 51s | 101 | 0 | 0 | 0 | 0 | 4 |
| `medium_json_merge` | medium | 100/100 | 🌸 BLOOM | 272s | 101 | 0 | 0 | 0 | 0 | 38 |
| `perf_optimization` | hard | 100/100 | 🌸 BLOOM | 361s | 124 | 0 | 0 | 0 | 0 | 0 |
| `security_audit` | hard | 100/100 | 🌸 BLOOM | 164s | 101 | 0 | 0 | 0 | 0 | 8 |
| `medium_bitset` | medium | 90/100 | 🌸 BLOOM | 310s | 101 | 0 | 124 | 1 | 0 | 1 |
| `refactor_monolith` | medium | 80/100 | 🌿 GROW | 233s | 0 | 0 | 0 | 0 | 0 | 6 |
| `testgen_ringbuf` | medium | 70/100 | 🌿 GROW | 490s | 0 | 0 | 124 | 1 | 0 | 0 |

## Category Breakdown

### Easy (2/2 passed, avg 100/100)

- `easy_calculator`: 🌸 100/100 in 55s
- `easy_string_ops`: 🌸 100/100 in 78s

### Medium (4/4 passed, avg 85/100)

- `medium_bitset`: 🌸 90/100 in 310s
- `medium_json_merge`: 🌸 100/100 in 272s
- `refactor_monolith`: 🌿 80/100 in 233s
- `testgen_ringbuf`: 🌿 70/100 in 490s

### Hard (5/5 passed, avg 100/100)

- `codegen_task_runner`: 🌸 100/100 in 85s
- `hard_event_bus`: 🌸 100/100 in 309s
- `hard_scheduler`: 🌸 100/100 in 51s
- `perf_optimization`: 🌸 100/100 in 361s
- `security_audit`: 🌸 100/100 in 164s

### Expert (1/1 passed, avg 100/100)

- `expert_async_race`: 🌸 100/100 in 77s

## Error Highlights

### easy_calculator
```
57:2. **`divide`** - panics on division by zero instead of returning `None`
71:<arguments>{"new_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    if b == 0 {\n        None\n    } else {\n        Some(a / b)\n    }\n}", "old_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    // BUG: division by zero currently panics instead of returning None\n    Some(a / b)\n}"}</arguments>
77:<arguments>{"new_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    if b == 0 {\n        None\n    } else {\n        Some(a / b)\n    }\n}", "old_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    // BUG: division by zero currently panics instead of returning None\n    Some(a / b)\n}", "path": "./src/lib.rs"}</arguments>
113:2. **`divide`** - Added check for `b == 0` to return `None` instead of panicking
125:2. **`divide`** - Added check for `b == 0` to return `None` instead of panicking
```

### easy_string_ops
```
47:2. **truncate**: Off-by-one error (uses `max_len.saturating_sub(1)` instead of `max_len`)
101:**2. `truncate`** - Fixed off-by-one error
120:**2. `truncate`** - Fixed off-by-one error
```

### medium_json_merge
```
53:║ Status: ✗ FAILED                        ║
60:║   • Fix type errors before proceeding
64:Let me check the compilation error first:
75:<arguments>{"instruction": "Fix the type error by cloning the base_subobj to get an owned Value instead of a reference", "path": "src/lib.rs", "old_str": "                    (Some(Value::Object(base_subobj)), Value::Object(patch_subobj)) => {\n                        // Recursively merge nested objects\n                        let merged_subobj = merge_json(base_subobj, patch_subobj);\n                        merged.insert(key.clone(), merged_subobj);\n                    }", "new_str": "                    (Some(Value::Object(base_subobj)), Value::Object(patch_subobj)) => {\n                        // Recursively merge nested objects\n                        let merged_subobj = merge_json(base_subobj, patch_subobj);\n                        merged.insert(key.clone(), merged_subobj);\n                    }"}</arguments>
89:<arguments>{"instruction": "Fix the type error by using &*base_subobj to dereference the reference, or by cloning the value first", "new_str": "                    (Some(base_subobj), Value::Object(patch_subobj)) if matches!(base_subobj, Value::Object(_)) => {\n                        // Recursively merge nested objects\n                        let merged_subobj = merge_json(base_subobj, patch_subobj);\n                        merged.insert(key.clone(), merged_subobj);\n                    }", "old_str": "                    (Some(Value::Object(base_subobj)), Value::Object(patch_subobj)) => {\n                        // Recursively merge nested objects\n                        let merged_subobj = merge_json(base_subobj, patch_subobj);\n                        merged.insert(key.clone(), merged_subobj);\n                    }", "path": "src/lib.rs"}</arguments>
96:║ Status: ✗ FAILED                        ║
103:║   • Fix type errors before proceeding
107:Let me check the exact error:
137:║ Status: ✗ FAILED                        ║
144:║   • Fix type errors before proceeding
148:Let me check the exact error message:
166:║ Status: ✗ FAILED                        ║
173:║   • Fix type errors before proceeding
177:Let me check the exact type mismatch error more carefully. The issue is that `base_subobj` is `&Value` but I need to pass it as `&Value`. Let me try dereferencing it:
188:║ Status: ✗ FAILED                        ║
195:║   • Fix type errors before proceeding
199:Let me check the exact error:
217:║ Status: ✗ FAILED                        ║
224:║   • Fix type errors before proceeding
228:Let me check the exact error message:
246:║ Status: ✗ FAILED                        ║
253:║   • Fix type errors before proceeding
257:Let me check the exact error:
284:║ Status: ✗ FAILED                        ║
292:║   • Fix type errors before proceeding
296:Let me check the exact error:
314:║ Status: ✗ FAILED                        ║
321:║   • Fix type errors before proceeding
325:Let me check the exact error:
343:║ Status: ✗ FAILED                        ║
```

### medium_bitset
```
14:The implementation has several logic bugs that cause tests to fail or panic.
```

### hard_scheduler
```
19:4. next_run_at must not panic on overflow (return None instead).
66:2. `next_run_at` panics on overflow instead of returning `None`
96:   - Changed `next_run_at` to use `checked_add` instead of raw `+` to prevent overflow panics, returning `None` on overflow
108:   - Changed `next_run_at` to use `checked_add` instead of raw `+` to prevent overflow panics, returning `None` on overflow
```

### hard_event_bus
```
175:The test has `display.contains("seq=7")` but the error message says "Display should contain seq: Event { topic: "user.login", data: {}, seq: 7 }". This is confusing. Let me run the test with more debugging:
182:Looking at the test output, the error message says "Display should contain seq: Event { topic: "user.login", data: {}, seq: 7 }". This is the expected format from the test assertion. The test checks if the display contains "seq=7" but the error message shows the expected format. Let me check if my current format produces `seq: 7` but the test expects `seq=7`. Let me look at the test file again more carefully:
210:I see the issue now. Looking at the test output, the error message says "Display should contain seq: Event { topic: "user.login", data: {}, seq: 7 }". This means the test expects the display to contain `seq: 7` (with colon and space), but the assertion checks for `display.contains("seq=7")`.
212:Wait, let me re-read the test. The assertion is `display.contains("seq=7")` but the error message says "Display should contain seq: Event { topic: "user.login", data: {}, seq: 7 }". This is confusing. Let me check if the test file was modified or if I'm misreading it.
217:The error message "Display should contain seq: Event { topic: "user.login", data: {}, seq: 7 }" is the formatted error message where `{}` is replaced with the actual display output. This means the display output is `Event { topic: "user.login", data: {}, seq: 7 }` but the test expects it to contain `seq=7`.
296:The clippy warnings are just suggestions (not errors). The code compiles and all tests pass. Let me verify the final state of all files and run cargo test one more time:
338:All 7 tests now pass and the code compiles without errors.
359:All 7 tests now pass and the code compiles without errors.
```

### expert_async_race
```
54:3. **Missing `running` decrement in `fail()`**: Only increments failed, doesn't decrement running
77:<arguments>{"new_str": "    /// Mark the current task as failed.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        state.running -= 1;\n        state.failed += 1;\n    }", "old_str": "    /// Mark the current task as failed.\n    ///\n    /// BUG: Increments failed but forgets to decrement running.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        // BUG: missing `state.running -= 1;`\n        state.failed += 1;\n    }", "path": "./src/lib.rs"}</arguments>
82:<arguments>{"new_str": "    /// Return a snapshot of (running, completed, failed).\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let state = self.state.lock().await;\n        (state.running, state.completed, state.failed)\n    }", "old_str": "    /// Return a snapshot of (running, completed, failed).\n    ///\n    /// BUG: Split lock - reads running under one lock acquisition,\n    /// then reads completed and failed under another. The state can\n    /// change between the two reads, giving an inconsistent view.\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let running = {\n            let state = self.state.lock().await;\n            state.running\n        };\n        // ^^^ Lock dropped. State can mutate here.\n        let (completed, failed) = {\n            let state = self.state.lock().await;\n            (state.completed, state.failed)\n        };\n        (running, completed, failed)\n    }", "path": "./src/lib.rs"}</arguments>
109:3. **Missing `running` decrement in `fail()`**: Added `state.running -= 1;` before incrementing failed. Previously failed tasks leaked from the running count.
111:4. **Split-lock in `snapshot()`**: Read all three counters (running, completed, failed) under a single lock acquisition. Previously it took two separate locks, allowing the state to change between reads and producing inconsistent snapshots.
124:3. **Missing `running` decrement in `fail()`**: Added `state.running -= 1;` before incrementing failed. Previously failed tasks leaked from the running count.
126:4. **Split-lock in `snapshot()`**: Read all three counters (running, completed, failed) under a single lock acquisition. Previously it took two separate locks, allowing the state to change between reads and producing inconsistent snapshots.
```

### security_audit
```
22:5. Implement handle_error_secure that returns generic error messages without internal paths.
55:<arguments>{"path": "./src/lib.rs", "old_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    todo!(\"Implement secure version with path canonicalization and prefix validation\")\n}", "new_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    use std::path::PathBuf;\n    \n    // Build the full path\n    let mut full_path = PathBuf::from(base_dir);\n    full_path.push(filename);\n    \n    // Canonicalize the path to resolve any .. or . components\n    let canonical_full = full_path.canonicalize().map_err(|e| format!(\"Failed to canonicalize path: {}\", e))?;\n    \n    // Canonicalize the base directory\n    let canonical_base = PathBuf::from(base_dir).canonicalize()\n        .map_err(|e| format!(\"Failed to canonicalize base dir: {}\", e))?;\n    \n    // Check if the canonicalized path is within the canonicalized base directory\n    if !canonical_full.starts_with(&canonical_base) {\n        return Err(\"Path traversal detected: path is outside base directory\".to_string());\n    }\n    \n    // Read the file\n    std::fs::read_to_string(&canonical_full).map_err(|e| format!(\"Failed to read file: {}\", e))\n}"}</arguments>
118:║ Status: ✗ FAILED                        ║
125:║   • Fix type errors before proceeding
136:Now let me implement the last secure function `handle_error_secure`:
140:<arguments>{"new_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(_path: &str) -> String {\n    \"Error: file not found\".to_string()\n}", "old_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(path: &str) -> String {\n    todo!(\"Implement secure version that returns generic error messages without internal paths\")\n}", "path": "./src/lib.rs"}</arguments>
201:5. **`handle_error_secure`**: Returns a generic error message without revealing internal file paths.
218:5. **`handle_error_secure`**: Returns a generic error message without revealing internal file paths.
```

### perf_optimization
No significant errors captured.

### codegen_task_runner
```
56:<arguments>{"path": "./src/lib.rs", "content": "use serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}\n\nimpl TaskManager {\n    pub fn new() -> Self {\n        TaskManager {\n            tasks: Vec::new(),\n            next_id: 1,\n        }\n    }\n\n    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {\n        let id = self.next_id;\n        self.next_id += 1;\n        let task = Task {\n            id,\n            name: name.to_string(),\n            priority,\n            status: Status::Pending,\n            tags: Vec::new(),\n            created_at: std::time::SystemTime::now()\n                .duration_since(std::time::UNIX_EPOCH)\n                .unwrap()\n                .as_secs(),\n        };\n        self.tasks.push(task);\n        id\n    }\n\n    pub fn get(&self, id: u64) -> Option<&Task> {\n        self.tasks.iter().find(|t| t.id == id)\n    }\n\n    pub fn remove(&mut self, id: u64) -> Option<Task> {\n        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {\n            Some(self.tasks.remove(pos))\n        } else {\n            None\n        }\n    }\n\n    pub fn update_status(&mut self, id: u64, status: Status) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            task.status = status;\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            if !task.tags.contains(&tag.to_string()) {\n                task.tags.push(tag.to_string());\n            }\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn by_status(&self, status: &Status) -> Vec<&Task> {\n        self.tasks.iter().filter(|t| &t.status == status).collect()\n    }\n\n    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {\n        self.tasks.iter().filter(|t| &t.priority == priority).collect()\n    }\n\n    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| t.tags.contains(&tag.to_string()))\n            .collect()\n    }\n\n    pub fn sorted_by_priority(&self) -> Vec<&Task> {\n        let mut result: Vec<&Task> = self.tasks.iter().collect();\n        result.sort_by(|a, b| {\n            let priority_order = |p: &Priority| match p {\n                Priority::Critical => 0,\n                Priority::High => 1,\n                Priority::Medium => 2,\n                Priority::Low => 3,\n            };\n            priority_order(&a.priority).cmp(&priority_order(&b.priority))\n        });\n        result\n    }\n\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should succeed\")\n    }\n\n    pub fn from_json(json: &str) -> Result<Self, String> {\n        serde_json::from_str(json).map_err(|e| e.to_string())\n    }\n\n    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {\n        let before = self.tasks.len();\n        self.tasks.retain(|t| {\n            !(t.status == Status::Completed && t.created_at < older_than)\n        });\n        before - self.tasks.len()\n    }\n}\n"}</arguments>
63:║ Status: ✗ FAILED                        ║
71:║   • Fix type errors before proceeding
75:I need to check the compilation error. Let me run cargo check to see the specific issue:
```

### testgen_ringbuf
No significant errors captured.

### refactor_monolith
```
104:<arguments>{"content": "use crate::store::KvStore;\n\n// -- Serialization ------------------------------------------------------\n\n/// Serialize the store to a JSON string.\npub fn to_json(store: &KvStore) -> String {\n    serde_json::to_string(store).expect(\"serialization should not fail\")\n}\n\n/// Deserialize a store from a JSON string.\npub fn from_json(json: &str) -> Result<KvStore, serde_json::Error> {\n    serde_json::from_str(json)\n}\n", "path": "./src/serialization.rs"}</arguments>
118:║ Status: ✗ FAILED                        ║
129:║   • Fix type errors before proceeding
144:║ Status: ✗ FAILED                        ║
153:║   • Fix type errors before proceeding
210:<arguments>{"content": "use crate::entry::Entry;\nuse serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n// ---------------------------------------------------------------------------\n// KvStore\n// ---------------------------------------------------------------------------\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct KvStore {\n    pub entries: HashMap<String, Entry>,\n    #[serde(skip)]\n    next_ts: u64,\n}\n\nimpl KvStore {\n    /// Create an empty store. The internal timestamp counter starts at 1.\n    pub fn new() -> Self {\n        Self {\n            entries: HashMap::new(),\n            next_ts: 1,\n        }\n    }\n\n    fn tick(&mut self) -> u64 {\n        let ts = self.next_ts;\n        self.next_ts += 1;\n        ts\n    }\n\n    // -- CRUD ---------------------------------------------------------------\n\n    /// Insert or update an entry. Tags are preserved on update.\n    pub fn set(&mut self, key: &str, value: &str) {\n        let ts = self.tick();\n        self.entries\n            .entry(key.to_string())\n            .and_modify(|e| {\n                e.value = value.to_string();\n                e.updated_at = ts;\n            })\n            .or_insert_with(|| Entry::new(key, value, ts));\n    }\n\n    /// Return the value for `key`, if it exists.\n    pub fn get(&self, key: &str) -> Option<&str> {\n        self.entries.get(key).map(|e| e.value.as_str())\n    }\n\n    /// Remove an entry. Returns `true` if the key existed.\n    pub fn delete(&mut self, key: &str) -> bool {\n        self.entries.remove(key).is_some()\n    }\n\n    /// Return all keys in arbitrary order.\n    pub fn keys(&self) -> Vec<&str> {\n        self.entries.keys().map(|k| k.as_str()).collect()\n    }\n\n    /// Number of entries.\n    pub fn len(&self) -> usize {\n        self.entries.len()\n    }\n\n    /// Whether the store is empty.\n    pub fn is_empty(&self) -> bool {\n        self.entries.is_empty()\n    }\n\n    // -- Tags ---------------------------------------------------------------\n\n    /// Add `tag` to the entry at `key`. Returns `false` if the key does not\n    /// exist or the tag is already present.\n    pub fn add_tag(&mut self, key: &str, tag: &str) -> bool {\n        if let Some(entry) = self.entries.get_mut(key) {\n            if entry.tags.iter().any(|t| t == tag) {\n                return false;\n            }\n            entry.tags.push(tag.to_string());\n            true\n        } else {\n            false\n        }\n    }\n\n    /// Remove `tag` from the entry at `key`. Returns `false` if the key does\n    /// not exist or the tag was not present.\n    pub fn remove_tag(&mut self, key: &str, tag: &str) -> bool {\n        if let Some(entry) = self.entries.get_mut(key) {\n            let before = entry.tags.len();\n            entry.tags.retain(|t| t != tag);\n            entry.tags.len() < before\n        } else {\n            false\n        }\n    }\n\n    // -- Queries ------------------------------------------------------------\n\n    /// Return all entries that carry `tag`.\n    pub fn find_by_tag(&self, tag: &str) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.tags.iter().any(|t| t == tag))\n            .collect()\n    }\n\n    /// Return all entries whose key starts with `prefix`.\n    pub fn find_by_prefix(&self, prefix: &str) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.key.starts_with(prefix))\n            .collect()\n    }\n\n    /// Return all entries whose `updated_at` is strictly greater than `ts`.\n    pub fn find_newer_than(&self, ts: u64) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.updated_at > ts)\n            .collect()\n    }\n\n    // -- Serialization ------------------------------------------------------\n\n    /// Serialize the store to a JSON string.\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should not fail\")\n    }\n\n    /// Deserialize a store from a JSON string.\n    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {\n        serde_json::from_str(json)\n    }\n\n    // -- Merge --------------------------------------------------------------\n\n    /// Merge `other` into `self`. For duplicate keys the entry with the later\n    /// `updated_at` wins. If timestamps are equal the entry from `other` wins.\n    pub fn merge(&mut self, other: &KvStore) {\n        for (key, other_entry) in &other.entries {\n            match self.entries.get(key) {\n                Some(existing) if existing.updated_at > other_entry.updated_at => {\n                    // keep ours\n                }\n                _ => {\n                    self.entries.insert(key.clone(), other_entry.clone());\n                }\n            }\n        }\n    }\n}\n\nimpl Default for KvStore {\n    fn default() -> Self {\n        Self::new()\n    }\n}\n", "path": "./src/store.rs"}</arguments>
```

## Progress Timeline

```
[03:57:04] hard_scheduler: score=100/100 rating=BLOOM duration=51s
[03:57:07] easy_calculator: score=100/100 rating=BLOOM duration=55s
[03:57:30] easy_string_ops: score=100/100 rating=BLOOM duration=78s
[03:58:39] expert_async_race: score=100/100 rating=BLOOM duration=77s
[03:59:57] security_audit: score=100/100 rating=BLOOM duration=164s
[04:00:10] codegen_task_runner: score=100/100 rating=BLOOM duration=85s
[04:00:45] medium_json_merge: score=100/100 rating=BLOOM duration=272s
[04:01:22] hard_event_bus: score=100/100 rating=BLOOM duration=309s
[04:01:23] medium_bitset: score=90/100 rating=BLOOM duration=310s
[04:04:09] refactor_monolith: score=80/100 rating=GROW duration=233s
[04:05:41] perf_optimization: score=100/100 rating=BLOOM duration=361s
[04:08:11] testgen_ringbuf: score=70/100 rating=GROW duration=490s
```

## Artifacts

- Report: `system_tests/projecte2e/reports/20260301-035609/REPORT.md`
- Results: `system_tests/projecte2e/reports/20260301-035609/results/`
- Logs: `system_tests/projecte2e/reports/20260301-035609/logs/<scenario>/`
