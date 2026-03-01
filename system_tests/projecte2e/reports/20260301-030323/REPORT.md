# Selfware Agentic Benchmark Suite (SAB) Report

## Summary

| Metric | Value |
|--------|-------|
| Date | 20260301-030323 |
| Model | Qwen/Qwen3-Coder-Next-FP8 |
| Endpoint | https://crazyshit.ngrok.io/v1 |
| Max Context | 1,010,000 tokens |
| Total Scenarios | 12 |
| Completed | 12 |
| Passed (tests green) | 11/12 |
| Average Score | 87/100 |
| Overall Rating | **🌸 BLOOM** |
| Total Duration | 9m 42s |

### Rating Distribution

| Rating | Count | Description |
|--------|-------|-------------|
| 🌸 BLOOM | 9 | Ship it. Model handles this reliably. |
| 🌿 GROW | 2 | Usable with occasional human review. |
| 🥀 WILT | 0 | Model struggles. Needs prompt tuning. |
| ❄️ FROST | 1 | Not ready for this task class. |

## Detailed Results

| Scenario | Difficulty | Score | Rating | Duration | Baseline | Post | Agent Exit | Timeout | Changed | Errors |
|----------|-----------|-------|--------|----------|----------|------|------------|---------|---------|--------|
| `codegen_task_runner` | hard | 100/100 | 🌸 BLOOM | 91s | 101 | 0 | 0 | 0 | 0 | 6 |
| `easy_calculator` | easy | 100/100 | 🌸 BLOOM | 59s | 101 | 0 | 0 | 0 | 0 | 4 |
| `easy_string_ops` | easy | 100/100 | 🌸 BLOOM | 81s | 101 | 0 | 0 | 0 | 0 | 3 |
| `expert_async_race` | expert | 100/100 | 🌸 BLOOM | 70s | 101 | 0 | 0 | 0 | 0 | 6 |
| `hard_event_bus` | hard | 100/100 | 🌸 BLOOM | 86s | 101 | 0 | 0 | 0 | 0 | 3 |
| `hard_scheduler` | hard | 100/100 | 🌸 BLOOM | 67s | 101 | 0 | 0 | 0 | 0 | 6 |
| `medium_json_merge` | medium | 100/100 | 🌸 BLOOM | 57s | 101 | 0 | 0 | 0 | 0 | 0 |
| `perf_optimization` | hard | 100/100 | 🌸 BLOOM | 359s | 124 | 0 | 0 | 0 | 0 | 1 |
| `security_audit` | hard | 100/100 | 🌸 BLOOM | 325s | 101 | 0 | 0 | 0 | 0 | 9 |
| `refactor_monolith` | medium | 80/100 | 🌿 GROW | 238s | 0 | 0 | 0 | 0 | 0 | 13 |
| `testgen_ringbuf` | medium | 70/100 | 🌿 GROW | 490s | 0 | 0 | 124 | 1 | 0 | 0 |
| `medium_bitset` | medium | 0/100 | ❄️ FROST | 310s | 101 | 101 | 124 | 1 | 0 | 1 |

## Category Breakdown

### Easy (2/2 passed, avg 100/100)

- `easy_calculator`: 🌸 100/100 in 59s
- `easy_string_ops`: 🌸 100/100 in 81s

### Medium (3/4 passed, avg 62/100)

- `medium_bitset`: ❄️ 0/100 in 310s
- `medium_json_merge`: 🌸 100/100 in 57s
- `refactor_monolith`: 🌿 80/100 in 238s
- `testgen_ringbuf`: 🌿 70/100 in 490s

### Hard (5/5 passed, avg 100/100)

- `codegen_task_runner`: 🌸 100/100 in 91s
- `hard_event_bus`: 🌸 100/100 in 86s
- `hard_scheduler`: 🌸 100/100 in 67s
- `perf_optimization`: 🌸 100/100 in 359s
- `security_audit`: 🌸 100/100 in 325s

### Expert (1/1 passed, avg 100/100)

- `expert_async_race`: 🌸 100/100 in 70s

## Error Highlights

### easy_calculator
```
47:2. **`divide`** - panics on division by zero instead of returning None
59:<arguments>{"path": "./src/lib.rs", "old_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    // BUG: division by zero currently panics instead of returning None\n    Some(a / b)\n}", "new_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    if b == 0 {\n        None\n    } else {\n        Some(a / b)\n    }\n}"}</arguments>
93:2. **`divide`**: Added check for `b == 0` to return `None` instead of panicking
102:2. **`divide`**: Added check for `b == 0` to return `None` instead of panicking
```

### easy_string_ops
```
47:2. **`truncate`**: Off-by-one error, uses `max_len.saturating_sub(1)` instead of `max_len`
95:2. **`truncate`**: Fixed the off-by-one error by changing `max_len.saturating_sub(1)` to `max_len` so the prefix takes exactly `max_len` characters.
110:2. **`truncate`**: Fixed the off-by-one error by changing `max_len.saturating_sub(1)` to `max_len` so the prefix takes exactly `max_len` characters.
```

### medium_json_merge
No significant errors captured.

### medium_bitset
```
14:The implementation has several logic bugs that cause tests to fail or panic.
```

### hard_scheduler
```
19:4. next_run_at must not panic on overflow (return None instead).
74:Now I need to fix the overflow issue in `scheduler.rs`. The `next_run_at` function should use `checked_add` to prevent panics.
112:   - Use `checked_add` instead of raw `+` to prevent overflow panics
113:   - Return `None` when addition overflows instead of panicking
125:   - Use `checked_add` instead of raw `+` to prevent overflow panics
126:   - Return `None` when addition overflows instead of panicking
```

### hard_event_bus
```
123:All tests pass and clippy has no errors (only a style suggestion). Let me verify the final state of the fixed files:
159:All 7 tests now pass, and cargo clippy shows no errors (only a style suggestion about `Default` implementation which is optional).
181:All 7 tests now pass, and cargo clippy shows no errors (only a style suggestion about `Default` implementation which is optional).
```

### expert_async_race
```
73:<arguments>{"path": "./src/lib.rs", "old_str": "    /// Mark the current task as failed.\n    ///\n    /// BUG: Increments failed but forgets to decrement running.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        // BUG: missing `state.running -= 1;`\n        state.failed += 1;\n    }", "new_str": "    /// Mark the current task as failed.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        state.running -= 1;\n        state.failed += 1;\n    }"}</arguments>
80:<arguments>{"path": "./src/lib.rs", "old_str": "    /// Return a snapshot of (running, completed, failed).\n    ///\n    /// BUG: Split lock - reads running under one lock acquisition,\n    /// then reads completed and failed under another. The state can\n    /// change between the two reads, giving an inconsistent view.\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let running = {\n            let state = self.state.lock().await;\n            state.running\n        };\n        // ^^^ Lock dropped. State can mutate here.\n        let (completed, failed) = {\n            let state = self.state.lock().await;\n            (state.completed, state.failed)\n        };\n        (running, completed, failed)\n    }", "new_str": "    /// Return a snapshot of (running, completed, failed).\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let state = self.state.lock().await;\n        (state.running, state.completed, state.failed)\n    }"}</arguments>
119:3. **`fail()` - Missing counter decrement**: Added `state.running -= 1` to properly track task failures. Previously only incremented `failed`.
121:4. **`snapshot()` - Split lock inconsistency**: Read all three counters (`running`, `completed`, `failed`) under a single lock acquisition. Previously read `running` under one lock, then `completed` and `failed` under another, allowing inconsistent views of the state.
134:3. **`fail()` - Missing counter decrement**: Added `state.running -= 1` to properly track task failures. Previously only incremented `failed`.
136:4. **`snapshot()` - Split lock inconsistency**: Read all three counters (`running`, `completed`, `failed`) under a single lock acquisition. Previously read `running` under one lock, then `completed` and `failed` under another, allowing inconsistent views of the state.
```

### security_audit
```
22:5. Implement handle_error_secure that returns generic error messages without internal paths.
56:<arguments>{"path": "./src/lib.rs", "old_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    todo!(\"Implement secure version with path canonicalization and prefix validation\")\n}", "new_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    let base_path = Path::new(base_dir);\n    let filename_path = Path::new(filename);\n    \n    // Join the paths\n    let joined_path = base_path.join(filename_path);\n    \n    // Canonicalize the resolved path\n    let canonical_path = match joined_path.canonicalize() {\n        Ok(p) => p,\n        Err(e) => return Err(format!(\"Failed to resolve path: {}\", e)),\n    };\n    \n    // Canonicalize the base directory\n    let canonical_base = match base_path.canonicalize() {\n        Ok(p) => p,\n        Err(e) => return Err(format!(\"Failed to resolve base directory: {}\", e)),\n    };\n    \n    // Verify the canonicalized path is within the base directory\n    if !canonical_path.starts_with(&canonical_base) {\n        return Err(\"Path traversal detected: path is outside base directory\".to_string());\n    }\n    \n    // Read the file\n    fs::read_to_string(&canonical_path).map_err(|e| e.to_string())\n}"}</arguments>
76:<arguments>{"new_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(_path: &str) -> String {\n    \"Error: file not found\".to_string()\n}", "old_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(path: &str) -> String {\n    todo!(\"Implement secure version that returns generic error messages without internal paths\")\n}", "path": "./src/lib.rs"}</arguments>
107:5. **`handle_error_secure`** - Returns generic error messages without revealing internal file paths.
131:5. **`handle_error_secure`** - Returns generic error messages without revealing internal file paths.
210:5. **`handle_error_secure`** - Returns generic error messages without revealing internal file paths.
588:5. **Information Leak** - `handle_error_secure` returns generic error messages without path exposure
603:5. **Information Leak** - `handle_error_secure` returns generic error messages without path exposure
612:5. **Information Leak** - `handle_error_secure` returns generic error messages without path exposure
```

### perf_optimization
```
75:The clippy error is unrelated to my code changes - it's a command-line argument parsing issue. Let me verify the code is correct by running cargo check again:
```

### codegen_task_runner
```
56:<arguments>{"content": "use serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}\n\nimpl TaskManager {\n    pub fn new() -> Self {\n        TaskManager {\n            tasks: Vec::new(),\n            next_id: 1,\n        }\n    }\n\n    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {\n        let id = self.next_id;\n        self.next_id += 1;\n        let task = Task {\n            id,\n            name: name.to_string(),\n            priority,\n            status: Status::Pending,\n            tags: Vec::new(),\n            created_at: std::time::SystemTime::now()\n                .duration_since(std::time::UNIX_EPOCH)\n                .unwrap()\n                .as_secs(),\n        };\n        self.tasks.push(task);\n        id\n    }\n\n    pub fn get(&self, id: u64) -> Option<&Task> {\n        self.tasks.iter().find(|t| t.id == id)\n    }\n\n    pub fn remove(&mut self, id: u64) -> Option<Task> {\n        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {\n            Some(self.tasks.remove(pos))\n        } else {\n            None\n        }\n    }\n\n    pub fn update_status(&mut self, id: u64, status: Status) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            task.status = status;\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            if !task.tags.contains(&tag.to_string()) {\n                task.tags.push(tag.to_string());\n            }\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn by_status(&self, status: &Status) -> Vec<&Task> {\n        self.tasks.iter().filter(|t| &t.status == status).collect()\n    }\n\n    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {\n        self.tasks.iter().filter(|t| &t.priority == priority).collect()\n    }\n\n    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| t.tags.iter().any(|t_tag| t_tag == tag))\n            .collect()\n    }\n\n    pub fn sorted_by_priority(&self) -> Vec<&Task> {\n        let mut sorted: Vec<&Task> = self.tasks.iter().collect();\n        sorted.sort_by(|a, b| {\n            let priority_order = |p: &Priority| match p {\n                Priority::Critical => 0,\n                Priority::High => 1,\n                Priority::Medium => 2,\n                Priority::Low => 3,\n            };\n            priority_order(&a.priority).cmp(&priority_order(&b.priority))\n        });\n        sorted\n    }\n\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"Failed to serialize TaskManager\")\n    }\n\n    pub fn from_json(json: &str) -> Result<Self, String> {\n        serde_json::from_str(json).map_err(|e| e.to_string())\n    }\n\n    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {\n        let before = self.tasks.len();\n        self.tasks.retain(|t| {\n            !(t.status == Status::Completed && t.created_at < older_than)\n        });\n        before - self.tasks.len()\n    }\n}", "path": "./src/lib.rs"}</arguments>
63:║ Status: ✗ FAILED                        ║
71:║   • Fix type errors before proceeding
82:Good, serde_json is available. Let me run cargo check to see the exact error:
107:Good, no errors. Now let me run the tests:
145:The clippy error is unrelated to my implementation. Let me verify everything is working correctly:
```

### testgen_ringbuf
No significant errors captured.

### refactor_monolith
```
74:║ Status: ✗ FAILED                        ║
83:║   • Fix type errors before proceeding
98:║ Status: ✗ FAILED                        ║
106:║   • Fix type errors before proceeding
121:║ Status: ✗ FAILED                        ║
130:║   • Fix type errors before proceeding
145:║ Status: ✗ FAILED                        ║
154:║   • Fix type errors before proceeding
169:║ Status: ✗ FAILED                        ║
176:║   • Fix type errors before proceeding
184:<arguments>{"content": "use crate::entry::Entry;\nuse crate::store::KvStore;\nuse serde::{Deserialize, Serialize};\n\n// -- Serialization ------------------------------------------------------\n\n/// Serialize the store to a JSON string.\npub fn to_json(store: &KvStore) -> String {\n    serde_json::to_string(store).expect(\"serialization should not fail\")\n}\n\n/// Deserialize a store from a JSON string.\npub fn from_json(json: &str) -> Result<KvStore, serde_json::Error> {\n    serde_json::from_str(json)\n}\n", "path": "./src/serialization.rs"}</arguments>
219:<arguments>{"new_str": "    // -- Serialization ------------------------------------------------------\n\n    /// Serialize the store to a JSON string.\n    pub fn to_json(&self) -> String {\n        crate::serialization::to_json(self)\n    }\n\n    /// Deserialize a store from a JSON string.\n    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {\n        crate::serialization::from_json(json)\n    }\n\n    // -- Merge --------------------------------------------------------------", "old_str": "    // -- Merge --------------------------------------------------------------", "path": "./src/store.rs"}</arguments>
261:<arguments>{"content": "use crate::store::KvStore;\n\n// -- Serialization ------------------------------------------------------\n\n/// Serialize the store to a JSON string.\npub fn to_json(store: &KvStore) -> String {\n    serde_json::to_string(store).expect(\"serialization should not fail\")\n}\n\n/// Deserialize a store from a JSON string.\npub fn from_json(json: &str) -> Result<KvStore, serde_json::Error> {\n    serde_json::from_str(json)\n}\n", "path": "./src/serialization.rs"}</arguments>
```

## Progress Timeline

```
[03:04:23] easy_calculator: score=100/100 rating=BLOOM duration=59s
[03:04:24] medium_json_merge: score=100/100 rating=BLOOM duration=57s
[03:04:33] hard_scheduler: score=100/100 rating=BLOOM duration=67s
[03:04:48] easy_string_ops: score=100/100 rating=BLOOM duration=81s
[03:04:52] hard_event_bus: score=100/100 rating=BLOOM duration=86s
[03:05:45] expert_async_race: score=100/100 rating=BLOOM duration=70s
[03:06:31] codegen_task_runner: score=100/100 rating=BLOOM duration=91s
[03:08:36] medium_bitset: score=0/100 rating=FROST duration=310s
[03:09:56] refactor_monolith: score=80/100 rating=GROW duration=238s
[03:10:02] security_audit: score=100/100 rating=BLOOM duration=325s
[03:12:32] perf_optimization: score=100/100 rating=BLOOM duration=359s
[03:13:05] testgen_ringbuf: score=70/100 rating=GROW duration=490s
```

## Artifacts

- Report: `system_tests/projecte2e/reports/20260301-030323/REPORT.md`
- Results: `system_tests/projecte2e/reports/20260301-030323/results/`
- Logs: `system_tests/projecte2e/reports/20260301-030323/logs/<scenario>/`
