//! Issue Tracker Integration
//!
//! Support for Jira, Linear, GitHub Issues with auto-linking,
//! status updates, time tracking, and sprint planning.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Atomic counters for unique IDs
static ISSUE_COUNTER: AtomicU64 = AtomicU64::new(0);
static SPRINT_COUNTER: AtomicU64 = AtomicU64::new(0);
static WORKLOG_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate unique issue ID
fn generate_issue_id() -> String {
    format!("issue-{}", ISSUE_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique sprint ID
fn generate_sprint_id() -> String {
    format!("sprint-{}", SPRINT_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique worklog ID
fn generate_worklog_id() -> String {
    format!("worklog-{}", WORKLOG_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Issue tracker provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackerType {
    /// GitHub Issues
    GitHub,
    /// Jira
    Jira,
    /// Linear
    Linear,
    /// GitLab Issues
    GitLab,
    /// Azure DevOps
    AzureDevOps,
    /// Trello
    Trello,
    /// Custom/Generic
    Custom,
}

impl std::fmt::Display for TrackerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackerType::GitHub => write!(f, "GitHub"),
            TrackerType::Jira => write!(f, "Jira"),
            TrackerType::Linear => write!(f, "Linear"),
            TrackerType::GitLab => write!(f, "GitLab"),
            TrackerType::AzureDevOps => write!(f, "Azure DevOps"),
            TrackerType::Trello => write!(f, "Trello"),
            TrackerType::Custom => write!(f, "Custom"),
        }
    }
}

impl TrackerType {
    /// Get issue key pattern for auto-linking
    pub fn key_pattern(&self) -> &'static str {
        match self {
            TrackerType::GitHub => r"#(\d+)",
            TrackerType::Jira => r"([A-Z]+-\d+)",
            TrackerType::Linear => r"([A-Z]+-\d+)",
            TrackerType::GitLab => r"#(\d+)",
            TrackerType::AzureDevOps => r"#(\d+)",
            TrackerType::Trello => r"",
            TrackerType::Custom => r"",
        }
    }

    /// Get URL template for issue links
    pub fn url_template(&self) -> &'static str {
        match self {
            TrackerType::GitHub => "https://github.com/{org}/{repo}/issues/{id}",
            TrackerType::Jira => "https://{domain}/browse/{id}",
            TrackerType::Linear => "https://linear.app/{org}/issue/{id}",
            TrackerType::GitLab => "https://gitlab.com/{org}/{repo}/-/issues/{id}",
            TrackerType::AzureDevOps => {
                "https://dev.azure.com/{org}/{project}/_workitems/edit/{id}"
            }
            TrackerType::Trello => "https://trello.com/c/{id}",
            TrackerType::Custom => "{base_url}/{id}",
        }
    }
}

/// Issue status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueStatus {
    /// Open/New
    Open,
    /// In Progress
    InProgress,
    /// In Review
    InReview,
    /// Testing
    Testing,
    /// Done/Closed
    Done,
    /// Blocked
    Blocked,
    /// Won't Fix
    WontFix,
    /// Duplicate
    Duplicate,
}

impl std::fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueStatus::Open => write!(f, "Open"),
            IssueStatus::InProgress => write!(f, "In Progress"),
            IssueStatus::InReview => write!(f, "In Review"),
            IssueStatus::Testing => write!(f, "Testing"),
            IssueStatus::Done => write!(f, "Done"),
            IssueStatus::Blocked => write!(f, "Blocked"),
            IssueStatus::WontFix => write!(f, "Won't Fix"),
            IssueStatus::Duplicate => write!(f, "Duplicate"),
        }
    }
}

impl IssueStatus {
    /// Check if status is terminal (closed)
    pub fn is_closed(&self) -> bool {
        matches!(
            self,
            IssueStatus::Done | IssueStatus::WontFix | IssueStatus::Duplicate
        )
    }

    /// Check if status is active (in progress)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            IssueStatus::InProgress | IssueStatus::InReview | IssueStatus::Testing
        )
    }

    /// Get valid transitions from this status
    pub fn valid_transitions(&self) -> Vec<IssueStatus> {
        match self {
            IssueStatus::Open => vec![
                IssueStatus::InProgress,
                IssueStatus::WontFix,
                IssueStatus::Duplicate,
            ],
            IssueStatus::InProgress => vec![
                IssueStatus::InReview,
                IssueStatus::Testing,
                IssueStatus::Blocked,
                IssueStatus::Done,
            ],
            IssueStatus::InReview => vec![
                IssueStatus::InProgress,
                IssueStatus::Testing,
                IssueStatus::Done,
            ],
            IssueStatus::Testing => vec![IssueStatus::InProgress, IssueStatus::Done],
            IssueStatus::Done => vec![IssueStatus::Open], // Reopen
            IssueStatus::Blocked => vec![IssueStatus::InProgress, IssueStatus::Open],
            IssueStatus::WontFix => vec![IssueStatus::Open],
            IssueStatus::Duplicate => vec![],
        }
    }
}

/// Issue priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IssuePriority {
    /// Lowest priority
    Lowest,
    /// Low priority
    Low,
    /// Medium priority
    Medium,
    /// High priority
    High,
    /// Highest/Critical priority
    Highest,
}

impl std::fmt::Display for IssuePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssuePriority::Lowest => write!(f, "Lowest"),
            IssuePriority::Low => write!(f, "Low"),
            IssuePriority::Medium => write!(f, "Medium"),
            IssuePriority::High => write!(f, "High"),
            IssuePriority::Highest => write!(f, "Highest"),
        }
    }
}

/// Issue type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueType {
    /// Bug report
    Bug,
    /// Feature request
    Feature,
    /// Task
    Task,
    /// Story
    Story,
    /// Epic
    Epic,
    /// Subtask
    Subtask,
    /// Improvement
    Improvement,
    /// Documentation
    Documentation,
    /// Chore
    Chore,
}

impl std::fmt::Display for IssueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueType::Bug => write!(f, "Bug"),
            IssueType::Feature => write!(f, "Feature"),
            IssueType::Task => write!(f, "Task"),
            IssueType::Story => write!(f, "Story"),
            IssueType::Epic => write!(f, "Epic"),
            IssueType::Subtask => write!(f, "Subtask"),
            IssueType::Improvement => write!(f, "Improvement"),
            IssueType::Documentation => write!(f, "Documentation"),
            IssueType::Chore => write!(f, "Chore"),
        }
    }
}

/// An issue
#[derive(Debug, Clone)]
pub struct Issue {
    /// Internal ID
    pub id: String,
    /// External key (e.g., "PROJ-123", "#42")
    pub key: String,
    /// Title/Summary
    pub title: String,
    /// Description
    pub description: String,
    /// Status
    pub status: IssueStatus,
    /// Priority
    pub priority: IssuePriority,
    /// Issue type
    pub issue_type: IssueType,
    /// Assignee
    pub assignee: Option<String>,
    /// Reporter
    pub reporter: Option<String>,
    /// Labels
    pub labels: Vec<String>,
    /// Sprint ID
    pub sprint_id: Option<String>,
    /// Epic/Parent ID
    pub parent_id: Option<String>,
    /// Linked issues
    pub links: Vec<IssueLink>,
    /// Story points
    pub story_points: Option<u32>,
    /// Time estimate (seconds)
    pub estimate_seconds: Option<u64>,
    /// Time spent (seconds)
    pub time_spent_seconds: u64,
    /// Created timestamp
    pub created_at: u64,
    /// Updated timestamp
    pub updated_at: u64,
    /// Due date (timestamp)
    pub due_date: Option<u64>,
    /// External URL
    pub url: Option<String>,
    /// Custom fields
    pub custom_fields: HashMap<String, String>,
}

impl Issue {
    /// Create a new issue
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_issue_id(),
            key: key.into(),
            title: title.into(),
            description: String::new(),
            status: IssueStatus::Open,
            priority: IssuePriority::Medium,
            issue_type: IssueType::Task,
            assignee: None,
            reporter: None,
            labels: Vec::new(),
            sprint_id: None,
            parent_id: None,
            links: Vec::new(),
            story_points: None,
            estimate_seconds: None,
            time_spent_seconds: 0,
            created_at: now,
            updated_at: now,
            due_date: None,
            url: None,
            custom_fields: HashMap::new(),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set status
    pub fn with_status(mut self, status: IssueStatus) -> Self {
        self.status = status;
        self.updated_at = current_timestamp();
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: IssuePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set issue type
    pub fn with_type(mut self, issue_type: IssueType) -> Self {
        self.issue_type = issue_type;
        self
    }

    /// Set assignee
    pub fn with_assignee(mut self, assignee: impl Into<String>) -> Self {
        self.assignee = Some(assignee.into());
        self
    }

    /// Set reporter
    pub fn with_reporter(mut self, reporter: impl Into<String>) -> Self {
        self.reporter = Some(reporter.into());
        self
    }

    /// Add label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }

    /// Set sprint
    pub fn with_sprint(mut self, sprint_id: impl Into<String>) -> Self {
        self.sprint_id = Some(sprint_id.into());
        self
    }

    /// Set parent
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Set story points
    pub fn with_story_points(mut self, points: u32) -> Self {
        self.story_points = Some(points);
        self
    }

    /// Set estimate
    pub fn with_estimate(mut self, duration: Duration) -> Self {
        self.estimate_seconds = Some(duration.as_secs());
        self
    }

    /// Set due date
    pub fn with_due_date(mut self, timestamp: u64) -> Self {
        self.due_date = Some(timestamp);
        self
    }

    /// Set URL
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add link
    pub fn add_link(&mut self, link: IssueLink) {
        self.links.push(link);
        self.updated_at = current_timestamp();
    }

    /// Log time
    pub fn log_time(&mut self, seconds: u64) {
        self.time_spent_seconds += seconds;
        self.updated_at = current_timestamp();
    }

    /// Transition to new status
    pub fn transition(&mut self, new_status: IssueStatus) -> Result<(), String> {
        if self.status.valid_transitions().contains(&new_status) {
            self.status = new_status;
            self.updated_at = current_timestamp();
            Ok(())
        } else {
            Err(format!(
                "Cannot transition from {} to {}",
                self.status, new_status
            ))
        }
    }

    /// Get remaining estimate
    pub fn remaining_estimate(&self) -> Option<u64> {
        self.estimate_seconds
            .map(|est| est.saturating_sub(self.time_spent_seconds))
    }

    /// Check if overdue
    pub fn is_overdue(&self) -> bool {
        match self.due_date {
            Some(due) => current_timestamp() > due && !self.status.is_closed(),
            None => false,
        }
    }
}

/// Link type between issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkType {
    /// Blocks another issue
    Blocks,
    /// Is blocked by another issue
    BlockedBy,
    /// Duplicates another issue
    Duplicates,
    /// Is duplicated by another issue
    DuplicatedBy,
    /// Relates to another issue
    RelatesTo,
    /// Is parent of
    ParentOf,
    /// Is child of
    ChildOf,
    /// Causes
    Causes,
    /// Is caused by
    CausedBy,
}

impl std::fmt::Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkType::Blocks => write!(f, "blocks"),
            LinkType::BlockedBy => write!(f, "is blocked by"),
            LinkType::Duplicates => write!(f, "duplicates"),
            LinkType::DuplicatedBy => write!(f, "is duplicated by"),
            LinkType::RelatesTo => write!(f, "relates to"),
            LinkType::ParentOf => write!(f, "is parent of"),
            LinkType::ChildOf => write!(f, "is child of"),
            LinkType::Causes => write!(f, "causes"),
            LinkType::CausedBy => write!(f, "is caused by"),
        }
    }
}

impl LinkType {
    /// Get inverse link type
    pub fn inverse(&self) -> Self {
        match self {
            LinkType::Blocks => LinkType::BlockedBy,
            LinkType::BlockedBy => LinkType::Blocks,
            LinkType::Duplicates => LinkType::DuplicatedBy,
            LinkType::DuplicatedBy => LinkType::Duplicates,
            LinkType::RelatesTo => LinkType::RelatesTo,
            LinkType::ParentOf => LinkType::ChildOf,
            LinkType::ChildOf => LinkType::ParentOf,
            LinkType::Causes => LinkType::CausedBy,
            LinkType::CausedBy => LinkType::Causes,
        }
    }
}

/// Issue link
#[derive(Debug, Clone)]
pub struct IssueLink {
    /// Target issue key
    pub target_key: String,
    /// Link type
    pub link_type: LinkType,
}

impl IssueLink {
    /// Create a new issue link
    pub fn new(target_key: impl Into<String>, link_type: LinkType) -> Self {
        Self {
            target_key: target_key.into(),
            link_type,
        }
    }
}

/// Work log entry
#[derive(Debug, Clone)]
pub struct WorkLog {
    /// Log ID
    pub id: String,
    /// Issue key
    pub issue_key: String,
    /// Author
    pub author: String,
    /// Time spent (seconds)
    pub time_spent_seconds: u64,
    /// Work description
    pub description: String,
    /// Started timestamp
    pub started_at: u64,
    /// Created timestamp
    pub created_at: u64,
}

impl WorkLog {
    /// Create a new work log
    pub fn new(issue_key: impl Into<String>, author: impl Into<String>, seconds: u64) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_worklog_id(),
            issue_key: issue_key.into(),
            author: author.into(),
            time_spent_seconds: seconds,
            description: String::new(),
            started_at: now,
            created_at: now,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set started time
    pub fn with_started_at(mut self, timestamp: u64) -> Self {
        self.started_at = timestamp;
        self
    }

    /// Format duration as human readable
    pub fn duration_string(&self) -> String {
        let hours = self.time_spent_seconds / 3600;
        let minutes = (self.time_spent_seconds % 3600) / 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    }
}

/// Sprint
#[derive(Debug, Clone)]
pub struct Sprint {
    /// Sprint ID
    pub id: String,
    /// Sprint name
    pub name: String,
    /// Sprint goal
    pub goal: Option<String>,
    /// Start date
    pub start_date: Option<u64>,
    /// End date
    pub end_date: Option<u64>,
    /// Status
    pub status: SprintStatus,
    /// Issues in sprint
    pub issue_keys: Vec<String>,
    /// Velocity (story points completed)
    pub velocity: u32,
    /// Committed points
    pub committed_points: u32,
}

impl Sprint {
    /// Create a new sprint
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: generate_sprint_id(),
            name: name.into(),
            goal: None,
            start_date: None,
            end_date: None,
            status: SprintStatus::Future,
            issue_keys: Vec::new(),
            velocity: 0,
            committed_points: 0,
        }
    }

    /// Set goal
    pub fn with_goal(mut self, goal: impl Into<String>) -> Self {
        self.goal = Some(goal.into());
        self
    }

    /// Set dates
    pub fn with_dates(mut self, start: u64, end: u64) -> Self {
        self.start_date = Some(start);
        self.end_date = Some(end);
        self
    }

    /// Add issue
    pub fn add_issue(&mut self, key: impl Into<String>) {
        self.issue_keys.push(key.into());
    }

    /// Remove issue
    pub fn remove_issue(&mut self, key: &str) {
        self.issue_keys.retain(|k| k != key);
    }

    /// Start sprint
    pub fn start(&mut self) -> Result<(), String> {
        if self.status == SprintStatus::Future {
            self.status = SprintStatus::Active;
            if self.start_date.is_none() {
                self.start_date = Some(current_timestamp());
            }
            Ok(())
        } else {
            Err(format!("Cannot start sprint in {} status", self.status))
        }
    }

    /// Complete sprint
    pub fn complete(&mut self, velocity: u32) -> Result<(), String> {
        if self.status == SprintStatus::Active {
            self.status = SprintStatus::Completed;
            self.velocity = velocity;
            if self.end_date.is_none() {
                self.end_date = Some(current_timestamp());
            }
            Ok(())
        } else {
            Err(format!("Cannot complete sprint in {} status", self.status))
        }
    }

    /// Get duration in days
    pub fn duration_days(&self) -> Option<u64> {
        match (self.start_date, self.end_date) {
            (Some(start), Some(end)) => Some((end - start) / (24 * 3600)),
            _ => None,
        }
    }

    /// Get remaining days
    pub fn remaining_days(&self) -> Option<u64> {
        match (self.status, self.end_date) {
            (SprintStatus::Active, Some(end)) => {
                let now = current_timestamp();
                if end > now {
                    Some((end - now) / (24 * 3600))
                } else {
                    Some(0)
                }
            }
            _ => None,
        }
    }
}

/// Sprint status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SprintStatus {
    /// Future sprint
    Future,
    /// Active sprint
    Active,
    /// Completed sprint
    Completed,
}

impl std::fmt::Display for SprintStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SprintStatus::Future => write!(f, "Future"),
            SprintStatus::Active => write!(f, "Active"),
            SprintStatus::Completed => write!(f, "Completed"),
        }
    }
}

/// Issue tracker connection
#[derive(Debug, Clone)]
pub struct TrackerConnection {
    /// Tracker type
    pub tracker_type: TrackerType,
    /// Base URL
    pub base_url: String,
    /// Organization/Workspace
    pub org: Option<String>,
    /// Project/Repository
    pub project: Option<String>,
    /// API token (redacted in debug)
    api_token: Option<String>,
    /// Connected status
    pub connected: bool,
}

impl TrackerConnection {
    /// Create a new connection
    pub fn new(tracker_type: TrackerType, base_url: impl Into<String>) -> Self {
        Self {
            tracker_type,
            base_url: base_url.into(),
            org: None,
            project: None,
            api_token: None,
            connected: false,
        }
    }

    /// Set organization
    pub fn with_org(mut self, org: impl Into<String>) -> Self {
        self.org = Some(org.into());
        self
    }

    /// Set project
    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    /// Set API token
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.api_token = Some(token.into());
        self
    }

    /// Mark as connected
    pub fn mark_connected(&mut self) {
        self.connected = true;
    }

    /// Mark as disconnected
    pub fn mark_disconnected(&mut self) {
        self.connected = false;
    }

    /// Check if has token
    pub fn has_token(&self) -> bool {
        self.api_token.is_some()
    }

    /// Build issue URL
    pub fn issue_url(&self, key: &str) -> String {
        let template = self.tracker_type.url_template();
        let mut url = template.to_string();

        url = url.replace("{base_url}", &self.base_url);
        url = url.replace("{domain}", &self.base_url.replace("https://", ""));
        url = url.replace("{org}", self.org.as_deref().unwrap_or(""));
        url = url.replace("{repo}", self.project.as_deref().unwrap_or(""));
        url = url.replace("{project}", self.project.as_deref().unwrap_or(""));
        url = url.replace("{id}", key);

        url
    }
}

/// Auto-linker for detecting issue references in text
#[derive(Debug)]
pub struct AutoLinker {
    /// Patterns for different trackers
    patterns: Vec<(TrackerType, regex::Regex)>,
}

impl Default for AutoLinker {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoLinker {
    /// Create a new auto-linker
    pub fn new() -> Self {
        let mut patterns = Vec::new();

        // GitHub/GitLab: #123
        if let Ok(re) = regex::Regex::new(r"#(\d+)") {
            patterns.push((TrackerType::GitHub, re));
        }

        // Jira/Linear: PROJ-123
        if let Ok(re) = regex::Regex::new(r"\b([A-Z][A-Z0-9]+-\d+)\b") {
            patterns.push((TrackerType::Jira, re));
        }

        Self { patterns }
    }

    /// Find issue references in text
    pub fn find_references(&self, text: &str) -> Vec<IssueReference> {
        let mut refs = Vec::new();

        for (tracker_type, pattern) in &self.patterns {
            for cap in pattern.captures_iter(text) {
                if let Some(m) = cap.get(1) {
                    refs.push(IssueReference {
                        key: m.as_str().to_string(),
                        tracker_type: *tracker_type,
                        start: m.start(),
                        end: m.end(),
                    });
                } else if let Some(m) = cap.get(0) {
                    refs.push(IssueReference {
                        key: m.as_str().to_string(),
                        tracker_type: *tracker_type,
                        start: m.start(),
                        end: m.end(),
                    });
                }
            }
        }

        refs
    }

    /// Replace references with links
    pub fn linkify(&self, text: &str, url_template: &str) -> String {
        let mut result = text.to_string();

        // Process in reverse order to maintain positions
        let mut refs = self.find_references(text);
        refs.sort_by(|a, b| b.start.cmp(&a.start));

        for r in refs {
            let url = url_template.replace("{key}", &r.key);
            let link = format!("[{}]({})", r.key, url);
            result.replace_range(r.start..r.end, &link);
        }

        result
    }
}

/// Issue reference found in text
#[derive(Debug, Clone)]
pub struct IssueReference {
    /// Issue key
    pub key: String,
    /// Tracker type
    pub tracker_type: TrackerType,
    /// Start position in text
    pub start: usize,
    /// End position in text
    pub end: usize,
}

/// Issue tracker manager
#[derive(Debug)]
pub struct IssueTracker {
    /// Connection
    connection: Option<TrackerConnection>,
    /// Issues cache
    issues: HashMap<String, Issue>,
    /// Sprints
    sprints: HashMap<String, Sprint>,
    /// Work logs
    worklogs: Vec<WorkLog>,
    /// Auto-linker
    auto_linker: AutoLinker,
}

impl Default for IssueTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl IssueTracker {
    /// Create a new issue tracker
    pub fn new() -> Self {
        Self {
            connection: None,
            issues: HashMap::new(),
            sprints: HashMap::new(),
            worklogs: Vec::new(),
            auto_linker: AutoLinker::new(),
        }
    }

    /// Connect to tracker
    pub fn connect(&mut self, connection: TrackerConnection) {
        self.connection = Some(connection);
    }

    /// Get connection
    pub fn connection(&self) -> Option<&TrackerConnection> {
        self.connection.as_ref()
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connection.as_ref().is_some_and(|c| c.connected)
    }

    /// Add issue
    pub fn add_issue(&mut self, issue: Issue) -> String {
        let key = issue.key.clone();
        self.issues.insert(key.clone(), issue);
        key
    }

    /// Get issue
    pub fn get_issue(&self, key: &str) -> Option<&Issue> {
        self.issues.get(key)
    }

    /// Get mutable issue
    pub fn get_issue_mut(&mut self, key: &str) -> Option<&mut Issue> {
        self.issues.get_mut(key)
    }

    /// Update issue status
    pub fn update_status(&mut self, key: &str, status: IssueStatus) -> Result<(), String> {
        match self.issues.get_mut(key) {
            Some(issue) => issue.transition(status),
            None => Err(format!("Issue {} not found", key)),
        }
    }

    /// Get issues by status
    pub fn issues_by_status(&self, status: IssueStatus) -> Vec<&Issue> {
        self.issues
            .values()
            .filter(|i| i.status == status)
            .collect()
    }

    /// Get issues by assignee
    pub fn issues_by_assignee(&self, assignee: &str) -> Vec<&Issue> {
        self.issues
            .values()
            .filter(|i| i.assignee.as_deref() == Some(assignee))
            .collect()
    }

    /// Get issues by sprint
    pub fn issues_by_sprint(&self, sprint_id: &str) -> Vec<&Issue> {
        self.issues
            .values()
            .filter(|i| i.sprint_id.as_deref() == Some(sprint_id))
            .collect()
    }

    /// Get open issues
    pub fn open_issues(&self) -> Vec<&Issue> {
        self.issues
            .values()
            .filter(|i| !i.status.is_closed())
            .collect()
    }

    /// Get overdue issues
    pub fn overdue_issues(&self) -> Vec<&Issue> {
        self.issues.values().filter(|i| i.is_overdue()).collect()
    }

    /// Add sprint
    pub fn add_sprint(&mut self, sprint: Sprint) -> String {
        let id = sprint.id.clone();
        self.sprints.insert(id.clone(), sprint);
        id
    }

    /// Get sprint
    pub fn get_sprint(&self, id: &str) -> Option<&Sprint> {
        self.sprints.get(id)
    }

    /// Get mutable sprint
    pub fn get_sprint_mut(&mut self, id: &str) -> Option<&mut Sprint> {
        self.sprints.get_mut(id)
    }

    /// Get active sprint
    pub fn active_sprint(&self) -> Option<&Sprint> {
        self.sprints
            .values()
            .find(|s| s.status == SprintStatus::Active)
    }

    /// Log work
    pub fn log_work(&mut self, worklog: WorkLog) {
        // Update issue time spent
        if let Some(issue) = self.issues.get_mut(&worklog.issue_key) {
            issue.log_time(worklog.time_spent_seconds);
        }
        self.worklogs.push(worklog);
    }

    /// Get work logs for issue
    pub fn worklogs_for_issue(&self, key: &str) -> Vec<&WorkLog> {
        self.worklogs
            .iter()
            .filter(|w| w.issue_key == key)
            .collect()
    }

    /// Get total time logged for issue
    pub fn total_time_logged(&self, key: &str) -> u64 {
        self.worklogs
            .iter()
            .filter(|w| w.issue_key == key)
            .map(|w| w.time_spent_seconds)
            .sum()
    }

    /// Find issue references in text
    pub fn find_references(&self, text: &str) -> Vec<IssueReference> {
        self.auto_linker.find_references(text)
    }

    /// Linkify text with issue references
    pub fn linkify(&self, text: &str) -> String {
        let template = self
            .connection
            .as_ref()
            .map(|c| c.issue_url("{key}"))
            .unwrap_or_else(|| "{key}".to_string());

        self.auto_linker.linkify(text, &template)
    }

    /// Get sprint report
    pub fn sprint_report(&self, sprint_id: &str) -> Option<SprintReport> {
        let sprint = self.sprints.get(sprint_id)?;
        let issues: Vec<_> = self.issues_by_sprint(sprint_id);

        let completed = issues.iter().filter(|i| i.status.is_closed()).count();
        let in_progress = issues.iter().filter(|i| i.status.is_active()).count();
        let todo = issues
            .iter()
            .filter(|i| i.status == IssueStatus::Open)
            .count();

        let completed_points: u32 = issues
            .iter()
            .filter(|i| i.status.is_closed())
            .filter_map(|i| i.story_points)
            .sum();

        let total_points: u32 = issues.iter().filter_map(|i| i.story_points).sum();

        Some(SprintReport {
            sprint_name: sprint.name.clone(),
            total_issues: issues.len(),
            completed,
            in_progress,
            todo,
            completed_points,
            total_points,
            remaining_days: sprint.remaining_days(),
        })
    }

    /// Get issue count
    pub fn issue_count(&self) -> usize {
        self.issues.len()
    }

    /// Get sprint count
    pub fn sprint_count(&self) -> usize {
        self.sprints.len()
    }
}

/// Sprint report
#[derive(Debug, Clone)]
pub struct SprintReport {
    /// Sprint name
    pub sprint_name: String,
    /// Total issues
    pub total_issues: usize,
    /// Completed issues
    pub completed: usize,
    /// In progress issues
    pub in_progress: usize,
    /// Todo issues
    pub todo: usize,
    /// Completed story points
    pub completed_points: u32,
    /// Total story points
    pub total_points: u32,
    /// Remaining days
    pub remaining_days: Option<u64>,
}

impl SprintReport {
    /// Get completion percentage
    pub fn completion_percentage(&self) -> f64 {
        if self.total_issues == 0 {
            100.0
        } else {
            (self.completed as f64 / self.total_issues as f64) * 100.0
        }
    }

    /// Get points completion percentage
    pub fn points_completion_percentage(&self) -> f64 {
        if self.total_points == 0 {
            100.0
        } else {
            (self.completed_points as f64 / self.total_points as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_type_display() {
        assert_eq!(format!("{}", TrackerType::GitHub), "GitHub");
        assert_eq!(format!("{}", TrackerType::Jira), "Jira");
        assert_eq!(format!("{}", TrackerType::Linear), "Linear");
    }

    #[test]
    fn test_tracker_type_pattern() {
        assert!(!TrackerType::GitHub.key_pattern().is_empty());
        assert!(!TrackerType::Jira.key_pattern().is_empty());
    }

    #[test]
    fn test_issue_status_display() {
        assert_eq!(format!("{}", IssueStatus::Open), "Open");
        assert_eq!(format!("{}", IssueStatus::InProgress), "In Progress");
        assert_eq!(format!("{}", IssueStatus::Done), "Done");
    }

    #[test]
    fn test_issue_status_is_closed() {
        assert!(IssueStatus::Done.is_closed());
        assert!(IssueStatus::WontFix.is_closed());
        assert!(!IssueStatus::Open.is_closed());
        assert!(!IssueStatus::InProgress.is_closed());
    }

    #[test]
    fn test_issue_status_is_active() {
        assert!(IssueStatus::InProgress.is_active());
        assert!(IssueStatus::InReview.is_active());
        assert!(!IssueStatus::Open.is_active());
        assert!(!IssueStatus::Done.is_active());
    }

    #[test]
    fn test_issue_status_valid_transitions() {
        let open_transitions = IssueStatus::Open.valid_transitions();
        assert!(open_transitions.contains(&IssueStatus::InProgress));
        assert!(!open_transitions.contains(&IssueStatus::Done));
    }

    #[test]
    fn test_issue_priority_ordering() {
        assert!(IssuePriority::Low < IssuePriority::Medium);
        assert!(IssuePriority::Medium < IssuePriority::High);
        assert!(IssuePriority::High < IssuePriority::Highest);
    }

    #[test]
    fn test_issue_type_display() {
        assert_eq!(format!("{}", IssueType::Bug), "Bug");
        assert_eq!(format!("{}", IssueType::Feature), "Feature");
    }

    #[test]
    fn test_issue_creation() {
        let issue = Issue::new("PROJ-123", "Fix login bug")
            .with_description("Users can't login")
            .with_priority(IssuePriority::High)
            .with_type(IssueType::Bug);

        assert_eq!(issue.key, "PROJ-123");
        assert_eq!(issue.title, "Fix login bug");
        assert_eq!(issue.priority, IssuePriority::High);
        assert_eq!(issue.issue_type, IssueType::Bug);
    }

    #[test]
    fn test_issue_with_assignee() {
        let issue = Issue::new("PROJ-1", "Task")
            .with_assignee("alice")
            .with_reporter("bob");

        assert_eq!(issue.assignee, Some("alice".to_string()));
        assert_eq!(issue.reporter, Some("bob".to_string()));
    }

    #[test]
    fn test_issue_with_labels() {
        let issue = Issue::new("PROJ-1", "Task")
            .with_label("urgent")
            .with_label("frontend");

        assert!(issue.labels.contains(&"urgent".to_string()));
        assert!(issue.labels.contains(&"frontend".to_string()));
    }

    #[test]
    fn test_issue_transition() {
        let mut issue = Issue::new("PROJ-1", "Task");

        assert!(issue.transition(IssueStatus::InProgress).is_ok());
        assert_eq!(issue.status, IssueStatus::InProgress);

        assert!(issue.transition(IssueStatus::Done).is_ok());
        assert_eq!(issue.status, IssueStatus::Done);
    }

    #[test]
    fn test_issue_invalid_transition() {
        let mut issue = Issue::new("PROJ-1", "Task");

        // Can't go directly from Open to Done
        let result = issue.transition(IssueStatus::Done);
        assert!(result.is_err());
    }

    #[test]
    fn test_issue_log_time() {
        let mut issue = Issue::new("PROJ-1", "Task").with_estimate(Duration::from_secs(3600));

        issue.log_time(1800);

        assert_eq!(issue.time_spent_seconds, 1800);
        assert_eq!(issue.remaining_estimate(), Some(1800));
    }

    #[test]
    fn test_link_type_inverse() {
        assert_eq!(LinkType::Blocks.inverse(), LinkType::BlockedBy);
        assert_eq!(LinkType::BlockedBy.inverse(), LinkType::Blocks);
        assert_eq!(LinkType::ParentOf.inverse(), LinkType::ChildOf);
        assert_eq!(LinkType::RelatesTo.inverse(), LinkType::RelatesTo);
    }

    #[test]
    fn test_issue_link() {
        let link = IssueLink::new("PROJ-456", LinkType::Blocks);

        assert_eq!(link.target_key, "PROJ-456");
        assert_eq!(link.link_type, LinkType::Blocks);
    }

    #[test]
    fn test_worklog_creation() {
        let log = WorkLog::new("PROJ-123", "alice", 3600).with_description("Fixed the bug");

        assert_eq!(log.issue_key, "PROJ-123");
        assert_eq!(log.author, "alice");
        assert_eq!(log.time_spent_seconds, 3600);
    }

    #[test]
    fn test_worklog_duration_string() {
        let log1 = WorkLog::new("PROJ-1", "alice", 3900); // 1h 5m
        let log2 = WorkLog::new("PROJ-1", "alice", 1800); // 30m

        assert_eq!(log1.duration_string(), "1h 5m");
        assert_eq!(log2.duration_string(), "30m");
    }

    #[test]
    fn test_sprint_creation() {
        let sprint = Sprint::new("Sprint 1").with_goal("Complete MVP");

        assert_eq!(sprint.name, "Sprint 1");
        assert_eq!(sprint.goal, Some("Complete MVP".to_string()));
        assert_eq!(sprint.status, SprintStatus::Future);
    }

    #[test]
    fn test_sprint_lifecycle() {
        let mut sprint = Sprint::new("Sprint 1");

        assert!(sprint.start().is_ok());
        assert_eq!(sprint.status, SprintStatus::Active);

        assert!(sprint.complete(21).is_ok());
        assert_eq!(sprint.status, SprintStatus::Completed);
        assert_eq!(sprint.velocity, 21);
    }

    #[test]
    fn test_sprint_add_remove_issue() {
        let mut sprint = Sprint::new("Sprint 1");

        sprint.add_issue("PROJ-1");
        sprint.add_issue("PROJ-2");

        assert_eq!(sprint.issue_keys.len(), 2);

        sprint.remove_issue("PROJ-1");

        assert_eq!(sprint.issue_keys.len(), 1);
        assert!(!sprint.issue_keys.contains(&"PROJ-1".to_string()));
    }

    #[test]
    fn test_tracker_connection() {
        let conn = TrackerConnection::new(TrackerType::GitHub, "https://github.com")
            .with_org("myorg")
            .with_project("myrepo")
            .with_token("secret");

        assert_eq!(conn.tracker_type, TrackerType::GitHub);
        assert!(conn.has_token());
        assert!(!conn.connected);
    }

    #[test]
    fn test_tracker_connection_issue_url() {
        let conn = TrackerConnection::new(TrackerType::GitHub, "https://github.com")
            .with_org("myorg")
            .with_project("myrepo");

        let url = conn.issue_url("42");
        assert!(url.contains("myorg"));
        assert!(url.contains("myrepo"));
        assert!(url.contains("42"));
    }

    #[test]
    fn test_auto_linker_github() {
        let linker = AutoLinker::new();

        let text = "Fixed issue #123 and #456";
        let refs = linker.find_references(text);

        assert!(!refs.is_empty());
    }

    #[test]
    fn test_auto_linker_jira() {
        let linker = AutoLinker::new();

        let text = "Working on PROJ-123 and TEST-456";
        let refs = linker.find_references(text);

        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_auto_linker_linkify() {
        let linker = AutoLinker::new();

        let text = "See PROJ-123";
        let linked = linker.linkify(text, "https://jira.com/browse/{key}");

        assert!(linked.contains("[PROJ-123]"));
        assert!(linked.contains("(https://jira.com/browse/PROJ-123)"));
    }

    #[test]
    fn test_issue_tracker_creation() {
        let tracker = IssueTracker::new();

        assert_eq!(tracker.issue_count(), 0);
        assert!(!tracker.is_connected());
    }

    #[test]
    fn test_issue_tracker_add_issue() {
        let mut tracker = IssueTracker::new();

        let issue = Issue::new("PROJ-1", "Task");
        tracker.add_issue(issue);

        assert_eq!(tracker.issue_count(), 1);
        assert!(tracker.get_issue("PROJ-1").is_some());
    }

    #[test]
    fn test_issue_tracker_update_status() {
        let mut tracker = IssueTracker::new();

        tracker.add_issue(Issue::new("PROJ-1", "Task"));

        assert!(tracker
            .update_status("PROJ-1", IssueStatus::InProgress)
            .is_ok());
        assert_eq!(
            tracker.get_issue("PROJ-1").unwrap().status,
            IssueStatus::InProgress
        );
    }

    #[test]
    fn test_issue_tracker_issues_by_status() {
        let mut tracker = IssueTracker::new();

        tracker.add_issue(Issue::new("PROJ-1", "Task 1"));
        tracker.add_issue(Issue::new("PROJ-2", "Task 2").with_status(IssueStatus::Done));

        let open = tracker.issues_by_status(IssueStatus::Open);
        assert_eq!(open.len(), 1);

        let done = tracker.issues_by_status(IssueStatus::Done);
        assert_eq!(done.len(), 1);
    }

    #[test]
    fn test_issue_tracker_issues_by_assignee() {
        let mut tracker = IssueTracker::new();

        tracker.add_issue(Issue::new("PROJ-1", "Task 1").with_assignee("alice"));
        tracker.add_issue(Issue::new("PROJ-2", "Task 2").with_assignee("bob"));
        tracker.add_issue(Issue::new("PROJ-3", "Task 3").with_assignee("alice"));

        let alice_issues = tracker.issues_by_assignee("alice");
        assert_eq!(alice_issues.len(), 2);
    }

    #[test]
    fn test_issue_tracker_log_work() {
        let mut tracker = IssueTracker::new();

        tracker.add_issue(Issue::new("PROJ-1", "Task"));

        let worklog = WorkLog::new("PROJ-1", "alice", 3600);
        tracker.log_work(worklog);

        let logs = tracker.worklogs_for_issue("PROJ-1");
        assert_eq!(logs.len(), 1);

        let total = tracker.total_time_logged("PROJ-1");
        assert_eq!(total, 3600);
    }

    #[test]
    fn test_issue_tracker_sprint() {
        let mut tracker = IssueTracker::new();

        let sprint = Sprint::new("Sprint 1");
        let sprint_id = tracker.add_sprint(sprint);

        assert_eq!(tracker.sprint_count(), 1);
        assert!(tracker.get_sprint(&sprint_id).is_some());
    }

    #[test]
    fn test_issue_tracker_sprint_report() {
        let mut tracker = IssueTracker::new();

        let mut sprint = Sprint::new("Sprint 1");
        sprint.add_issue("PROJ-1");
        sprint.add_issue("PROJ-2");
        let sprint_id = tracker.add_sprint(sprint);

        tracker.add_issue(
            Issue::new("PROJ-1", "Task 1")
                .with_sprint(sprint_id.clone())
                .with_story_points(3),
        );
        tracker.add_issue(
            Issue::new("PROJ-2", "Task 2")
                .with_sprint(sprint_id.clone())
                .with_story_points(5)
                .with_status(IssueStatus::Done),
        );

        let report = tracker.sprint_report(&sprint_id).unwrap();

        assert_eq!(report.total_issues, 2);
        assert_eq!(report.completed, 1);
        assert_eq!(report.total_points, 8);
        assert_eq!(report.completed_points, 5);
    }

    #[test]
    fn test_sprint_report_completion_percentage() {
        let report = SprintReport {
            sprint_name: "Test".to_string(),
            total_issues: 10,
            completed: 5,
            in_progress: 3,
            todo: 2,
            completed_points: 15,
            total_points: 30,
            remaining_days: Some(5),
        };

        assert!((report.completion_percentage() - 50.0).abs() < 0.01);
        assert!((report.points_completion_percentage() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_unique_issue_ids() {
        let i1 = Issue::new("A", "1");
        let i2 = Issue::new("B", "2");

        assert_ne!(i1.id, i2.id);
    }

    #[test]
    fn test_unique_sprint_ids() {
        let s1 = Sprint::new("Sprint 1");
        let s2 = Sprint::new("Sprint 2");

        assert_ne!(s1.id, s2.id);
    }

    #[test]
    fn test_unique_worklog_ids() {
        let w1 = WorkLog::new("A", "alice", 100);
        let w2 = WorkLog::new("B", "bob", 200);

        assert_ne!(w1.id, w2.id);
    }

    #[test]
    fn test_issue_add_link() {
        let mut issue = Issue::new("PROJ-1", "Task");
        issue.add_link(IssueLink::new("PROJ-2", LinkType::Blocks));

        assert_eq!(issue.links.len(), 1);
    }

    #[test]
    fn test_issue_story_points() {
        let issue = Issue::new("PROJ-1", "Task").with_story_points(5);

        assert_eq!(issue.story_points, Some(5));
    }

    #[test]
    fn test_sprint_status_display() {
        assert_eq!(format!("{}", SprintStatus::Future), "Future");
        assert_eq!(format!("{}", SprintStatus::Active), "Active");
        assert_eq!(format!("{}", SprintStatus::Completed), "Completed");
    }

    #[test]
    fn test_issue_tracker_open_issues() {
        let mut tracker = IssueTracker::new();

        tracker.add_issue(Issue::new("PROJ-1", "Open Task"));
        tracker.add_issue(Issue::new("PROJ-2", "Done Task").with_status(IssueStatus::Done));

        let open = tracker.open_issues();
        assert_eq!(open.len(), 1);
    }

    #[test]
    fn test_issue_tracker_connect() {
        let mut tracker = IssueTracker::new();

        let mut conn = TrackerConnection::new(TrackerType::GitHub, "https://github.com");
        conn.mark_connected();

        tracker.connect(conn);

        assert!(tracker.is_connected());
    }

    #[test]
    fn test_tracker_connection_mark_disconnected() {
        let mut conn = TrackerConnection::new(TrackerType::Jira, "https://jira.example.com");

        conn.mark_connected();
        assert!(conn.connected);

        conn.mark_disconnected();
        assert!(!conn.connected);
    }
}
