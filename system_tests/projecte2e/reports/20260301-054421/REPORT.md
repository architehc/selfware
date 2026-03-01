# Selfware Agentic Benchmark Suite (SAB) Report

## Summary

| Metric | Value |
|--------|-------|
| Date | 20260301-054421 |
| Model | Qwen/Qwen3-Coder-Next-FP8 |
| Endpoint | https://crazyshit.ngrok.io/v1 |
| Max Context | 1,010,000 tokens |
| Total Scenarios | 12 |
| Completed | 12 |
| Passed (tests green) | 12/12 |
| Average Score | 95/100 |
| Overall Rating | **🌸 BLOOM** |
| Total Duration | 10m 14s |

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
| `codegen_task_runner` | hard | 100/100 | 🌸 BLOOM | 77s | 101 | 0 | 0 | 0 | 0 | 5 |
| `easy_calculator` | easy | 100/100 | 🌸 BLOOM | 56s | 101 | 0 | 0 | 0 | 0 | 4 |
| `easy_string_ops` | easy | 100/100 | 🌸 BLOOM | 85s | 101 | 0 | 0 | 0 | 0 | 3 |
| `expert_async_race` | expert | 100/100 | 🌸 BLOOM | 80s | 101 | 0 | 0 | 0 | 0 | 4 |
| `hard_event_bus` | hard | 100/100 | 🌸 BLOOM | 130s | 101 | 0 | 0 | 0 | 0 | 1 |
| `hard_scheduler` | hard | 100/100 | 🌸 BLOOM | 69s | 101 | 0 | 0 | 0 | 0 | 4 |
| `medium_bitset` | medium | 100/100 | 🌸 BLOOM | 112s | 101 | 0 | 0 | 0 | 0 | 1 |
| `medium_json_merge` | medium | 100/100 | 🌸 BLOOM | 48s | 101 | 0 | 0 | 0 | 0 | 1 |
| `perf_optimization` | hard | 100/100 | 🌸 BLOOM | 354s | 124 | 0 | 0 | 0 | 0 | 1 |
| `security_audit` | hard | 100/100 | 🌸 BLOOM | 89s | 101 | 0 | 0 | 0 | 0 | 11 |
| `refactor_monolith` | medium | 80/100 | 🌿 GROW | 224s | 0 | 0 | 0 | 0 | 0 | 22 |
| `testgen_ringbuf` | medium | 70/100 | 🌿 GROW | 490s | 0 | 0 | 124 | 1 | 0 | 0 |

## Category Breakdown

### Easy (2/2 passed, avg 100/100)

- `easy_calculator`: 🌸 100/100 in 56s
- `easy_string_ops`: 🌸 100/100 in 85s

### Medium (4/4 passed, avg 87/100)

- `medium_bitset`: 🌸 100/100 in 112s
- `medium_json_merge`: 🌸 100/100 in 48s
- `refactor_monolith`: 🌿 80/100 in 224s
- `testgen_ringbuf`: 🌿 70/100 in 490s

### Hard (5/5 passed, avg 100/100)

- `codegen_task_runner`: 🌸 100/100 in 77s
- `hard_event_bus`: 🌸 100/100 in 130s
- `hard_scheduler`: 🌸 100/100 in 69s
- `perf_optimization`: 🌸 100/100 in 354s
- `security_audit`: 🌸 100/100 in 89s

### Expert (1/1 passed, avg 100/100)

- `expert_async_race`: 🌸 100/100 in 80s

## Error Highlights

### easy_calculator
```
47:2. **`divide_handles_zero_safely`** - Division by zero panics instead of returning `None`
59:<arguments>{"new_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    if b == 0 {\n        None\n    } else {\n        Some(a / b)\n    }\n}", "old_str": "pub fn divide(a: i64, b: i64) -> Option<i64> {\n    // BUG: division by zero currently panics instead of returning None\n    Some(a / b)\n}", "path": "./src/lib.rs"}</arguments>
93:2. **`divide`** - Added check for `b == 0` to return `None` instead of panicking
102:2. **`divide`** - Added check for `b == 0` to return `None` instead of panicking
```

### easy_string_ops
```
47:**Bug 2: `truncate`** - off-by-one error, uses `max_len.saturating_sub(1)` instead of `max_len`
94:**2. `truncate` function** - Fixed off-by-one error
110:**2. `truncate` function** - Fixed off-by-one error
```

### medium_json_merge
```
64:The clippy error is unrelated to my changes - it's a command-line parsing issue. Let me verify the code is correct with cargo check:
```

### medium_bitset
```
14:The implementation has several logic bugs that cause tests to fail or panic.
```

### hard_scheduler
```
19:4. next_run_at must not panic on overflow (return None instead).
80:- Changed `next_run_at` to use `checked_add` instead of raw `+` to prevent overflow panics, returning `None` on overflow
101:- Changed `next_run_at` to use `checked_add` instead of raw `+` to prevent overflow panics, returning `None` on overflow
111:- Changed `next_run_at` to use `checked_add` instead of raw `+` to prevent overflow panics, returning `None` on overflow
```

### hard_event_bus
```
121:Good progress! 6 out of 7 tests pass now. The remaining issue is with the event_display_format test. Looking at the error message, the test expects `seq=7` but my format outputs `seq: 7`. Let me check the test again and fix the Display implementation:
```

### expert_async_race
```
74:<arguments>{"new_str": "    /// Mark the current task as failed.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        state.running -= 1;\n        state.failed += 1;\n    }", "old_str": "    /// Mark the current task as failed.\n    ///\n    /// BUG: Increments failed but forgets to decrement running.\n    pub async fn fail(&self) {\n        let mut state = self.state.lock().await;\n        // BUG: missing `state.running -= 1;`\n        state.failed += 1;\n    }", "path": "./src/lib.rs"}</arguments>
79:<arguments>{"new_str": "    /// Return a snapshot of (running, completed, failed).\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let state = self.state.lock().await;\n        (state.running, state.completed, state.failed)\n    }", "old_str": "    /// Return a snapshot of (running, completed, failed).\n    ///\n    /// BUG: Split lock - reads running under one lock acquisition,\n    /// then reads completed and failed under another. The state can\n    /// change between the two reads, giving an inconsistent view.\n    pub async fn snapshot(&self) -> (u32, u32, u32) {\n        let running = {\n            let state = self.state.lock().await;\n            state.running\n        };\n        // ^^^ Lock dropped. State can mutate here.\n        let (completed, failed) = {\n            let state = self.state.lock().await;\n            (state.completed, state.failed)\n        };\n        (running, completed, failed)\n    }", "path": "./src/lib.rs"}</arguments>
108:4. **Split-lock bug in `snapshot()`**: Read all three counters (`running`, `completed`, `failed`) under a single lock acquisition to ensure consistency
126:4. **Split-lock bug in `snapshot()`**: Read all three counters (`running`, `completed`, `failed`) under a single lock acquisition to ensure consistency
```

### security_audit
```
22:5. Implement handle_error_secure that returns generic error messages without internal paths.
48:<arguments>{"new_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    let base_path = Path::new(base_dir);\n    let joined_path = base_path.join(filename);\n    \n    // Canonicalize the resolved path\n    let canonical_path = match joined_path.canonicalize() {\n        Ok(p) => p,\n        Err(e) => return Err(format!(\"Failed to resolve path: {}\", e)),\n    };\n    \n    // Canonicalize the base directory to get its absolute form\n    let canonical_base = match base_path.canonicalize() {\n        Ok(p) => p,\n        Err(e) => return Err(format!(\"Failed to resolve base directory: {}\", e)),\n    };\n    \n    // Check if the canonical path is within the canonical base directory\n    if !canonical_path.starts_with(&canonical_base) {\n        return Err(\"Path traversal detected: path is outside base directory\".to_string());\n    }\n    \n    // Read the file\n    fs::read_to_string(&canonical_path).map_err(|e| format!(\"Failed to read file: {}\", e))\n}", "old_str": "/// SECURE: Must canonicalize the resolved path and verify it stays within base_dir.\npub fn read_user_file_secure(base_dir: &str, filename: &str) -> Result<String, String> {\n    todo!(\"Implement secure version with path canonicalization and prefix validation\")\n}", "path": "./src/lib.rs"}</arguments>
72:Now let me implement `handle_error_secure` that returns generic error messages without internal paths:
76:<arguments>{"new_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(_path: &str) -> String {\n    \"Error: file not found\".to_string()\n}", "old_str": "/// SECURE: Must return a generic error message without revealing internal paths.\npub fn handle_error_secure(path: &str) -> String {\n    todo!(\"Implement secure version that returns generic error messages without internal paths\")\n}", "path": "./src/lib.rs"}</arguments>
93:The clippy error seems to be a tool issue, not a code issue. Let me verify the code compiles correctly:
123:   - Returns error if traversal is detected
139:5. **`handle_error_secure`** - Information leak protection
140:   - Returns generic error message without revealing internal paths
153:   - Returns error if traversal is detected
169:5. **`handle_error_secure`** - Information leak protection
170:   - Returns generic error message without revealing internal paths
```

### perf_optimization
```
75:The formatting check failed, so let me run cargo fmt to fix it:
```

### codegen_task_runner
```
71:<arguments>{"content": "use serde::{Deserialize, Serialize};\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}\n\nimpl TaskManager {\n    pub fn new() -> Self {\n        TaskManager {\n            tasks: Vec::new(),\n            next_id: 1,\n        }\n    }\n\n    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {\n        let task = Task {\n            id: self.next_id,\n            name: name.to_string(),\n            priority,\n            status: Status::Pending,\n            tags: Vec::new(),\n            created_at: self.next_id, // Use next_id as a simple timestamp\n        };\n        self.tasks.push(task);\n        let id = self.next_id;\n        self.next_id += 1;\n        id\n    }\n\n    pub fn get(&self, id: u64) -> Option<&Task> {\n        self.tasks.iter().find(|t| t.id == id)\n    }\n\n    pub fn remove(&mut self, id: u64) -> Option<Task> {\n        if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {\n            Some(self.tasks.remove(pos))\n        } else {\n            None\n        }\n    }\n\n    pub fn update_status(&mut self, id: u64, status: Status) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            task.status = status;\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {\n        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {\n            if !task.tags.contains(&tag.to_string()) {\n                task.tags.push(tag.to_string());\n            }\n            true\n        } else {\n            false\n        }\n    }\n\n    pub fn by_status(&self, status: &Status) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| &t.status == status)\n            .collect()\n    }\n\n    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| &t.priority == priority)\n            .collect()\n    }\n\n    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {\n        self.tasks\n            .iter()\n            .filter(|t| t.tags.contains(&tag.to_string()))\n            .collect()\n    }\n\n    pub fn sorted_by_priority(&self) -> Vec<&Task> {\n        let mut result: Vec<&Task> = self.tasks.iter().collect();\n        result.sort_by(|a, b| {\n            let priority_order = |p: &Priority| match p {\n                Priority::Critical => 0,\n                Priority::High => 1,\n                Priority::Medium => 2,\n                Priority::Low => 3,\n            };\n            priority_order(&a.priority).cmp(&priority_order(&b.priority))\n        });\n        result\n    }\n\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should succeed\")\n    }\n\n    pub fn from_json(json: &str) -> Result<Self, String> {\n        serde_json::from_str(json).map_err(|e| e.to_string())\n    }\n\n    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {\n        let before = self.tasks.len();\n        self.tasks.retain(|t| {\n            !(matches!(t.status, Status::Completed) && t.created_at < older_than)\n        });\n        before - self.tasks.len()\n    }\n}", "path": "./src/lib.rs"}</arguments>
78:║ Status: ✗ FAILED                        ║
86:║   • Fix type errors before proceeding
90:I need to check the type errors. Let me run cargo check to see what's wrong:
101:<arguments>{"new_str": "use serde::{Deserialize, Serialize};\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}", "old_str": "use serde::{Deserialize, Serialize};\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Priority {\n    Low,\n    Medium,\n    High,\n    Critical,\n}\n\n#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\npub enum Status {\n    Pending,\n    Running,\n    Completed,\n    Failed(String),\n}\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct Task {\n    pub id: u64,\n    pub name: String,\n    pub priority: Priority,\n    pub status: Status,\n    pub tags: Vec<String>,\n    pub created_at: u64, // unix timestamp\n}\n\npub struct TaskManager {\n    tasks: Vec<Task>,\n    next_id: u64,\n}", "path": "./src/lib.rs"}</arguments>
```

### testgen_ringbuf
No significant errors captured.

### refactor_monolith
```
102:║ Status: ✗ FAILED                        ║
111:║   • Fix type errors before proceeding
126:║ Status: ✗ FAILED                        ║
139:║   • Fix type errors before proceeding
147:<arguments>{"content": "use crate::entry::Entry;\nuse serde::{Deserialize, Serialize};\nuse std::collections::HashMap;\n\n// ---------------------------------------------------------------------------\n// KvStore\n// ---------------------------------------------------------------------------\n\n#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct KvStore {\n    pub(crate) entries: HashMap<String, Entry>,\n    #[serde(skip)]\n    next_ts: u64,\n}\n\nimpl KvStore {\n    /// Create an empty store. The internal timestamp counter starts at 1.\n    pub fn new() -> Self {\n        Self {\n            entries: HashMap::new(),\n            next_ts: 1,\n        }\n    }\n\n    fn tick(&mut self) -> u64 {\n        let ts = self.next_ts;\n        self.next_ts += 1;\n        ts\n    }\n\n    // -- CRUD ---------------------------------------------------------------\n\n    /// Insert or update an entry. Tags are preserved on update.\n    pub fn set(&mut self, key: &str, value: &str) {\n        let ts = self.tick();\n        self.entries\n            .entry(key.to_string())\n            .and_modify(|e| {\n                e.value = value.to_string();\n                e.updated_at = ts;\n            })\n            .or_insert_with(|| Entry::new(key, value, ts));\n    }\n\n    /// Return the value for `key`, if it exists.\n    pub fn get(&self, key: &str) -> Option<&str> {\n        self.entries.get(key).map(|e| e.value.as_str())\n    }\n\n    /// Remove an entry. Returns `true` if the key existed.\n    pub fn delete(&mut self, key: &str) -> bool {\n        self.entries.remove(key).is_some()\n    }\n\n    /// Return all keys in arbitrary order.\n    pub fn keys(&self) -> Vec<&str> {\n        self.entries.keys().map(|k| k.as_str()).collect()\n    }\n\n    /// Number of entries.\n    pub fn len(&self) -> usize {\n        self.entries.len()\n    }\n\n    /// Whether the store is empty.\n    pub fn is_empty(&self) -> bool {\n        self.entries.is_empty()\n    }\n\n    // -- Queries ------------------------------------------------------------\n\n    /// Return all entries that carry `tag`.\n    pub fn find_by_tag(&self, tag: &str) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.tags.iter().any(|t| t == tag))\n            .collect()\n    }\n\n    /// Return all entries whose key starts with `prefix`.\n    pub fn find_by_prefix(&self, prefix: &str) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.key.starts_with(prefix))\n            .collect()\n    }\n\n    /// Return all entries whose `updated_at` is strictly greater than `ts`.\n    pub fn find_newer_than(&self, ts: u64) -> Vec<&Entry> {\n        self.entries\n            .values()\n            .filter(|e| e.updated_at > ts)\n            .collect()\n    }\n\n    // -- Tags ---------------------------------------------------------------\n\n    /// Add `tag` to the entry at `key`. Returns `false` if the key does not\n    /// exist or the tag is already present.\n    pub fn add_tag(&mut self, key: &str, tag: &str) -> bool {\n        if let Some(entry) = self.entries.get_mut(key) {\n            if entry.tags.iter().any(|t| t == tag) {\n                return false;\n            }\n            entry.tags.push(tag.to_string());\n            true\n        } else {\n            false\n        }\n    }\n\n    /// Remove `tag` from the entry at `key`. Returns `false` if the key does\n    /// not exist or the tag was not present.\n    pub fn remove_tag(&mut self, key: &str, tag: &str) -> bool {\n        if let Some(entry) = self.entries.get_mut(key) {\n            let before = entry.tags.len();\n            entry.tags.retain(|t| t != tag);\n            entry.tags.len() < before\n        } else {\n            false\n        }\n    }\n\n    // -- Serialization ------------------------------------------------------\n\n    /// Serialize the store to a JSON string.\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should not fail\")\n    }\n\n    /// Deserialize a store from a JSON string.\n    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {\n        serde_json::from_str(json)\n    }\n\n    // -- Merge --------------------------------------------------------------\n\n    /// Merge `other` into `self`. For duplicate keys the entry with the later\n    /// `updated_at` wins. If timestamps are equal the entry from `other` wins.\n    pub fn merge(&mut self, other: &KvStore) {\n        for (key, other_entry) in &other.entries {\n            match self.entries.get(key) {\n                Some(existing) if existing.updated_at > other_entry.updated_at => {\n                    // keep ours\n                }\n                _ => {\n                    self.entries.insert(key.clone(), other_entry.clone());\n                }\n            }\n        }\n    }\n}\n\nimpl Default for KvStore {\n    fn default() -> Self {\n        Self::new()\n    }\n}\n", "path": "./src/store.rs"}</arguments>
154:║ Status: ✗ FAILED                        ║
167:║   • Fix type errors before proceeding
189:║ Status: ✗ FAILED                        ║
197:║   • Fix type errors before proceeding
212:║ Status: ✗ FAILED                        ║
224:║   • Fix type errors before proceeding
246:║ Status: ✗ FAILED                        ║
253:║   • Fix type errors before proceeding
296:║ Status: ✗ FAILED                        ║
305:║   • Fix type errors before proceeding
320:<arguments>{"content": "use crate::store::KvStore;\nuse serde::{Deserialize, Serialize};\n\n// ---------------------------------------------------------------------------\n// Serialization methods for KvStore\n// ---------------------------------------------------------------------------\n\nimpl KvStore {\n    // -- Serialization ------------------------------------------------------\n\n    /// Serialize the store to a JSON string.\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should not fail\")\n    }\n\n    /// Deserialize a store from a JSON string.\n    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {\n        serde_json::from_str(json)\n    }\n\n    // -- Merge --------------------------------------------------------------\n\n    /// Merge `other` into `self`. For duplicate keys the entry with the later\n    /// `updated_at` wins. If timestamps are equal the entry from `other` wins.\n    pub fn merge(&mut self, other: &KvStore) {\n        for (key, other_entry) in &other.entries {\n            match self.entries.get(key) {\n                Some(existing) if existing.updated_at > other_entry.updated_at => {\n                    // keep ours\n                }\n                _ => {\n                    self.entries.insert(key.clone(), other_entry.clone());\n                }\n            }\n        }\n    }\n}\n", "path": "./src/serialization.rs"}</arguments>
327:║ Status: ✗ FAILED                        ║
336:║   • Fix type errors before proceeding
379:<arguments>{"content": "use crate::store::KvStore;\n\n// ---------------------------------------------------------------------------\n// Serialization methods for KvStore\n// ---------------------------------------------------------------------------\n\nimpl KvStore {\n    // -- Serialization ------------------------------------------------------\n\n    /// Serialize the store to a JSON string.\n    pub fn to_json(&self) -> String {\n        serde_json::to_string(self).expect(\"serialization should not fail\")\n    }\n\n    /// Deserialize a store from a JSON string.\n    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {\n        serde_json::from_str(json)\n    }\n\n    // -- Merge --------------------------------------------------------------\n\n    /// Merge `other` into `self`. For duplicate keys the entry with the later\n    /// `updated_at` wins. If timestamps are equal the entry from `other` wins.\n    pub fn merge(&mut self, other: &KvStore) {\n        for (key, other_entry) in &other.entries {\n            match self.entries.get(key) {\n                Some(existing) if existing.updated_at > other_entry.updated_at => {\n                    // keep ours\n                }\n                _ => {\n                    self.entries.insert(key.clone(), other_entry.clone());\n                }\n            }\n        }\n    }\n}\n", "path": "./src/serialization.rs"}</arguments>
431:Good! No clippy errors. Let me verify the final structure by checking all the files:
488:- No compilation errors
513:- No compilation errors
```

## Progress Timeline

```
[05:45:15] medium_json_merge: score=100/100 rating=BLOOM duration=48s
[05:45:19] easy_calculator: score=100/100 rating=BLOOM duration=56s
[05:45:33] hard_scheduler: score=100/100 rating=BLOOM duration=69s
[05:45:50] easy_string_ops: score=100/100 rating=BLOOM duration=85s
[05:46:16] medium_bitset: score=100/100 rating=BLOOM duration=112s
[05:46:35] hard_event_bus: score=100/100 rating=BLOOM duration=130s
[05:46:55] security_audit: score=100/100 rating=BLOOM duration=89s
[05:46:57] expert_async_race: score=100/100 rating=BLOOM duration=80s
[05:47:16] codegen_task_runner: score=100/100 rating=BLOOM duration=77s
[05:50:33] refactor_monolith: score=80/100 rating=GROW duration=224s
[05:53:38] perf_optimization: score=100/100 rating=BLOOM duration=354s
[05:54:33] testgen_ringbuf: score=70/100 rating=GROW duration=490s
```

## Artifacts

- Report: `system_tests/projecte2e/reports/20260301-054421/REPORT.md`
- Results: `system_tests/projecte2e/reports/20260301-054421/results/`
- Logs: `system_tests/projecte2e/reports/20260301-054421/logs/<scenario>/`
