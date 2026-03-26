use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Pending,
    Running,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub name: String,
    pub priority: Priority,
    pub status: Status,
    pub tags: Vec<String>,
    pub created_at: u64, // unix timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskManager {
    tasks: Vec<Task>,
    next_id: u64,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let task = Task {
            id,
            name: name.to_string(),
            priority,
            status: Status::Pending,
            tags: Vec::new(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        self.tasks.push(task);
        id
    }

    pub fn get(&self, id: u64) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn remove(&mut self, id: u64) -> Option<Task> {
        let pos = self.tasks.iter().position(|t| t.id == id)?;
        Some(self.tasks.remove(pos))
    }

    pub fn update_status(&mut self, id: u64, status: Status) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.status = status;
            true
        } else {
            false
        }
    }

    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if !task.tags.contains(&tag.to_string()) {
                task.tags.push(tag.to_string());
            }
            true
        } else {
            false
        }
    }

    pub fn by_status(&self, status: &Status) -> Vec<&Task> {
        self.tasks.iter().filter(|t| &t.status == status).collect()
    }

    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {
        self.tasks.iter().filter(|t| &t.priority == priority).collect()
    }

    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.tags.contains(&tag.to_string()))
            .collect()
    }

    pub fn sorted_by_priority(&self) -> Vec<&Task> {
        let mut sorted: Vec<&Task> = self.tasks.iter().collect();
        sorted.sort_by(|a, b| {
            let order = |p: &Priority| match p {
                Priority::Critical => 0,
                Priority::High => 1,
                Priority::Medium => 2,
                Priority::Low => 3,
            };
            order(&a.priority).cmp(&order(&b.priority))
        });
        sorted
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize TaskManager")
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {
        let initial_len = self.tasks.len();
        self.tasks.retain(|t| {
            t.status != Status::Completed || t.created_at >= older_than
        });
        initial_len - self.tasks.len()
    }
}
