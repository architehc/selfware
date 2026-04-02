use std::collections::BTreeMap;

use crate::model::{Priority, Status, Task};
use crate::parser::{AddSpec, Command};

#[derive(Debug)]
pub enum SchedulerError {
    MissingTask(u64),
    MissingDependency { task_id: u64, dependency_id: u64 },
    InvalidOperation(String),
    Snapshot(String),
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTask(id) => write!(f, "task {} does not exist", id),
            Self::MissingDependency {
                task_id,
                dependency_id,
            } => write!(f, "task {} depends on missing task {}", task_id, dependency_id),
            Self::InvalidOperation(message) => write!(f, "{}", message),
            Self::Snapshot(message) => write!(f, "snapshot error: {}", message),
        }
    }
}

impl std::error::Error for SchedulerError {}

#[derive(Debug, Default)]
pub struct Scheduler {
    tasks: Vec<Task>,
    next_id: u64,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn apply(&mut self, command: Command) -> Result<Option<u64>, SchedulerError> {
        match command {
            Command::Add(spec) => {
                let id = self.add(spec);
                Ok(Some(id))
            }
            Command::Complete(task_id) => {
                self.complete(task_id)?;
                Ok(None)
            }
            Command::Block {
                task_id,
                dependency_ids,
            } => {
                self.block(task_id, dependency_ids)?;
                Ok(None)
            }
            Command::Retag { task_id, tags } => {
                let task = self.task_mut(task_id)?;
                task.tags = tags;
                Ok(None)
            }
        }
    }

    pub fn add(&mut self, spec: AddSpec) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let mut task = Task::new(id, spec.title);
        task.priority = Priority::default();
        task.tags = spec.tags;
        task.estimate_minutes = spec.estimate_minutes;
        task.recurrence = spec.recurrence;

        self.tasks.push(task);
        id
    }

    pub fn complete(&mut self, task_id: u64) -> Result<(), SchedulerError> {
        let task = self.task_mut(task_id)?;
        task.status = Status::Done;
        Ok(())
    }

    pub fn block(&mut self, task_id: u64, dependency_ids: Vec<u64>) -> Result<(), SchedulerError> {
        if dependency_ids.is_empty() {
            return Err(SchedulerError::InvalidOperation(
                "cannot block a task on an empty dependency list".to_string(),
            ));
        }

        for dependency_id in &dependency_ids {
            if !self.tasks.iter().any(|task| task.id == *dependency_id) {
                return Err(SchedulerError::MissingDependency {
                    task_id,
                    dependency_id: *dependency_id,
                });
            }
        }

        let task = self.task_mut(task_id)?;
        task.dependencies = dependency_ids;
        Ok(())
    }

    pub fn ready_tasks(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| !task.is_done() && task.dependencies.is_empty())
            .collect()
    }

    pub fn blocked_tasks(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| !task.is_done() && !task.dependencies.is_empty())
            .collect()
    }

    pub fn summary_by_priority(&self) -> BTreeMap<Priority, usize> {
        let mut summary = BTreeMap::new();
        for task in &self.tasks {
            *summary.entry(task.priority).or_insert(0) += 1;
        }
        summary
    }

    pub fn snapshot_json(&self) -> Result<String, SchedulerError> {
        serde_json::to_string_pretty(&self.tasks)
            .map_err(|error| SchedulerError::Snapshot(error.to_string()))
    }

    pub fn restore_from_snapshot(snapshot: &str) -> Result<Self, SchedulerError> {
        let tasks = serde_json::from_str::<Vec<Task>>(snapshot)
            .map_err(|error| SchedulerError::Snapshot(error.to_string()))?;
        let next_id = tasks.iter().map(|task| task.id).max().unwrap_or(0) + 1;
        Ok(Self { tasks, next_id })
    }

    fn task_mut(&mut self, task_id: u64) -> Result<&mut Task, SchedulerError> {
        self.tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or(SchedulerError::MissingTask(task_id))
    }
}
