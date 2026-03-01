# Selfware Agentic Benchmark Suite (SAB) Report

## Summary

| Metric | Value |
|--------|-------|
| Date | 20260301-082517 |
| Model | Qwen/Qwen3-Coder-Next-FP8 |
| Endpoint | https://crazyshit.ngrok.io/v1 |
| Max Context | 1,010,000 tokens |
| Total Scenarios | 12 |
| Completed | 12 |
| Passed (tests green) | 12/12 |
| Average Score | 96/100 |
| Overall Rating | **🌸 BLOOM** |
| Total Duration | 10m 34s |

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
| `codegen_task_runner` | hard | 100/100 | 🌸 BLOOM | 65s | 101 | 0 | 0 | 0 | 0 | 5 |
| `easy_calculator` | easy | 100/100 | 🌸 BLOOM | 48s | 101 | 0 | 0 | 0 | 0 | 4 |
| `easy_string_ops` | easy | 100/100 | 🌸 BLOOM | 77s | 101 | 0 | 0 | 0 | 0 | 2 |
| `expert_async_race` | expert | 100/100 | 🌸 BLOOM | 67s | 101 | 0 | 0 | 0 | 0 | 4 |
| `hard_event_bus` | hard | 100/100 | 🌸 BLOOM | 623s | 101 | 0 | 0 | 0 | 0 | 5 |
| `hard_scheduler` | hard | 100/100 | 🌸 BLOOM | 49s | 101 | 0 | 0 | 0 | 0 | 6 |
| `medium_bitset` | medium | 100/100 | 🌸 BLOOM | 88s | 101 | 0 | 0 | 0 | 0 | 1 |
| `medium_json_merge` | medium | 100/100 | 🌸 BLOOM | 42s | 101 | 0 | 0 | 0 | 0 | 0 |
| `perf_optimization` | hard | 100/100 | 🌸 BLOOM | 374s | 124 | 0 | 0 | 0 | 0 | 0 |
| `security_audit` | hard | 100/100 | 🌸 BLOOM | 73s | 101 | 0 | 0 | 0 | 0 | 12 |
| `refactor_monolith` | medium | 80/100 | 🌿 GROW | 220s | 0 | 0 | 0 | 0 | 0 | 10 |
| `testgen_ringbuf` | medium | 80/100 | 🌿 GROW | 71s | 0 | 0 | 0 | 0 | 0 | 0 |

## Category Breakdown

### Easy (2/2 passed, avg 100/100)

- `easy_calculator`: 🌸 100/100 in 48s
- `easy_string_ops`: 🌸 100/100 in 77s

### Medium (4/4 passed, avg 90/100)

- `medium_bitset`: 🌸 100/100 in 88s
- `medium_json_merge`: 🌸 100/100 in 42s
- `refactor_monolith`: 🌿 80/100 in 220s
- `testgen_ringbuf`: 🌿 80/100 in 71s

### Hard (5/5 passed, avg 100/100)

- `codegen_task_runner`: 🌸 100/100 in 65s
- `hard_event_bus`: 🌸 100/100 in 623s
- `hard_scheduler`: 🌸 100/100 in 49s
- `perf_optimization`: 🌸 100/100 in 374s
- `security_audit`: 🌸 100/100 in 73s

### Expert (1/1 passed, avg 100/100)

- `expert_async_race`: 🌸 100/100 in 67s

## Error Highlights

### easy_calculator
```
44:2. `divide` panics on division by zero instead of returning `None`
57:<arguments>{"path": "./src/lib.rs", "new_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    if b == 0 {\n        None\n    } else {\n        Some(a / b)\n    }\n}", "old_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    // BUG: division by zero currently panics instead of returning None\n    Some(a / b)\n}"}</arguments>
84:2. **`divide`**: Added a check for `b == 0` to return `None` instead of panicking on division by zero
95:2. **`divide`**: Added a check for `b == 0` to return `None` instead of panicking on division by zero
```

### easy_string_ops
```
106:2. **`truncate`**: Fixed the off-by-one error by changing `max_len.saturating_sub(1)` to `max_len`. Now correctly truncates to the specified maximum length.
119:2. **`truncate`**: Fixed the off-by-one error by changing `max_len.saturating_sub(1)` to `max_len`. Now correctly truncates to the specified maximum length.
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
69:<arguments>{"content": "use crate::duration::parse_duration;\n\n/// Compute next run timestamp (epoch seconds) from a duration string.\npub fn next_run_at(now_epoch: u64, every: &str) -> Option<u64> {\n    let seconds = parse_duration(every)?;\n    // Use checked_add to avoid overflow panic\n    now_epoch.checked_add(seconds)\n}\n\n/// Return true if enough time has elapsed to run again.\npub fn should_run(last_run_epoch: u64, now_epoch: u64, every: &str) -> bool {\n    match next_run_at(last_run_epoch, every) {\n        Some(next) => now_epoch >= next,\n        None => false,\n    }\n}\n", "path": "./src/scheduler.rs"}</arguments>
98:4. Fixed overflow panic in `next_run_at` - replaced `+` with `checked_add()` to return `None` on overflow instead of panicking
103:- `next_run_at_handles_large_values_without_panicking`
117:4. Fixed overflow panic in `next_run_at` - replaced `+` with `checked_add()` to return `None` on overflow instead of panicking
122:- `next_run_at_handles_large_values_without_panicking`
```

### hard_event_bus
```
96:Good progress! 6 tests now pass. The only remaining issue is the display format. Looking at the test error, it expects `Event { topic: "user.login", seq: 7 }` format. Let me fix the Display implementation:
103:Looking at the test error more carefully, the expected format is `Event { topic: "user.login", seq: 7 }` but my current output uses `{:?}` which adds extra quotes around the topic. Let me check what the actual output is and adjust:
155:Looking at the test error message: `Display should contain seq: Event { topic: "user.login", seq: 7 }` - the test expects `seq=7` format (with equals sign, no space). Let me fix this:
210:Let me look at the test error message more carefully. The test expects `seq=7` but I'm producing `seq: 7`. Let me fix this:
271:Looking at the test error message more carefully: `Display should contain seq: Event { topic: "user.login", seq: 7 }`. The test expects `seq=7` (with equals sign, no space). Let me check what my current output is and fix it:
```

### expert_async_race
```
74:<arguments>{"path": "./src/lib.rs", "new_str": "    /// Mark the current task as failed.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        state.running -= 1;\n        state.failed += 1;\n    }", "old_str": "    /// Mark the current task as failed.\n    ///\n    /// BUG: Increments failed but forgets to decrement running.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        // BUG: missing `state.running -= 1;`\n        state.failed += 1;\n    }"}</arguments>
79:<arguments>{"path": "./src/lib.rs", "new_str": "    /// Return a snapshot of (running, completed, failed).\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let state = self.state.lock().await;\n        (state.running, state.completed, state.failed)\n    }", "old_str": "    /// Return a snapshot of (running, completed, failed).\n    ///\n    /// BUG: Split lock - reads running under one lock acquisition,\n    /// then reads completed and failed under another. The state can\n    /// change between the two reads, giving an inconsistent view.\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let running = {\n            let state = self.state.lock().await;\n            state.running\n        };\n        // ^^^ Lock dropped. State can mutate here.\n        let (completed, failed) = {\n            let state = self.state.lock().await;\n            (state.completed, state.failed)\n        };\n        (running, completed, failed)\n    }"}</arguments>
108:4. **Split lock in `snapshot()`**: Combined two separate lock acquisitions (one for `running`, another for `completed` and `failed`) into a single lock acquisition that reads all three values atomically, ensuring a consistent view of the state.
123:4. **Split lock in `snapshot()`**: Combined two separate lock acquisitions (one for `running`, another for `completed` and `failed`) into a single lock acquisition that reads all three values atomically, ensuring a consistent view of the state.
```

### security_audit
```
22:5. Implement handle_error_secure that returns generic error messages without internal paths.
46:<arguments>{"path": "./src/lib.rs", "old_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    todo!(\"Implement secure version with path canonicalization and prefix validation\")\n}", "new_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    // Build the full path\n    let full_path = format!(\"{}/{}\", base_dir, filename);\n    \n    // Canonicalize the resolved path\n    let canonical_path = Path::new(&full_path)\n        .canonicalize()\n        .map_err(|e| format!(\"Failed to resolve path: {}\", e))?;\n    \n    // Canonicalize the base directory\n    let canonical_base = Path::new(base_dir)\n        .canonicalize()\n        .map_err(|e| format!(\"Invalid base directory: {}\", e))?;\n    \n    // Check if the resolved path is within the base directory\n    if !canonical_path.starts_with(&canonical_base) {\n        return Err(\"Access denied: path traversal attempt detected\".to_string());\n    }\n    \n    // Read the file\n    fs::read_to_string(&canonical_path).map_err(|e| format!(\"Failed to read file: {}\", e))\n}"}</arguments>
74:<arguments>{"new_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(path: &str) -> String {\n    // Return a generic error message that doesn't reveal the path\n    \"Error: file not found\".to_string()\n}", "old_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(path: &str) -> String {\n    todo!(\"Implement secure version that returns generic error messages without internal paths\")\n}", "path": "./src/lib.rs"}</arguments>
88:<arguments>{"new_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(_path: &str) -> String {\n    // Return a generic error message that doesn't reveal the path\n    \"Error: file not found\".to_string()\n}", "old_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(path: &str) -> String {\n    // Return a generic error message that doesn't reveal the path\n    \"Error: file not found\".to_string()\n}", "path": "./src/lib.rs"}</arguments>
110:- Returns error if traversal attempt detected
126:**5. `handle_error_secure`** - Information Leak Protection
127:- Returns generic error message without revealing internal paths
128:- Removes path information from error output
138:- Returns error if traversal attempt detected
154:**5. `handle_error_secure`** - Information Leak Protection
155:- Returns generic error message without revealing internal paths
156:- Removes path information from error output
```

### perf_optimization
No significant errors captured.

### codegen_task_runner
```
52:<arguments>{"path": "./src/lib.rs", "content": "use serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}\n\nimpl TaskManager {\n    pub fn new() -> Self {\n        TaskManager {\n            tasks: Vec::new(),\n            next_id: 1,\n        }\n    }\n\n    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {\n        let id = self.next_id;\n        self.next_id += 1;\n        let task = Task {\n            id,\n            name: name.to_string(),\n            priority,\n            status: Status::Pending,\n            tags: Vec::new(),\n            created_at: std::time::SystemTime::now()\n                .duration_since(std::time::UNIX_EPOCH)\n                .unwrap()\n                .as_secs(),\n        };\n        self.tasks.push(task);\n        id\n    }\n\n    pub fn get(&self, id: u64) -> Option<&Task> {\n        self.tasks.iter().find(|t| t.id == id)\n    }\n\n    pub fn remove(&mut self, id: u64) -> Option<Task> {\n        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {\n            Some(self.tasks.remove(pos))\n        } else {\n            None\n        }\n    }\n\n    pub fn update_status(&mut self, id: u64, status: Status) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            task.status = status;\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            task.tags.push(tag.to_string());\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn by_status(&self, status: &Status) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| &t.status == status)\n            .collect()\n    }\n\n    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| &t.priority == priority)\n            .collect()\n    }\n\n    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| t.tags.iter().any(|t_tag| t_tag == tag))\n            .collect()\n    }\n\n    pub fn sorted_by_priority(&self) -> Vec<&Task> {\n        let mut sorted: Vec<&Task> = self.tasks.iter().collect();\n        sorted.sort_by(|a, b| {\n            let priority_order = |p: &Priority| match p {\n                Priority::Critical => 0,\n                Priority::High => 1,\n                Priority::Medium => 2,\n                Priority::Low => 3,\n            };\n            priority_order(&a.priority).cmp(&priority_order(&b.priority))\n        });\n        sorted\n    }\n\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should succeed\")\n    }\n\n    pub fn from_json(json: &str) -> Result<Self, String> {\n        serde_json::from_str(json).map_err(|e| e.to_string())\n    }\n\n    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {\n        let before_len = self.tasks.len();\n        self.tasks.retain(|t| {\n            !(t.status == Status::Completed && t.created_at < older_than)\n        });\n        before_len - self.tasks.len()\n    }\n}\n"}</arguments>
59:║ Status: ✗ FAILED                        ║
67:║   • Fix type errors before proceeding
71:I need to check the compilation errors. Let me run cargo check to see the specific errors:
82:<arguments>{"path": "./src/lib.rs", "old_str": "use serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}", "new_str": "use serde::{Deserialize, Serialize};\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}"}</arguments>
```

### testgen_ringbuf
No significant errors captured.

### refactor_monolith
```
102:║ Status: ✗ FAILED                        ║
110:║   • Fix type errors before proceeding
125:║ Status: ✗ FAILED                        ║
137:║   • Fix type errors before proceeding
152:<arguments>{"content": "use crate::entry::Entry;\nuse serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n// ---------------------------------------------------------------------------\n// KvStore\n// ---------------------------------------------------------------------------\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct KvStore {\n    pub entries: HashMap<String, Entry>,\n    #[serde(skip)]\n    next_ts: u64,\n}\n\nimpl KvStore {\n    /// Create an empty store. The internal timestamp counter starts at 1.\n    pub fn new() -> Self {\n        Self {\n            entries: HashMap::new(),\n            next_ts: 1,\n        }\n    }\n\n    fn tick(&mut self) -> u64 {\n        let ts = self.next_ts;\n        self.next_ts += 1;\n        ts\n    }\n\n    // -- CRUD ---------------------------------------------------------------\n\n    /// Insert or update an entry. Tags are preserved on update.\n    pub fn set(&mut self, key: &str, value: &str) {\n        let ts = self.tick();\n        self.entries\n            .entry(key.to_string())\n            .and_modify(|e| {\n                e.value = value.to_string();\n                e.updated_at = ts;\n            })\n            .or_insert_with(|| Entry::new(key, value, ts));\n    }\n\n    /// Return the value for `key`, if it exists.\n    pub fn get(&self, key: &str) -> Option<&str> {\n        self.entries.get(key).map(|e| e.value.as_str())\n    }\n\n    /// Remove an entry. Returns `true` if the key existed.\n    pub fn delete(&mut self, key: &str) -> bool {\n        self.entries.remove(key).is_some()\n    }\n\n    /// Return all keys in arbitrary order.\n    pub fn keys(&self) -> Vec<&str> {\n        self.entries.keys().map(|k| k.as_str()).collect()\n    }\n\n    /// Number of entries.\n    pub fn len(&self) -> usize {\n        self.entries.len()\n    }\n\n    /// Whether the store is empty.\n    pub fn is_empty(&self) -> bool {\n        self.entries.is_empty()\n    }\n\n    // -- Queries ------------------------------------------------------------\n\n    /// Return all entries that carry `tag`.\n    pub fn find_by_tag(&self, tag: &str) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.tags.iter().any(|t| t == tag))\n            .collect()\n    }\n\n    /// Return all entries whose key starts with `prefix`.\n    pub fn find_by_prefix(&self, prefix: &str) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.key.starts_with(prefix))\n            .collect()\n    }\n\n    /// Return all entries whose `updated_at` is strictly greater than `ts`.\n    pub fn find_newer_than(&self, ts: u64) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.updated_at > ts)\n            .collect()\n    }\n\n    // -- Tags ---------------------------------------------------------------\n\n    /// Add `tag` to the entry at `key`. Returns `false` if the key does not\n    /// exist or the tag is already present.\n    pub fn add_tag(&mut self, key: &str, tag: &str) -> bool {\n        if let Some(entry) = self.entries.get_mut(key) {\n            if entry.tags.iter().any(|t| t == tag) {\n                return false;\n            }\n            entry.tags.push(tag.to_string());\n            true\n        } else {\n            false\n        }\n    }\n\n    /// Remove `tag` from the entry at `key`. Returns `false` if the key does\n    /// not exist or the tag was not present.\n    pub fn remove_tag(&mut self, key: &str, tag: &str) -> bool {\n        if let Some(entry) = self.entries.get_mut(key) {\n            let before = entry.tags.len();\n            entry.tags.retain(|t| t != tag);\n            entry.tags.len() < before\n        } else {\n            false\n        }\n    }\n\n    // -- Serialization ------------------------------------------------------\n\n    /// Serialize the store to a JSON string.\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should not fail\")\n    }\n\n    /// Deserialize a store from a JSON string.\n    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {\n        serde_json::from_str(json)\n    }\n\n    // -- Merge --------------------------------------------------------------\n\n    /// Merge `other` into `self`. For duplicate keys the entry with the later\n    /// `updated_at` wins. If timestamps are equal the entry from `other` wins.\n    pub fn merge(&mut self, other: &KvStore) {\n        for (key, other_entry) in &other.entries {\n            match self.entries.get(key) {\n                Some(existing) if existing.updated_at > other_entry.updated_at => {\n                    // keep ours\n                }\n                _ => {\n                    self.entries.insert(key.clone(), other_entry.clone());\n                }\n            }\n        }\n    }\n}\n\nimpl Default for KvStore {\n    fn default() -> Self {\n        Self::new()\n    }\n}\n", "path": "./src/store.rs"}</arguments>
159:║ Status: ✗ FAILED                        ║
168:║   • Fix type errors before proceeding
190:║ Status: ✗ FAILED                        ║
197:║   • Fix type errors before proceeding
205:<arguments>{"content": "use crate::store::KvStore;\n\n// -- Serialization ------------------------------------------------------\n\n/// Serialize the store to a JSON string.\npub fn to_json(store: &KvStore) -> String {\n    serde_json::to_string(store).expect(\"serialization should not fail\")\n}\n\n/// Deserialize a store from a JSON string.\npub fn from_json(json: &str) -> Result<KvStore, serde_json::Error> {\n    serde_json::from_str(json)\n}\n", "path": "./src/serialization.rs"}</arguments>
```

## Progress Timeline

```
[08:26:05] medium_json_merge: score=100/100 rating=BLOOM duration=42s
[08:26:07] easy_calculator: score=100/100 rating=BLOOM duration=48s
[08:26:09] hard_scheduler: score=100/100 rating=BLOOM duration=49s
[08:26:38] easy_string_ops: score=100/100 rating=BLOOM duration=77s
[08:26:48] medium_bitset: score=100/100 rating=BLOOM duration=88s
[08:27:26] security_audit: score=100/100 rating=BLOOM duration=73s
[08:27:30] expert_async_race: score=100/100 rating=BLOOM duration=67s
[08:27:50] codegen_task_runner: score=100/100 rating=BLOOM duration=65s
[08:28:01] testgen_ringbuf: score=80/100 rating=GROW duration=71s
[08:31:15] refactor_monolith: score=80/100 rating=GROW duration=220s
[08:34:33] perf_optimization: score=100/100 rating=BLOOM duration=374s
[08:35:45] hard_event_bus: score=100/100 rating=BLOOM duration=623s
```

## Artifacts

- Report: `system_tests/projecte2e/reports/20260301-082517/REPORT.md`
- Results: `system_tests/projecte2e/reports/20260301-082517/results/`
- Logs: `system_tests/projecte2e/reports/20260301-082517/logs/<scenario>/`
