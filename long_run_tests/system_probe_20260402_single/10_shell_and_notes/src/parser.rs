use crate::model::{Priority, Recurrence};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddSpec {
    pub title: String,
    pub priority: Priority,
    pub tags: Vec<String>,
    pub estimate_minutes: Option<u32>,
    pub recurrence: Option<Recurrence>,
}

impl AddSpec {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            priority: Priority::default(),
            tags: Vec::new(),
            estimate_minutes: None,
            recurrence: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Add(AddSpec),
    Complete(u64),
    Block {
        task_id: u64,
        dependency_ids: Vec<u64>,
    },
    Retag {
        task_id: u64,
        tags: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

pub fn parse_command(input: &str) -> Result<Command, ParseError> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("add ") {
        return parse_add(rest).map(Command::Add);
    }

    if let Some(rest) = input.strip_prefix("complete ") {
        let id = rest
            .trim()
            .parse()
            .map_err(|_| ParseError::new("expected numeric task id after 'complete'"))?;
        return Ok(Command::Complete(id));
    }

    if let Some(rest) = input.strip_prefix("block ") {
        let (lhs, rhs) = rest
            .split_once(" on ")
            .ok_or_else(|| ParseError::new("expected syntax: block <task-id> on <deps>"))?;
        let task_id = lhs
            .trim()
            .parse()
            .map_err(|_| ParseError::new("expected numeric task id before 'on'"))?;
        let dependency_ids = rhs
            .split(',')
            .filter_map(|value| value.trim().parse::<u64>().ok())
            .collect::<Vec<_>>();

        if dependency_ids.is_empty() {
            return Err(ParseError::new(
                "block requires at least one numeric dependency id",
            ));
        }

        return Ok(Command::Block {
            task_id,
            dependency_ids,
        });
    }

    if let Some(rest) = input.strip_prefix("retag ") {
        let (lhs, rhs) = rest
            .split_once(' ')
            .ok_or_else(|| ParseError::new("expected syntax: retag <task-id> <tags>"))?;
        let task_id = lhs
            .trim()
            .parse()
            .map_err(|_| ParseError::new("expected numeric task id after 'retag'"))?;
        let tags = rhs
            .split(',')
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect::<Vec<_>>();
        return Ok(Command::Retag { task_id, tags });
    }

    Err(ParseError::new("unsupported command"))
}

fn parse_add(rest: &str) -> Result<AddSpec, ParseError> {
    let mut segments = rest.split(';');
    let title = segments
        .next()
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| ParseError::new("add command requires a title"))?;

    let mut spec = AddSpec::new(title);

    for segment in segments {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }

        if let Some(priority) = Priority::parse(segment) {
            spec.priority = priority;
            continue;
        }

        if let Some(tags) = segment.strip_prefix("tags=") {
            spec.tags = tags
                .split(',')
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
                .collect();
            continue;
        }

        if let Some(minutes) = segment.strip_prefix("estimate=") {
            spec.estimate_minutes = minutes.trim().parse::<u32>().ok();
            continue;
        }

        if let Some(raw) = segment.strip_prefix("every=") {
            spec.recurrence = parse_recurrence(raw);
            continue;
        }
    }

    Ok(spec)
}

fn parse_recurrence(input: &str) -> Option<Recurrence> {
    let normalized = input.trim().to_ascii_lowercase();
    if normalized == "daily" {
        return Some(Recurrence::Daily);
    }

    if let Some(value) = normalized.strip_prefix("weekly:") {
        let interval_weeks = value.parse::<u8>().ok()?;
        return Some(Recurrence::Weekly { interval_weeks });
    }

    if let Some(value) = normalized.strip_prefix("days:") {
        let days = value.parse::<u16>().ok()?;
        return Some(Recurrence::EveryNDays(days));
    }

    None
}
