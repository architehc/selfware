//! Persistent on-disk store for SWE-bench instance memory.
//!
//! Each instance is stored as a JSON file under:
//!
//! ```text
//! <db_path>/
//!   <repo_safe>/
//!     <commit_short>/
//!       instances/
//!         <instance_id_safe>__<quant_safe>__trial<N>.json
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::swebench::{Lesson, MemoryKey, SwebenchInstanceMemory};

pub struct SwebenchMemoryStore {
    db_path: PathBuf,
}

impl SwebenchMemoryStore {
    /// Open (or create) a store at `db_path`.
    pub fn load(db_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(db_path)
            .with_context(|| format!("creating memory store at {}", db_path.display()))?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
        })
    }

    /// Retrieve memory for a specific key, or `None` if not persisted.
    pub fn get_instance(&self, key: &MemoryKey) -> Option<SwebenchInstanceMemory> {
        let path = self.instance_path(key);
        std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    }

    /// Persist memory for a key, overwriting any existing entry.
    pub fn save_instance(&self, memory: &SwebenchInstanceMemory) -> Result<()> {
        let key = MemoryKey::new(
            &memory.repo,
            &memory.base_commit,
            &memory.instance_id,
            &memory.quant,
            memory.trial,
        );
        let path = self.instance_path(&key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_vec_pretty(memory)?)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Return all repo-level lessons across every instance for `repo`.
    ///
    /// This aggregates lessons from *all* instances of the repo, de-duplicating
    /// by `(file, insight)` so the cap is applied to unique insights only.
    pub fn get_repo_lessons(&self, repo: &str) -> Vec<Lesson> {
        let repo_dir = self.db_path.join(sanitize(repo));
        if !repo_dir.exists() {
            return Vec::new();
        }

        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut out = Vec::new();

        // Walk all commit subdirectories.
        let commit_dirs = match std::fs::read_dir(&repo_dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect::<Vec<_>>(),
            Err(_) => return Vec::new(),
        };

        for commit_dir in commit_dirs {
            let instances_dir = commit_dir.join("instances");
            if !instances_dir.exists() {
                continue;
            }
            let entries = match std::fs::read_dir(&instances_dir) {
                Ok(rd) => rd
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .collect::<Vec<_>>(),
                Err(_) => continue,
            };
            for path in entries {
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let bytes = match std::fs::read(&path) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let mem: SwebenchInstanceMemory = match serde_json::from_slice(&bytes) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                for lesson in &mem.lessons {
                    let key = (lesson.file.clone(), lesson.insight.clone());
                    if seen.insert(key) {
                        out.push(lesson.clone());
                    }
                }
            }
        }

        out
    }

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn instance_path(&self, key: &MemoryKey) -> PathBuf {
        self.db_path
            .join(sanitize(&key.repo))
            .join(&key.base_commit[..key.base_commit.len().min(12)])
            .join("instances")
            .join(format!("{}.json", key.dir_name()))
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::swebench::{Lesson, MemoryKey, SwebenchInstanceMemory};

    #[test]
    fn test_store_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SwebenchMemoryStore::load(tmp.path()).unwrap();

        let key = MemoryKey::new("django/django", "abc123", "inst-1", "qwen-7b", 1);
        let mut mem = SwebenchInstanceMemory::new(&key);
        mem.add_lesson(Lesson::new("auth.py", "Use authenticate()", "verification"));

        store.save_instance(&mem).unwrap();
        let loaded = store.get_instance(&key);
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.instance_id, "inst-1");
        assert_eq!(loaded.lessons.len(), 1);
        assert_eq!(loaded.lessons[0].file, "auth.py");
    }

    #[test]
    fn test_store_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SwebenchMemoryStore::load(tmp.path()).unwrap();
        let key = MemoryKey::new("a", "b", "c", "d", 1);
        assert!(store.get_instance(&key).is_none());
    }

    #[test]
    fn test_get_repo_lessons_dedup() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SwebenchMemoryStore::load(tmp.path()).unwrap();

        let key1 = MemoryKey::new("django/django", "abc123", "inst-1", "qwen-7b", 1);
        let mut mem1 = SwebenchInstanceMemory::new(&key1);
        mem1.add_lesson(Lesson::new("auth.py", "Use authenticate()", "verification"));
        mem1.add_lesson(Lesson::new("models.py", "Check null", "agent"));
        store.save_instance(&mem1).unwrap();

        let key2 = MemoryKey::new("django/django", "abc123", "inst-2", "qwen-7b", 1);
        let mut mem2 = SwebenchInstanceMemory::new(&key2);
        // Same insight as mem1 — should be deduped.
        mem2.add_lesson(Lesson::new("auth.py", "Use authenticate()", "verification"));
        mem2.add_lesson(Lesson::new("views.py", "Validate input", "agent"));
        store.save_instance(&mem2).unwrap();

        let lessons = store.get_repo_lessons("django/django");
        assert_eq!(lessons.len(), 3, "dedup should collapse identical insights");
        let files: Vec<_> = lessons.iter().map(|l| l.file.as_str()).collect();
        assert!(files.contains(&"auth.py"));
        assert!(files.contains(&"models.py"));
        assert!(files.contains(&"views.py"));
    }

    #[test]
    fn test_get_repo_lessons_empty_for_unknown_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SwebenchMemoryStore::load(tmp.path()).unwrap();
        let lessons = store.get_repo_lessons("nonexistent/repo");
        assert!(lessons.is_empty());
    }

    #[test]
    fn test_save_instance_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SwebenchMemoryStore::load(tmp.path()).unwrap();
        let key = MemoryKey::new("r", "c", "i", "q", 1);

        let mut mem = SwebenchInstanceMemory::new(&key);
        mem.add_lesson(Lesson::new("a.py", "first", "agent"));
        store.save_instance(&mem).unwrap();

        mem.lessons.clear();
        mem.add_lesson(Lesson::new("b.py", "second", "verification"));
        store.save_instance(&mem).unwrap();

        let loaded = store.get_instance(&key).unwrap();
        assert_eq!(loaded.lessons.len(), 1);
        assert_eq!(loaded.lessons[0].file, "b.py");
    }
}
