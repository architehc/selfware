use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::P2
    }
}

impl Priority {
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "p0" => Some(Self::P0),
            "p1" => Some(Self::P1),
            "p2" => Some(Self::P2),
            "p3" => Some(Self::P3),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Todo,
    InProgress,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recurrence {
    Daily,
    Weekly { interval_weeks: u8 },
    EveryNDays(u16),
}

impl Recurrence {
    pub fn describe(&self) -> String {
        match self {
            Self::Daily => "daily".to_string(),
            Self::Weekly { interval_weeks } => format!("weekly:{}", interval_weeks),
            Self::EveryNDays(days) => format!("every:{}", days),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub estimate_minutes: Option<u32>,
    pub dependencies: Vec<u64>,
    pub recurrence: Option<Recurrence>,
    pub status: Status,
}

impl Task {
    pub fn new(id: u64, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            priority: Priority::default(),
            tags: Vec::new(),
            estimate_minutes: None,
            dependencies: Vec::new(),
            recurrence: None,
            status: Status::Todo,
        }
    }

    pub fn is_done(&self) -> bool {
        self.status == Status::Done
    }
}
