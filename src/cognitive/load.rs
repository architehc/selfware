//! Cognitive Load Reducer
//!
//! Accessibility features to reduce overwhelm through progressive disclosure,
//! simplified views, context summaries, and focus mode.

use std::collections::HashMap;

/// Detail level for progressive disclosure
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DetailLevel {
    /// Minimal detail - just the essentials
    Minimal,
    /// Basic detail - key information only
    Basic,
    /// Standard detail - normal output
    Standard,
    /// Detailed - extra information
    Detailed,
    /// Verbose - all available information
    Verbose,
}

impl std::fmt::Display for DetailLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetailLevel::Minimal => write!(f, "Minimal"),
            DetailLevel::Basic => write!(f, "Basic"),
            DetailLevel::Standard => write!(f, "Standard"),
            DetailLevel::Detailed => write!(f, "Detailed"),
            DetailLevel::Verbose => write!(f, "Verbose"),
        }
    }
}

impl DetailLevel {
    /// Get numeric level (1-5)
    pub fn level(&self) -> u8 {
        match self {
            DetailLevel::Minimal => 1,
            DetailLevel::Basic => 2,
            DetailLevel::Standard => 3,
            DetailLevel::Detailed => 4,
            DetailLevel::Verbose => 5,
        }
    }

    /// Create from numeric level
    pub fn from_level(level: u8) -> Self {
        match level {
            0 | 1 => DetailLevel::Minimal,
            2 => DetailLevel::Basic,
            3 => DetailLevel::Standard,
            4 => DetailLevel::Detailed,
            _ => DetailLevel::Verbose,
        }
    }

    /// Increase detail level
    pub fn more_detail(&self) -> Self {
        DetailLevel::from_level(self.level() + 1)
    }

    /// Decrease detail level
    pub fn less_detail(&self) -> Self {
        DetailLevel::from_level(self.level().saturating_sub(1))
    }
}

/// Focus area for focus mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusArea {
    /// Current file only
    CurrentFile,
    /// Current function/method
    CurrentFunction,
    /// Current task
    CurrentTask,
    /// Errors and warnings only
    ErrorsOnly,
    /// Test results only
    TestsOnly,
    /// Git changes only
    GitChanges,
    /// Custom focus
    Custom,
}

impl std::fmt::Display for FocusArea {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FocusArea::CurrentFile => write!(f, "Current File"),
            FocusArea::CurrentFunction => write!(f, "Current Function"),
            FocusArea::CurrentTask => write!(f, "Current Task"),
            FocusArea::ErrorsOnly => write!(f, "Errors Only"),
            FocusArea::TestsOnly => write!(f, "Tests Only"),
            FocusArea::GitChanges => write!(f, "Git Changes"),
            FocusArea::Custom => write!(f, "Custom"),
        }
    }
}

/// Content that can be progressively disclosed
#[derive(Debug, Clone)]
pub struct ProgressiveContent {
    /// Content at each detail level
    levels: HashMap<DetailLevel, String>,
    /// Tags for categorization
    tags: Vec<String>,
    /// Priority (higher = more important)
    priority: u8,
}

impl ProgressiveContent {
    /// Create new content with minimal version
    pub fn new(minimal: impl Into<String>) -> Self {
        let mut levels = HashMap::new();
        levels.insert(DetailLevel::Minimal, minimal.into());
        Self {
            levels,
            tags: Vec::new(),
            priority: 5,
        }
    }

    /// Add content for a detail level
    pub fn with_level(mut self, level: DetailLevel, content: impl Into<String>) -> Self {
        self.levels.insert(level, content.into());
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    /// Get content at the specified level, falling back to simpler versions
    pub fn get(&self, level: DetailLevel) -> &str {
        // Try requested level first
        if let Some(content) = self.levels.get(&level) {
            return content;
        }

        // Fall back to lower levels
        for l in (1..=level.level()).rev() {
            if let Some(content) = self.levels.get(&DetailLevel::from_level(l)) {
                return content;
            }
        }

        // Default to minimal if nothing found
        self.levels
            .get(&DetailLevel::Minimal)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Check if content has a specific tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

/// Context summary for reducing information overload
#[derive(Debug, Clone)]
pub struct ContextSummary {
    /// Main summary (one sentence)
    pub headline: String,
    /// Key points (bullet points)
    pub key_points: Vec<String>,
    /// Details (expandable sections)
    pub details: HashMap<String, String>,
    /// Related items
    pub related: Vec<String>,
    /// Suggested actions
    pub actions: Vec<SuggestedAction>,
}

impl ContextSummary {
    /// Create a new context summary
    pub fn new(headline: impl Into<String>) -> Self {
        Self {
            headline: headline.into(),
            key_points: Vec::new(),
            details: HashMap::new(),
            related: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Add a key point
    pub fn with_point(mut self, point: impl Into<String>) -> Self {
        self.key_points.push(point.into());
        self
    }

    /// Add detail section
    pub fn with_detail(mut self, title: impl Into<String>, content: impl Into<String>) -> Self {
        self.details.insert(title.into(), content.into());
        self
    }

    /// Add related item
    pub fn with_related(mut self, item: impl Into<String>) -> Self {
        self.related.push(item.into());
        self
    }

    /// Add suggested action
    pub fn with_action(mut self, action: SuggestedAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Render at detail level
    pub fn render(&self, level: DetailLevel) -> String {
        let mut output = String::new();

        output.push_str(&self.headline);
        output.push('\n');

        if level >= DetailLevel::Basic && !self.key_points.is_empty() {
            output.push('\n');
            for point in &self.key_points {
                output.push_str(&format!("- {}\n", point));
            }
        }

        if level >= DetailLevel::Detailed && !self.details.is_empty() {
            output.push('\n');
            for (title, content) in &self.details {
                output.push_str(&format!("## {}\n{}\n\n", title, content));
            }
        }

        if level >= DetailLevel::Verbose && !self.related.is_empty() {
            output.push_str("\nRelated: ");
            output.push_str(&self.related.join(", "));
            output.push('\n');
        }

        if level >= DetailLevel::Basic && !self.actions.is_empty() {
            output.push_str("\nSuggested actions:\n");
            for action in &self.actions {
                output.push_str(&format!("- {}\n", action.label));
            }
        }

        output
    }
}

/// Suggested action for user
#[derive(Debug, Clone)]
pub struct SuggestedAction {
    /// Action label
    pub label: String,
    /// Action description
    pub description: String,
    /// Command to execute (if applicable)
    pub command: Option<String>,
    /// Priority
    pub priority: Priority,
}

impl SuggestedAction {
    /// Create a new action
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: String::new(),
            command: None,
            priority: Priority::Normal,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set command
    pub fn with_command(mut self, cmd: impl Into<String>) -> Self {
        self.command = Some(cmd.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

/// Action priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Low priority
    Low,
    /// Normal priority
    Normal,
    /// High priority
    High,
    /// Urgent
    Urgent,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Low => write!(f, "Low"),
            Priority::Normal => write!(f, "Normal"),
            Priority::High => write!(f, "High"),
            Priority::Urgent => write!(f, "Urgent"),
        }
    }
}

/// Simplified view configuration
#[derive(Debug, Clone)]
pub struct SimplifiedView {
    /// View name
    pub name: String,
    /// Detail level
    pub detail_level: DetailLevel,
    /// Hide patterns (regex or glob)
    pub hide_patterns: Vec<String>,
    /// Show only patterns
    pub show_only: Vec<String>,
    /// Maximum items to show
    pub max_items: Option<usize>,
    /// Group similar items
    pub group_similar: bool,
    /// Collapse repeated messages
    pub collapse_repeated: bool,
}

impl SimplifiedView {
    /// Create a new simplified view
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail_level: DetailLevel::Standard,
            hide_patterns: Vec::new(),
            show_only: Vec::new(),
            max_items: None,
            group_similar: false,
            collapse_repeated: false,
        }
    }

    /// Set detail level
    pub fn with_detail_level(mut self, level: DetailLevel) -> Self {
        self.detail_level = level;
        self
    }

    /// Add hide pattern
    pub fn hide(mut self, pattern: impl Into<String>) -> Self {
        self.hide_patterns.push(pattern.into());
        self
    }

    /// Add show only pattern
    pub fn show_only(mut self, pattern: impl Into<String>) -> Self {
        self.show_only.push(pattern.into());
        self
    }

    /// Set max items
    pub fn with_max_items(mut self, max: usize) -> Self {
        self.max_items = Some(max);
        self
    }

    /// Enable grouping
    pub fn group_similar(mut self) -> Self {
        self.group_similar = true;
        self
    }

    /// Enable collapse repeated
    pub fn collapse_repeated(mut self) -> Self {
        self.collapse_repeated = true;
        self
    }

    /// Create minimal view preset
    pub fn minimal() -> Self {
        Self::new("Minimal")
            .with_detail_level(DetailLevel::Minimal)
            .with_max_items(5)
            .group_similar()
            .collapse_repeated()
    }

    /// Create errors-only view preset
    pub fn errors_only() -> Self {
        Self::new("Errors Only")
            .with_detail_level(DetailLevel::Basic)
            .show_only("error")
            .show_only("Error")
            .show_only("ERROR")
    }

    /// Create summary view preset
    pub fn summary() -> Self {
        Self::new("Summary")
            .with_detail_level(DetailLevel::Basic)
            .with_max_items(10)
            .group_similar()
    }
}

/// Focus mode configuration
#[derive(Debug, Clone)]
pub struct FocusMode {
    /// Whether focus mode is active
    pub active: bool,
    /// Current focus area
    pub focus_area: FocusArea,
    /// Filter criteria
    pub filters: Vec<FocusFilter>,
    /// Hide distractions
    pub hide_distractions: bool,
    /// Mute notifications
    pub mute_notifications: bool,
    /// Time-limited focus (in minutes)
    pub time_limit: Option<u32>,
}

impl Default for FocusMode {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusMode {
    /// Create new focus mode (inactive by default)
    pub fn new() -> Self {
        Self {
            active: false,
            focus_area: FocusArea::CurrentTask,
            filters: Vec::new(),
            hide_distractions: true,
            mute_notifications: false,
            time_limit: None,
        }
    }

    /// Activate focus mode
    pub fn activate(mut self) -> Self {
        self.active = true;
        self
    }

    /// Set focus area
    pub fn with_area(mut self, area: FocusArea) -> Self {
        self.focus_area = area;
        self
    }

    /// Add filter
    pub fn with_filter(mut self, filter: FocusFilter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Enable distraction hiding
    pub fn hide_distractions(mut self) -> Self {
        self.hide_distractions = true;
        self
    }

    /// Enable notification muting
    pub fn mute_notifications(mut self) -> Self {
        self.mute_notifications = true;
        self
    }

    /// Set time limit
    pub fn with_time_limit(mut self, minutes: u32) -> Self {
        self.time_limit = Some(minutes);
        self
    }

    /// Check if item should be shown
    pub fn should_show(&self, item: &FocusItem) -> bool {
        if !self.active {
            return true;
        }

        // Check if item matches focus area
        let matches_area = match self.focus_area {
            FocusArea::ErrorsOnly => item.is_error,
            FocusArea::TestsOnly => item.is_test_related,
            FocusArea::CurrentFile => item
                .file
                .as_ref()
                .is_some_and(|f| f == &item.current_file.clone().unwrap_or_default()),
            FocusArea::GitChanges => item.is_git_change,
            _ => true,
        };

        if !matches_area {
            return false;
        }

        // Check filters
        for filter in &self.filters {
            if !filter.matches(item) {
                return false;
            }
        }

        true
    }
}

/// Focus filter
#[derive(Debug, Clone)]
pub struct FocusFilter {
    /// Filter type
    pub filter_type: FilterType,
    /// Pattern to match
    pub pattern: String,
    /// Include or exclude
    pub include: bool,
}

impl FocusFilter {
    /// Create include filter
    pub fn include(filter_type: FilterType, pattern: impl Into<String>) -> Self {
        Self {
            filter_type,
            pattern: pattern.into(),
            include: true,
        }
    }

    /// Create exclude filter
    pub fn exclude(filter_type: FilterType, pattern: impl Into<String>) -> Self {
        Self {
            filter_type,
            pattern: pattern.into(),
            include: false,
        }
    }

    /// Check if item matches filter
    pub fn matches(&self, item: &FocusItem) -> bool {
        let matches = match self.filter_type {
            FilterType::Tag => item.tags.iter().any(|t| t.contains(&self.pattern)),
            FilterType::File => item
                .file
                .as_ref()
                .is_some_and(|f| f.contains(&self.pattern)),
            FilterType::Content => item.content.contains(&self.pattern),
            FilterType::Priority => item.priority.to_string() == self.pattern,
        };

        if self.include {
            matches
        } else {
            !matches
        }
    }
}

/// Filter type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    /// Filter by tag
    Tag,
    /// Filter by file
    File,
    /// Filter by content
    Content,
    /// Filter by priority
    Priority,
}

/// Item to check against focus filter
#[derive(Debug, Clone)]
pub struct FocusItem {
    /// Content
    pub content: String,
    /// Tags
    pub tags: Vec<String>,
    /// File path
    pub file: Option<String>,
    /// Current file for comparison
    pub current_file: Option<String>,
    /// Priority
    pub priority: Priority,
    /// Is error
    pub is_error: bool,
    /// Is test related
    pub is_test_related: bool,
    /// Is git change
    pub is_git_change: bool,
}

impl FocusItem {
    /// Create a new focus item
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tags: Vec::new(),
            file: None,
            current_file: None,
            priority: Priority::Normal,
            is_error: false,
            is_test_related: false,
            is_git_change: false,
        }
    }

    /// Set file
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Set current file
    pub fn with_current_file(mut self, file: impl Into<String>) -> Self {
        self.current_file = Some(file.into());
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Mark as error
    pub fn as_error(mut self) -> Self {
        self.is_error = true;
        self
    }

    /// Mark as test related
    pub fn as_test(mut self) -> Self {
        self.is_test_related = true;
        self
    }

    /// Mark as git change
    pub fn as_git_change(mut self) -> Self {
        self.is_git_change = true;
        self
    }
}

/// Cognitive load reducer
#[derive(Debug)]
pub struct CognitiveLoadReducer {
    /// Current detail level
    detail_level: DetailLevel,
    /// Active view
    active_view: SimplifiedView,
    /// Focus mode
    focus_mode: FocusMode,
    /// Content cache for deduplication
    _content_cache: Vec<String>,
    /// Repetition threshold for collapsing
    repetition_threshold: usize,
}

impl Default for CognitiveLoadReducer {
    fn default() -> Self {
        Self::new()
    }
}

impl CognitiveLoadReducer {
    /// Create new reducer with defaults
    pub fn new() -> Self {
        Self {
            detail_level: DetailLevel::Standard,
            active_view: SimplifiedView::new("Default"),
            focus_mode: FocusMode::new(),
            _content_cache: Vec::new(),
            repetition_threshold: 3,
        }
    }

    /// Set detail level
    pub fn set_detail_level(&mut self, level: DetailLevel) {
        self.detail_level = level;
        self.active_view.detail_level = level;
    }

    /// Get current detail level
    pub fn detail_level(&self) -> DetailLevel {
        self.detail_level
    }

    /// Set active view
    pub fn set_view(&mut self, view: SimplifiedView) {
        self.active_view = view;
    }

    /// Get active view
    pub fn view(&self) -> &SimplifiedView {
        &self.active_view
    }

    /// Enable focus mode
    pub fn enable_focus(&mut self, area: FocusArea) {
        self.focus_mode = FocusMode::new().activate().with_area(area);
    }

    /// Disable focus mode
    pub fn disable_focus(&mut self) {
        self.focus_mode.active = false;
    }

    /// Check if focus mode is active
    pub fn is_focused(&self) -> bool {
        self.focus_mode.active
    }

    /// Get focus mode
    pub fn focus_mode(&self) -> &FocusMode {
        &self.focus_mode
    }

    /// Set focus mode
    pub fn set_focus_mode(&mut self, mode: FocusMode) {
        self.focus_mode = mode;
    }

    /// Check if content should be shown
    pub fn should_show(&self, item: &FocusItem) -> bool {
        self.focus_mode.should_show(item)
    }

    /// Process and simplify text output
    pub fn simplify_output(&mut self, lines: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        let mut seen_patterns: HashMap<String, usize> = HashMap::new();

        for line in lines {
            // Check hide patterns
            if self
                .active_view
                .hide_patterns
                .iter()
                .any(|p| line.contains(p))
            {
                continue;
            }

            // Check show only patterns
            if !self.active_view.show_only.is_empty()
                && !self.active_view.show_only.iter().any(|p| line.contains(p))
            {
                continue;
            }

            // Collapse repeated content
            if self.active_view.collapse_repeated {
                let pattern = self.extract_pattern(line);
                let count = seen_patterns.entry(pattern.clone()).or_insert(0);
                *count += 1;

                if *count > self.repetition_threshold {
                    continue; // Skip this repeated line
                }
            }

            result.push(line.clone());

            // Check max items
            if let Some(max) = self.active_view.max_items {
                if result.len() >= max {
                    result.push(format!("... and {} more lines", lines.len() - max));
                    break;
                }
            }
        }

        result
    }

    /// Extract pattern from line for deduplication
    fn extract_pattern(&self, line: &str) -> String {
        // Remove numbers and specific identifiers to find the pattern
        let mut pattern = String::new();
        let mut in_number = false;

        for c in line.chars() {
            if c.is_ascii_digit() {
                if !in_number {
                    pattern.push('#');
                    in_number = true;
                }
            } else {
                in_number = false;
                pattern.push(c);
            }
        }

        pattern
    }

    /// Summarize a list of items
    pub fn summarize(&self, items: &[String], category: &str) -> ContextSummary {
        let count = items.len();

        let headline = if count == 0 {
            format!("No {} to show", category)
        } else if count == 1 {
            format!("1 {}", category)
        } else {
            format!("{} {}", count, category)
        };

        let mut summary = ContextSummary::new(headline);

        // Add first few items as key points (based on detail level)
        let max_points = match self.detail_level {
            DetailLevel::Minimal => 1,
            DetailLevel::Basic => 3,
            DetailLevel::Standard => 5,
            DetailLevel::Detailed => 10,
            DetailLevel::Verbose => items.len(),
        };

        for (i, item) in items.iter().take(max_points).enumerate() {
            // Truncate long items
            let truncated = if item.len() > 80 {
                let safe_truncate: String = item.chars().take(77).collect();
                format!("{}...", safe_truncate)
            } else {
                item.clone()
            };
            summary.key_points.push(truncated);

            if i == max_points - 1 && items.len() > max_points {
                summary
                    .key_points
                    .push(format!("... and {} more", items.len() - max_points));
                break;
            }
        }

        summary
    }

    /// Create preset configurations
    pub fn preset_minimal(&mut self) {
        self.detail_level = DetailLevel::Minimal;
        self.active_view = SimplifiedView::minimal();
    }

    /// Create preset for errors
    pub fn preset_errors_only(&mut self) {
        self.detail_level = DetailLevel::Basic;
        self.active_view = SimplifiedView::errors_only();
        self.focus_mode = FocusMode::new().activate().with_area(FocusArea::ErrorsOnly);
    }

    /// Create preset for deep work
    pub fn preset_deep_work(&mut self) {
        self.detail_level = DetailLevel::Standard;
        self.active_view = SimplifiedView::new("Deep Work")
            .with_max_items(10)
            .collapse_repeated();
        self.focus_mode = FocusMode::new()
            .activate()
            .with_area(FocusArea::CurrentTask)
            .hide_distractions()
            .mute_notifications();
    }

    /// Increase detail level
    pub fn more_detail(&mut self) {
        self.detail_level = self.detail_level.more_detail();
        self.active_view.detail_level = self.detail_level;
    }

    /// Decrease detail level
    pub fn less_detail(&mut self) {
        self.detail_level = self.detail_level.less_detail();
        self.active_view.detail_level = self.detail_level;
    }
}

/// Distraction types that can be hidden
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Distraction {
    /// Status bar updates
    StatusUpdates,
    /// Progress spinners
    ProgressIndicators,
    /// Verbose logging
    VerboseLogs,
    /// Suggestions
    Suggestions,
    /// Tips and hints
    Tips,
    /// Marketing messages
    Marketing,
    /// News/updates
    News,
}

impl std::fmt::Display for Distraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Distraction::StatusUpdates => write!(f, "Status Updates"),
            Distraction::ProgressIndicators => write!(f, "Progress Indicators"),
            Distraction::VerboseLogs => write!(f, "Verbose Logs"),
            Distraction::Suggestions => write!(f, "Suggestions"),
            Distraction::Tips => write!(f, "Tips"),
            Distraction::Marketing => write!(f, "Marketing"),
            Distraction::News => write!(f, "News"),
        }
    }
}

/// Distraction filter
#[derive(Debug)]
pub struct DistractionFilter {
    /// Distractions to hide
    hidden: std::collections::HashSet<Distraction>,
}

impl Default for DistractionFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl DistractionFilter {
    /// Create new filter (nothing hidden)
    pub fn new() -> Self {
        Self {
            hidden: std::collections::HashSet::new(),
        }
    }

    /// Hide a distraction type
    pub fn hide(&mut self, distraction: Distraction) {
        self.hidden.insert(distraction);
    }

    /// Show a distraction type
    pub fn show(&mut self, distraction: Distraction) {
        self.hidden.remove(&distraction);
    }

    /// Check if distraction is hidden
    pub fn is_hidden(&self, distraction: Distraction) -> bool {
        self.hidden.contains(&distraction)
    }

    /// Hide all distractions
    pub fn hide_all(&mut self) {
        use Distraction::*;
        for d in [
            StatusUpdates,
            ProgressIndicators,
            VerboseLogs,
            Suggestions,
            Tips,
            Marketing,
            News,
        ] {
            self.hidden.insert(d);
        }
    }

    /// Show all distractions
    pub fn show_all(&mut self) {
        self.hidden.clear();
    }

    /// Create focus mode filter
    pub fn focus_mode() -> Self {
        let mut filter = Self::new();
        filter.hide(Distraction::Marketing);
        filter.hide(Distraction::News);
        filter.hide(Distraction::Tips);
        filter.hide(Distraction::VerboseLogs);
        filter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detail_level_ordering() {
        assert!(DetailLevel::Minimal < DetailLevel::Basic);
        assert!(DetailLevel::Basic < DetailLevel::Standard);
        assert!(DetailLevel::Standard < DetailLevel::Detailed);
        assert!(DetailLevel::Detailed < DetailLevel::Verbose);
    }

    #[test]
    fn test_detail_level_conversion() {
        assert_eq!(DetailLevel::from_level(1), DetailLevel::Minimal);
        assert_eq!(DetailLevel::from_level(3), DetailLevel::Standard);
        assert_eq!(DetailLevel::from_level(5), DetailLevel::Verbose);
    }

    #[test]
    fn test_detail_level_more_less() {
        let level = DetailLevel::Standard;
        assert_eq!(level.more_detail(), DetailLevel::Detailed);
        assert_eq!(level.less_detail(), DetailLevel::Basic);
    }

    #[test]
    fn test_detail_level_bounds() {
        let minimal = DetailLevel::Minimal;
        assert_eq!(minimal.less_detail(), DetailLevel::Minimal);

        let verbose = DetailLevel::Verbose;
        assert_eq!(verbose.more_detail(), DetailLevel::Verbose);
    }

    #[test]
    fn test_focus_area_display() {
        assert_eq!(format!("{}", FocusArea::CurrentFile), "Current File");
        assert_eq!(format!("{}", FocusArea::ErrorsOnly), "Errors Only");
    }

    #[test]
    fn test_progressive_content_creation() {
        let content = ProgressiveContent::new("Brief version")
            .with_level(DetailLevel::Detailed, "Detailed version")
            .with_tag("important");

        assert_eq!(content.get(DetailLevel::Minimal), "Brief version");
        assert_eq!(content.get(DetailLevel::Detailed), "Detailed version");
        assert!(content.has_tag("important"));
    }

    #[test]
    fn test_progressive_content_fallback() {
        let content = ProgressiveContent::new("Only minimal");

        // Should fall back to minimal for all levels
        assert_eq!(content.get(DetailLevel::Verbose), "Only minimal");
    }

    #[test]
    fn test_context_summary_creation() {
        let summary = ContextSummary::new("Test headline")
            .with_point("Point 1")
            .with_point("Point 2")
            .with_detail("Section", "Content");

        assert_eq!(summary.headline, "Test headline");
        assert_eq!(summary.key_points.len(), 2);
        assert!(summary.details.contains_key("Section"));
    }

    #[test]
    fn test_context_summary_render() {
        let summary = ContextSummary::new("Headline").with_point("Point 1");

        let minimal = summary.render(DetailLevel::Minimal);
        let basic = summary.render(DetailLevel::Basic);

        assert!(minimal.contains("Headline"));
        assert!(!minimal.contains("Point 1"));
        assert!(basic.contains("Point 1"));
    }

    #[test]
    fn test_suggested_action() {
        let action = SuggestedAction::new("Run tests")
            .with_command("cargo test")
            .with_priority(Priority::High);

        assert_eq!(action.label, "Run tests");
        assert_eq!(action.command, Some("cargo test".to_string()));
        assert_eq!(action.priority, Priority::High);
    }

    #[test]
    fn test_simplified_view_creation() {
        let view = SimplifiedView::new("Test")
            .with_detail_level(DetailLevel::Basic)
            .hide("debug")
            .with_max_items(10);

        assert_eq!(view.name, "Test");
        assert_eq!(view.detail_level, DetailLevel::Basic);
        assert!(view.hide_patterns.contains(&"debug".to_string()));
        assert_eq!(view.max_items, Some(10));
    }

    #[test]
    fn test_simplified_view_presets() {
        let minimal = SimplifiedView::minimal();
        assert_eq!(minimal.detail_level, DetailLevel::Minimal);
        assert!(minimal.group_similar);

        let errors = SimplifiedView::errors_only();
        assert!(!errors.show_only.is_empty());
    }

    #[test]
    fn test_focus_mode_creation() {
        let mode = FocusMode::new()
            .activate()
            .with_area(FocusArea::ErrorsOnly)
            .mute_notifications();

        assert!(mode.active);
        assert_eq!(mode.focus_area, FocusArea::ErrorsOnly);
        assert!(mode.mute_notifications);
    }

    #[test]
    fn test_focus_mode_should_show_errors() {
        let mode = FocusMode::new().activate().with_area(FocusArea::ErrorsOnly);

        let error_item = FocusItem::new("Error message").as_error();
        let normal_item = FocusItem::new("Normal message");

        assert!(mode.should_show(&error_item));
        assert!(!mode.should_show(&normal_item));
    }

    #[test]
    fn test_focus_mode_inactive() {
        let mode = FocusMode::new(); // Not activated

        let item = FocusItem::new("Anything");

        assert!(mode.should_show(&item));
    }

    #[test]
    fn test_focus_filter_include() {
        let filter = FocusFilter::include(FilterType::Tag, "important");

        let matches = FocusItem::new("Test").with_tag("important");
        let no_match = FocusItem::new("Test").with_tag("other");

        assert!(filter.matches(&matches));
        assert!(!filter.matches(&no_match));
    }

    #[test]
    fn test_focus_filter_exclude() {
        let filter = FocusFilter::exclude(FilterType::Content, "debug");

        let excluded = FocusItem::new("debug message");
        let included = FocusItem::new("normal message");

        assert!(!filter.matches(&excluded));
        assert!(filter.matches(&included));
    }

    #[test]
    fn test_focus_item_creation() {
        let item = FocusItem::new("Content")
            .with_file("src/lib.rs")
            .with_tag("test")
            .as_error();

        assert!(item.is_error);
        assert!(item.file.is_some());
        assert!(item.tags.contains(&"test".to_string()));
    }

    #[test]
    fn test_cognitive_load_reducer_creation() {
        let reducer = CognitiveLoadReducer::new();

        assert_eq!(reducer.detail_level(), DetailLevel::Standard);
        assert!(!reducer.is_focused());
    }

    #[test]
    fn test_cognitive_load_reducer_detail_level() {
        let mut reducer = CognitiveLoadReducer::new();

        reducer.set_detail_level(DetailLevel::Minimal);
        assert_eq!(reducer.detail_level(), DetailLevel::Minimal);

        reducer.more_detail();
        assert_eq!(reducer.detail_level(), DetailLevel::Basic);

        reducer.less_detail();
        assert_eq!(reducer.detail_level(), DetailLevel::Minimal);
    }

    #[test]
    fn test_cognitive_load_reducer_focus() {
        let mut reducer = CognitiveLoadReducer::new();

        reducer.enable_focus(FocusArea::ErrorsOnly);
        assert!(reducer.is_focused());

        reducer.disable_focus();
        assert!(!reducer.is_focused());
    }

    #[test]
    fn test_cognitive_load_reducer_simplify_output() {
        let mut reducer = CognitiveLoadReducer::new();
        reducer.set_view(SimplifiedView::new("Test").with_max_items(2));

        let lines: Vec<String> = vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
            "Line 4".to_string(),
        ];

        let result = reducer.simplify_output(&lines);

        assert!(result.len() <= 3); // max_items + "and X more"
    }

    #[test]
    fn test_cognitive_load_reducer_simplify_hide() {
        let mut reducer = CognitiveLoadReducer::new();
        reducer.set_view(SimplifiedView::new("Test").hide("DEBUG"));

        let lines: Vec<String> = vec![
            "DEBUG: something".to_string(),
            "INFO: important".to_string(),
        ];

        let result = reducer.simplify_output(&lines);

        assert_eq!(result.len(), 1);
        assert!(result[0].contains("INFO"));
    }

    #[test]
    fn test_cognitive_load_reducer_summarize() {
        let reducer = CognitiveLoadReducer::new();

        let items = vec![
            "Item 1".to_string(),
            "Item 2".to_string(),
            "Item 3".to_string(),
        ];

        let summary = reducer.summarize(&items, "items");

        assert!(summary.headline.contains("3"));
        assert!(!summary.key_points.is_empty());
    }

    #[test]
    fn test_cognitive_load_reducer_presets() {
        let mut reducer = CognitiveLoadReducer::new();

        reducer.preset_minimal();
        assert_eq!(reducer.detail_level(), DetailLevel::Minimal);

        reducer.preset_errors_only();
        assert!(reducer.is_focused());

        reducer.preset_deep_work();
        assert!(reducer.focus_mode().hide_distractions);
    }

    #[test]
    fn test_distraction_filter_creation() {
        let mut filter = DistractionFilter::new();

        filter.hide(Distraction::Marketing);
        assert!(filter.is_hidden(Distraction::Marketing));
        assert!(!filter.is_hidden(Distraction::Tips));

        filter.show(Distraction::Marketing);
        assert!(!filter.is_hidden(Distraction::Marketing));
    }

    #[test]
    fn test_distraction_filter_hide_all() {
        let mut filter = DistractionFilter::new();
        filter.hide_all();

        assert!(filter.is_hidden(Distraction::Marketing));
        assert!(filter.is_hidden(Distraction::News));
        assert!(filter.is_hidden(Distraction::Tips));
    }

    #[test]
    fn test_distraction_filter_show_all() {
        let mut filter = DistractionFilter::new();
        filter.hide_all();
        filter.show_all();

        assert!(!filter.is_hidden(Distraction::Marketing));
    }

    #[test]
    fn test_distraction_filter_focus_mode() {
        let filter = DistractionFilter::focus_mode();

        assert!(filter.is_hidden(Distraction::Marketing));
        assert!(filter.is_hidden(Distraction::News));
        assert!(!filter.is_hidden(Distraction::StatusUpdates));
    }

    #[test]
    fn test_distraction_display() {
        assert_eq!(format!("{}", Distraction::VerboseLogs), "Verbose Logs");
        assert_eq!(format!("{}", Distraction::Marketing), "Marketing");
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Low < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::Urgent);
    }

    #[test]
    fn test_context_summary_with_action() {
        let action = SuggestedAction::new("Fix error");
        let summary = ContextSummary::new("Test").with_action(action);

        assert_eq!(summary.actions.len(), 1);
    }

    #[test]
    fn test_progressive_content_priority() {
        let content = ProgressiveContent::new("Test").with_priority(8);

        assert_eq!(content.priority, 8);
    }

    #[test]
    fn test_simplified_view_group_collapse() {
        let view = SimplifiedView::new("Test")
            .group_similar()
            .collapse_repeated();

        assert!(view.group_similar);
        assert!(view.collapse_repeated);
    }

    #[test]
    fn test_focus_mode_with_time_limit() {
        let mode = FocusMode::new().with_time_limit(25);

        assert_eq!(mode.time_limit, Some(25));
    }

    #[test]
    fn test_focus_item_test_and_git() {
        let item = FocusItem::new("test").as_test().as_git_change();

        assert!(item.is_test_related);
        assert!(item.is_git_change);
    }
}
