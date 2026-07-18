use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

pub struct TaskManager {
    tasks: Vec<Task>,
    next_id: u64,
}

impl TaskManager {
    pub fn new() -> Self {
        todo!()
    }

    pub fn add(&mut self, name: &str, priority: Priority) -> u64 {
        todo!()
    }

    pub fn get(&self, id: u64) -> Option<&Task> {
        todo!()
    }

    pub fn remove(&mut self, id: u64) -> Option<Task> {
        todo!()
    }

    pub fn update_status(&mut self, id: u64, status: Status) -> bool {
        todo!()
    }

    pub fn add_tag(&mut self, id: u64, tag: &str) -> bool {
        todo!()
    }

    pub fn by_status(&self, status: &Status) -> Vec<&Task> {
        todo!()
    }

    pub fn by_priority(&self, priority: &Priority) -> Vec<&Task> {
        todo!()
    }

    pub fn by_tag(&self, tag: &str) -> Vec<&Task> {
        todo!()
    }

    pub fn sorted_by_priority(&self) -> Vec<&Task> {
        todo!()
    }

    pub fn to_json(&self) -> String {
        todo!()
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        todo!()
    }

    pub fn cleanup_completed(&mut self, older_than: u64) -> usize {
        todo!()
    }
}
