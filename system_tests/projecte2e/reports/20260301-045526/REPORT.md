# Selfware Agentic Benchmark Suite (SAB) Report

## Summary

| Metric | Value |
|--------|-------|
| Date | 20260301-045526 |
| Model | Qwen/Qwen3-Coder-Next-FP8 |
| Endpoint | https://crazyshit.ngrok.io/v1 |
| Max Context | 1,010,000 tokens |
| Total Scenarios | 12 |
| Completed | 12 |
| Passed (tests green) | 12/12 |
| Average Score | 96/100 |
| Overall Rating | **🌸 BLOOM** |
| Total Duration | 9m 23s |

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
| `codegen_task_runner` | hard | 100/100 | 🌸 BLOOM | 79s | 101 | 0 | 0 | 0 | 0 | 4 |
| `easy_calculator` | easy | 100/100 | 🌸 BLOOM | 35s | 101 | 0 | 0 | 0 | 0 | 4 |
| `easy_string_ops` | easy | 100/100 | 🌸 BLOOM | 70s | 101 | 0 | 0 | 0 | 0 | 3 |
| `expert_async_race` | expert | 100/100 | 🌸 BLOOM | 83s | 101 | 0 | 0 | 0 | 0 | 7 |
| `hard_event_bus` | hard | 100/100 | 🌸 BLOOM | 231s | 101 | 0 | 0 | 0 | 0 | 14 |
| `hard_scheduler` | hard | 100/100 | 🌸 BLOOM | 67s | 101 | 0 | 0 | 0 | 0 | 9 |
| `medium_bitset` | medium | 100/100 | 🌸 BLOOM | 77s | 101 | 0 | 0 | 0 | 0 | 3 |
| `medium_json_merge` | medium | 100/100 | 🌸 BLOOM | 191s | 101 | 0 | 0 | 0 | 0 | 25 |
| `perf_optimization` | hard | 100/100 | 🌸 BLOOM | 357s | 124 | 0 | 0 | 0 | 0 | 1 |
| `security_audit` | hard | 100/100 | 🌸 BLOOM | 77s | 101 | 0 | 0 | 0 | 0 | 6 |
| `refactor_monolith` | medium | 80/100 | 🌿 GROW | 111s | 0 | 0 | 0 | 0 | 0 | 3 |
| `testgen_ringbuf` | medium | 80/100 | 🌿 GROW | 407s | 0 | 0 | 0 | 0 | 0 | 2 |

## Category Breakdown

### Easy (2/2 passed, avg 100/100)

- `easy_calculator`: 🌸 100/100 in 35s
- `easy_string_ops`: 🌸 100/100 in 70s

### Medium (4/4 passed, avg 90/100)

- `medium_bitset`: 🌸 100/100 in 77s
- `medium_json_merge`: 🌸 100/100 in 191s
- `refactor_monolith`: 🌿 80/100 in 111s
- `testgen_ringbuf`: 🌿 80/100 in 407s

### Hard (5/5 passed, avg 100/100)

- `codegen_task_runner`: 🌸 100/100 in 79s
- `hard_event_bus`: 🌸 100/100 in 231s
- `hard_scheduler`: 🌸 100/100 in 67s
- `perf_optimization`: 🌸 100/100 in 357s
- `security_audit`: 🌸 100/100 in 77s

### Expert (1/1 passed, avg 100/100)

- `expert_async_race`: 🌸 100/100 in 83s

## Error Highlights

### easy_calculator
```
45:2. `divide` panics on division by zero instead of returning `None`
57:<arguments>{"new_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    if b == 0 {\n        None\n    } else {\n        Some(a / b)\n    }\n}", "old_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    // BUG: division by zero currently panics instead of returning None\n    Some(a / b)\n}", "path": "./src/lib.rs"}</arguments>
73:2. **`divide`**: Added check for `b == 0` to return `None` instead of panicking
78:2. **`divide`**: Added check for `b == 0` to return `None` instead of panicking
```

### easy_string_ops
```
47:**Bug 2: `truncate` - off-by-one error (uses max_len-1 instead of max_len)**
94:2. **`truncate`** - Fixed off-by-one error
110:2. **`truncate`** - Fixed off-by-one error
```

### medium_json_merge
```
50:║ Status: ✗ FAILED                        ║
57:║   • Fix type errors before proceeding
61:Let me check the cargo_check output to see the actual error:
83:║ Status: ✗ FAILED                        ║
90:║   • Fix type errors before proceeding
94:Let me check the actual error again:
108:I see the issue - `base_sub` is a `&Value` but `merge_json` expects `&Value` as well. The problem is that `merged.get(key)` returns `Option<&Value>`, and when I match on it, `base_sub` is `&Value`. But `merge_json` takes `&Value`, so I need to pass `&base_sub` or adjust the function signature. Let me check the exact type issue more carefully by looking at the error:
123:║ Status: ✗ FAILED                        ║
130:║   • Fix type errors before proceeding
134:Let me check the exact error again with more detail:
159:║ Status: ✗ FAILED                        ║
166:║   • Fix type errors before proceeding
181:║ Status: ✗ FAILED                        ║
188:║   • Fix type errors before proceeding
192:Let me check the exact error with cargo_check:
199:Let me try a different approach - I'll use `cargo test` directly to see the actual compilation error:
217:║ Status: ✗ FAILED                        ║
225:║   • Fix type errors before proceeding
247:║ Status: ✗ FAILED                        ║
256:║   • Fix type errors before proceeding
260:Let me check the actual error:
278:║ Status: ✗ FAILED                        ║
286:║   • Fix type errors before proceeding
290:Let me check the actual error:
297:Let me try a completely different approach - I'll use `cargo_test` directly to see the error:
```

### medium_bitset
```
14:The implementation has several logic bugs that cause tests to fail or panic.
84:- **Problem:** The shift amount was `index` instead of `index % 64`, causing overflow panics when `index >= 64`
105:- **Problem:** The shift amount was `index` instead of `index % 64`, causing overflow panics when `index >= 64`
```

### hard_scheduler
```
19:4. next_run_at must not panic on overflow (return None instead).
59:   - Handle overflow without panicking
69:Now let me fix the scheduler.rs to handle overflow without panicking:
111:- Changed `next_run_at` to use `checked_add` instead of raw `+` to prevent overflow panics
112:- Returns `None` on overflow instead of panicking
118:- `next_run_at_handles_large_values_without_panicking` ✓
131:- Changed `next_run_at` to use `checked_add` instead of raw `+` to prevent overflow panics
132:- Returns `None` on overflow instead of panicking
138:- `next_run_at_handles_large_values_without_panicking` ✓
```

### hard_event_bus
```
85:Good progress! 6 tests pass now. The only remaining issue is the display format. Looking at the error message, the test expects the format to match exactly what's in the assertion. Let me check what format the test expects more carefully:
91:Wait, the error says "Display should contain seq: Event { topic: "user.login", data: {...}, seq: 7 }" - this means the display output should contain that exact string. Let me adjust the format to match exactly:
105:Looking at the error message again: "Display should contain seq: Event { topic: "user.login", data: {...}, seq: 7 }"
127:Looking at the error message again: "Display should contain seq: Event { topic: "user.login", data: {...}, seq: 7 }"
157:Wait, I realize the issue - the test expects "seq=7" but I'm producing "seq: 7". Let me look at the error message more carefully. The error says "Display should contain seq: Event { topic: "user.login", data: {...}, seq: 7 }" - this is the expected output format. But the test assertion is checking for `display.contains("seq=7")`.
171:Looking at the error message more carefully: "Display should contain seq: Event { topic: "user.login", data: {...}, seq: 7 }"
173:The test is checking `display.contains("seq=7")` but the error message shows the expected format. The issue is that my format produces "seq: 7" but the test expects "seq=7" to be in the output.
198:Let me analyze the test output more carefully. The error message says "Display should contain seq: Event { topic: "user.login", data: {...}, seq: 7 }". This is the message from the assertion failure, which means the test is checking if the display contains "seq=7".
222:Let me re-read the test assertion and error message more carefully. The error says "Display should contain seq: Event { topic: "user.login", data: {...}, seq: 7 }". This is the error message from the assertion, not the expected output.
224:Actually, I think I've been misunderstanding the error. Let me look at the test again:
302:All tests pass and clippy shows only warnings (not errors) about adding a Default implementation, which is optional and not required for this task.
321:All 7 tests now pass, and the code compiles without errors.
322:Final answer: All tests pass and clippy shows only warnings (not errors) about adding a Default implementation, which is optional and not required for this task.
341:All 7 tests now pass, and the code compiles without errors.
```

### expert_async_race
```
67:<arguments>{"new_str": "    /// Mark the current task as failed.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        state.running -= 1;\n        state.failed += 1;\n    }", "old_str": "    /// Mark the current task as failed.\n    ///\n    /// BUG: Increments failed but forgets to decrement running.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        // BUG: missing `state.running -= 1;`\n        state.failed += 1;\n    }", "path": "./src/lib.rs"}</arguments>
74:<arguments>{"new_str": "    /// Return a snapshot of (running, completed, failed).\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let state = self.state.lock().await;\n        (state.running, state.completed, state.failed)\n    }", "old_str": "    /// Return a snapshot of (running, completed, failed).\n    ///\n    /// BUG: Split lock - reads running under one lock acquisition,\n    /// then reads completed and failed under another. The state can\n    /// change between the two reads, giving an inconsistent view.\n    pub async fn snapshot(&self) {\n        let running = {\n            let state = self.state.lock().await;\n            state.running\n        };\n        // ^^^ Lock dropped. State can mutate here.\n        let (completed, failed) = {\n            let state = self.state.lock().await;\n            (state.completed, state.failed)\n        };\n        (running, completed, failed)\n    }", "path": "./src/lib.rs"}</arguments>
88:<arguments>{"new_str": "    /// Return a snapshot of (running, completed, failed).\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let state = self.state.lock().await;\n        (state.running, state.completed, state.failed)\n    }", "old_str": "    /// Return a snapshot of (running, completed, failed).\n    ///\n    /// BUG: Split lock - reads running under one lock acquisition,\n    /// then reads completed and failed under another. The state can\n    /// change between the two reads, giving an inconsistent view.\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let running = {\n            let state = self.state.lock().await;\n            state.running\n        };\n        // ^^^ Lock dropped. State can mutate here.\n        let (completed, failed) = {\n            let state = self.state.lock().await;\n            (state.completed, state.failed)\n        };\n        (running, completed, failed)\n    }", "path": "./src/lib.rs"}</arguments>
129:3. **`fail()` - Missing counter decrement**: Added `state.running -= 1` to properly decrement the running count when tasks fail. Previously, only `failed` was incremented but `running` was never decremented.
131:4. **`snapshot()` - Split lock bug**: Combined both lock acquisitions into a single lock to get a consistent view of all counters. Previously, `running` was read under one lock, then `completed` and `failed` were read under another, allowing the state to change between reads and producing inconsistent snapshots.
144:3. **`fail()` - Missing counter decrement**: Added `state.running -= 1` to properly decrement the running count when tasks fail. Previously, only `failed` was incremented but `running` was never decremented.
146:4. **`snapshot()` - Split lock bug**: Combined both lock acquisitions into a single lock to get a consistent view of all counters. Previously, `running` was read under one lock, then `completed` and `failed` were read under another, allowing the state to change between reads and producing inconsistent snapshots.
```

### security_audit
```
22:5. Implement handle_error_secure that returns generic error messages without internal paths.
58:<arguments>{"new_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    let base_path = Path::new(base_dir);\n    let joined_path = base_path.join(filename);\n    \n    // Canonicalize both paths to resolve any .. or . components\n    let canonical_base = base_path\n        .canonicalize()\n        .map_err(|e| format!(\"Failed to canonicalize base directory: {}\", e))?;\n    let canonical_joined = joined_path\n        .canonicalize()\n        .map_err(|e| format!(\"Failed to canonicalize path: {}\", e))?;\n    \n    // Verify the canonicalized path is within the base directory\n    if !canonical_joined.starts_with(&canonical_base) {\n        return Err(\"Path traversal detected: resolved path is outside base directory\".to_string());\n    }\n    \n    // Read the file\n    fs::read_to_string(&canonical_joined)\n        .map_err(|e| format!(\"Failed to read file: {}\", e))\n}", "old_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    todo!(\"Implement secure version with path canonicalization and prefix validation\")\n}", "path": "./src/lib.rs"}</arguments>
89:Now let me implement `handle_error_secure`:
93:<arguments>{"new_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(_path: &str) -> String {\n    \"Error: file not found\".to_string()\n}", "old_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(path: &str) -> String {\n    todo!(\"Implement secure version that returns generic error messages without internal paths\")\n}", "path": "./src/lib.rs"}</arguments>
124:5. **`handle_error_secure`**: Implemented to return a generic error message without revealing internal file paths. The path parameter is now prefixed with `_` to indicate it's used specially.
141:5. **`handle_error_secure`**: Implemented to return a generic error message without revealing internal file paths. The path parameter is now prefixed with `_` to indicate it's used specially.
```

### perf_optimization
```
35:The test timed out, confirming the performance issues. Let me read the current implementation to understand what needs to be optimized.
```

### codegen_task_runner
```
49:<arguments>{"path": "./src/lib.rs", "old_str": "use serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}\n\nimpl TaskManager {\n    pub fn new() -> Self {\n        todo!()\n    }\n\n    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {\n        todo!()\n    }\n\n    pub fn get(&self, id: u64) -> Option<&Task> {\n        todo!()\n    }\n\n    pub fn remove(&mut self, id: u64) -> Option<Task> {\n        todo!()\n    }\n\n    pub fn update_status(&mut self, id: u64, status: Status) -> bool {\n        todo!()\n    }\n\n    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {\n        todo!()\n    }\n\n    pub fn by_status(&self, status: &Status) -> Vec<&Task> {\n        todo!()\n    }\n\n    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {\n        todo!()\n    }\n\n    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {\n        todo!()\n    }\n\n    pub fn sorted_by_priority(&self) -> Vec<&Task> {\n        todo!()\n    }\n\n    pub fn to_json(&self) -> String {\n        todo!()\n    }\n\n    pub fn from_json(json: &str) -> Result<Self, String> {\n        todo!()\n    }\n\n    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {\n        todo!()\n    }\n}", "new_str": "use serde::{Deserialize, Serialize};\nuse std::time::{SystemTime, UNIX_EPOCH};\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}\n\nimpl TaskManager {\n    pub fn new() -> Self {\n        TaskManager {\n            tasks: Vec::new(),\n            next_id: 1,\n        }\n    }\n\n    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {\n        let now = SystemTime::now()\n            .duration_since(UNIX_EPOCH)\n            .unwrap()\n            .as_secs();\n        let id = self.next_id;\n        self.next_id += 1;\n        let task = Task {\n            id,\n            name: name.to_string(),\n            priority,\n            status: Status::Pending,\n            tags: Vec::new(),\n            created_at: now,\n        };\n        self.tasks.push(task);\n        id\n    }\n\n    pub fn get(&self, id: u64) -> Option<&Task> {\n        self.tasks.iter().find(|t| t.id == id)\n    }\n\n    pub fn remove(&mut self, id: u64) -> Option<Task> {\n        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {\n            Some(self.tasks.remove(pos))\n        } else {\n            None\n        }\n    }\n\n    pub fn update_status(&mut self, id: u64, status: Status) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            task.status = status;\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            if !task.tags.contains(&tag.to_string()) {\n                task.tags.push(tag.to_string());\n            }\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn by_status(&self, status: &Status) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| &t.status == status)\n            .collect()\n    }\n\n    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| &t.priority == priority)\n            .collect()\n    }\n\n    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| t.tags.contains(&tag.to_string()))\n            .collect()\n    }\n\n    pub fn sorted_by_priority(&self) -> Vec<&Task> {\n        let mut result: Vec<&Task> = self.tasks.iter().collect();\n        result.sort_by(|a, b| {\n            let priority_order = |p: &Priority| match p {\n                Priority::Critical => 0,\n                Priority::High => 1,\n                Priority::Medium => 2,\n                Priority::Low => 3,\n            };\n            priority_order(&a.priority).cmp(&priority_order(&b.priority))\n        });\n        result\n    }\n\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should succeed\")\n    }\n\n    pub fn from_json(json: &str) -> Result<Self, String> {\n        serde_json::from_str(json).map_err(|e| e.to_string())\n    }\n\n    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {\n        let before_len = self.tasks.len();\n        self.tasks.retain(|t| {\n            !(t.status == Status::Completed && t.created_at < older_than)\n        });\n        before_len - self.tasks.len()\n    }\n}"}</arguments>
56:║ Status: ✗ FAILED                        ║
64:║   • Fix type errors before proceeding
75:The Cargo.toml already has serde_json. Let me run cargo check to see the actual error:
```

### testgen_ringbuf
```
62:7. **Additional edge cases**: `test_push_pop_alternating`, `test_clear_does_not_panic`, `test_capacity_constructor`, `test_zero_capacity_panics`
81:7. **Additional edge cases**: `test_push_pop_alternating`, `test_clear_does_not_panic`, `test_capacity_constructor`, `test_zero_capacity_panics`
```

### refactor_monolith
```
91:<arguments>{"content": "use crate::store::KvStore;\n\n// ---------------------------------------------------------------------------\n// Serialization methods for KvStore\n// ---------------------------------------------------------------------------\n\nimpl KvStore {\n    // -- Serialization ------------------------------------------------------\n\n    /// Serialize the store to a JSON string.\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should not fail\")\n    }\n\n    /// Deserialize a store from a JSON string.\n    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {\n        serde_json::from_str(json)\n    }\n\n    // -- Merge --------------------------------------------------------------\n\n    /// Merge `other` into `self`. For duplicate keys the entry with the later\n    /// `updated_at` wins. If timestamps are equal the entry from `other` wins.\n    pub fn merge(&mut self, other: &KvStore) {\n        for (key, other_entry) in &other.entries {\n            match self.entries.get(key) {\n                Some(existing) if existing.updated_at > other_entry.updated_at => {\n                    // keep ours\n                }\n                _ => {\n                    self.entries.insert(key.clone(), other_entry.clone());\n                }\n            }\n        }\n    }\n}\n", "path": "./src/serialization.rs"}</arguments>
122:║ Status: ✗ FAILED                        ║
136:║   • Fix type errors before proceeding
```

## Progress Timeline

```
[04:56:03] easy_calculator: score=100/100 rating=BLOOM duration=35s
[04:56:35] hard_scheduler: score=100/100 rating=BLOOM duration=67s
[04:56:38] easy_string_ops: score=100/100 rating=BLOOM duration=70s
[04:56:45] medium_bitset: score=100/100 rating=BLOOM duration=77s
[04:57:41] expert_async_race: score=100/100 rating=BLOOM duration=83s
[04:57:57] security_audit: score=100/100 rating=BLOOM duration=77s
[04:58:11] codegen_task_runner: score=100/100 rating=BLOOM duration=79s
[04:58:43] medium_json_merge: score=100/100 rating=BLOOM duration=191s
[04:59:21] hard_event_bus: score=100/100 rating=BLOOM duration=231s
[05:00:04] refactor_monolith: score=80/100 rating=GROW duration=111s
[05:04:34] testgen_ringbuf: score=80/100 rating=GROW duration=407s
[05:04:45] perf_optimization: score=100/100 rating=BLOOM duration=357s
```

## Artifacts

- Report: `system_tests/projecte2e/reports/20260301-045526/REPORT.md`
- Results: `system_tests/projecte2e/reports/20260301-045526/results/`
- Logs: `system_tests/projecte2e/reports/20260301-045526/logs/<scenario>/`
