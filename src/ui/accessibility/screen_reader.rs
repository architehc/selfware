//! Screen Reader Integration
//!
//! Accessibility features for screen reader compatibility.
//! Provides semantic structure, ARIA-style annotations,
//! focus management, and text-to-speech friendly output.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

static ELEMENT_COUNTER: AtomicU64 = AtomicU64::new(1);
static REGION_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_element_id() -> String {
    format!("elem-{}", ELEMENT_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_region_id() -> String {
    format!("region-{}", REGION_COUNTER.fetch_add(1, Ordering::SeqCst))
}

// ============================================================================
// ARIA Roles
// ============================================================================

/// ARIA roles for semantic structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AriaRole {
    // Document structure
    Document,
    Main,
    Navigation,
    Complementary,
    ContentInfo,
    Banner,
    Article,
    Section,
    Region,

    // Widget roles
    Button,
    Link,
    Checkbox,
    Radio,
    Textbox,
    Listbox,
    Option,
    Combobox,
    Slider,
    Spinbutton,
    Switch,
    Tab,
    Tabpanel,
    Tablist,
    Menu,
    Menuitem,
    Menubar,
    Tree,
    Treeitem,
    Grid,
    Gridcell,
    Row,
    Rowgroup,
    Columnheader,
    Rowheader,

    // Live regions
    Alert,
    Status,
    Log,
    Timer,
    Marquee,
    Progressbar,

    // Other
    Dialog,
    Alertdialog,
    Tooltip,
    Form,
    Search,
    Separator,
    Img,
    Heading,
    List,
    Listitem,
    Term,
    Definition,
    Group,
    Note,

    // Custom
    Custom(u32),
}

impl AriaRole {
    /// Get role name for screen readers
    pub fn name(&self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Main => "main",
            Self::Navigation => "navigation",
            Self::Complementary => "complementary",
            Self::ContentInfo => "contentinfo",
            Self::Banner => "banner",
            Self::Article => "article",
            Self::Section => "section",
            Self::Region => "region",
            Self::Button => "button",
            Self::Link => "link",
            Self::Checkbox => "checkbox",
            Self::Radio => "radio",
            Self::Textbox => "textbox",
            Self::Listbox => "listbox",
            Self::Option => "option",
            Self::Combobox => "combobox",
            Self::Slider => "slider",
            Self::Spinbutton => "spinbutton",
            Self::Switch => "switch",
            Self::Tab => "tab",
            Self::Tabpanel => "tabpanel",
            Self::Tablist => "tablist",
            Self::Menu => "menu",
            Self::Menuitem => "menuitem",
            Self::Menubar => "menubar",
            Self::Tree => "tree",
            Self::Treeitem => "treeitem",
            Self::Grid => "grid",
            Self::Gridcell => "gridcell",
            Self::Row => "row",
            Self::Rowgroup => "rowgroup",
            Self::Columnheader => "columnheader",
            Self::Rowheader => "rowheader",
            Self::Alert => "alert",
            Self::Status => "status",
            Self::Log => "log",
            Self::Timer => "timer",
            Self::Marquee => "marquee",
            Self::Progressbar => "progressbar",
            Self::Dialog => "dialog",
            Self::Alertdialog => "alertdialog",
            Self::Tooltip => "tooltip",
            Self::Form => "form",
            Self::Search => "search",
            Self::Separator => "separator",
            Self::Img => "img",
            Self::Heading => "heading",
            Self::List => "list",
            Self::Listitem => "listitem",
            Self::Term => "term",
            Self::Definition => "definition",
            Self::Group => "group",
            Self::Note => "note",
            Self::Custom(_) => "custom",
        }
    }

    /// Check if role is a landmark
    pub fn is_landmark(&self) -> bool {
        matches!(
            self,
            Self::Main
                | Self::Navigation
                | Self::Complementary
                | Self::ContentInfo
                | Self::Banner
                | Self::Region
                | Self::Search
                | Self::Form
        )
    }

    /// Check if role is a widget
    pub fn is_widget(&self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Link
                | Self::Checkbox
                | Self::Radio
                | Self::Textbox
                | Self::Listbox
                | Self::Option
                | Self::Combobox
                | Self::Slider
                | Self::Spinbutton
                | Self::Switch
                | Self::Tab
                | Self::Menuitem
        )
    }

    /// Check if role is a live region
    pub fn is_live_region(&self) -> bool {
        matches!(
            self,
            Self::Alert | Self::Status | Self::Log | Self::Timer | Self::Marquee
        )
    }
}

// ============================================================================
// ARIA States and Properties
// ============================================================================

/// ARIA states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AriaState {
    Busy,
    Checked,
    Disabled,
    Expanded,
    Hidden,
    Invalid,
    Pressed,
    Selected,
    Current,
    Grabbed,
}

impl AriaState {
    /// Get state name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Busy => "aria-busy",
            Self::Checked => "aria-checked",
            Self::Disabled => "aria-disabled",
            Self::Expanded => "aria-expanded",
            Self::Hidden => "aria-hidden",
            Self::Invalid => "aria-invalid",
            Self::Pressed => "aria-pressed",
            Self::Selected => "aria-selected",
            Self::Current => "aria-current",
            Self::Grabbed => "aria-grabbed",
        }
    }
}

/// State value
#[derive(Debug, Clone, PartialEq)]
pub enum StateValue {
    True,
    False,
    Mixed,
    Text(String),
}

impl StateValue {
    /// Convert to string
    pub fn to_str(&self) -> String {
        match self {
            Self::True => "true".to_string(),
            Self::False => "false".to_string(),
            Self::Mixed => "mixed".to_string(),
            Self::Text(s) => s.clone(),
        }
    }
}

/// ARIA properties
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AriaProperty {
    Label,
    LabelledBy,
    DescribedBy,
    Controls,
    Owns,
    FlowTo,
    ActiveDescendant,
    Atomic,
    Autocomplete,
    Colcount,
    Colindex,
    Colspan,
    Haspopup,
    Keyshortcuts,
    Level,
    Live,
    Modal,
    Multiline,
    Multiselectable,
    Orientation,
    Placeholder,
    Posinset,
    Readonly,
    Relevant,
    Required,
    Roledescription,
    Rowcount,
    Rowindex,
    Rowspan,
    Setsize,
    Sort,
    Valuemax,
    Valuemin,
    Valuenow,
    Valuetext,
}

impl AriaProperty {
    /// Get property name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Label => "aria-label",
            Self::LabelledBy => "aria-labelledby",
            Self::DescribedBy => "aria-describedby",
            Self::Controls => "aria-controls",
            Self::Owns => "aria-owns",
            Self::FlowTo => "aria-flowto",
            Self::ActiveDescendant => "aria-activedescendant",
            Self::Atomic => "aria-atomic",
            Self::Autocomplete => "aria-autocomplete",
            Self::Colcount => "aria-colcount",
            Self::Colindex => "aria-colindex",
            Self::Colspan => "aria-colspan",
            Self::Haspopup => "aria-haspopup",
            Self::Keyshortcuts => "aria-keyshortcuts",
            Self::Level => "aria-level",
            Self::Live => "aria-live",
            Self::Modal => "aria-modal",
            Self::Multiline => "aria-multiline",
            Self::Multiselectable => "aria-multiselectable",
            Self::Orientation => "aria-orientation",
            Self::Placeholder => "aria-placeholder",
            Self::Posinset => "aria-posinset",
            Self::Readonly => "aria-readonly",
            Self::Relevant => "aria-relevant",
            Self::Required => "aria-required",
            Self::Roledescription => "aria-roledescription",
            Self::Rowcount => "aria-rowcount",
            Self::Rowindex => "aria-rowindex",
            Self::Rowspan => "aria-rowspan",
            Self::Setsize => "aria-setsize",
            Self::Sort => "aria-sort",
            Self::Valuemax => "aria-valuemax",
            Self::Valuemin => "aria-valuemin",
            Self::Valuenow => "aria-valuenow",
            Self::Valuetext => "aria-valuetext",
        }
    }
}

// ============================================================================
// Accessible Element
// ============================================================================

/// An accessible element with ARIA attributes
#[derive(Debug, Clone)]
pub struct AccessibleElement {
    pub id: String,
    pub role: AriaRole,
    pub label: Option<String>,
    pub description: Option<String>,
    pub states: HashMap<AriaState, StateValue>,
    pub properties: HashMap<AriaProperty, String>,
    pub children: Vec<String>,
    pub parent: Option<String>,
    pub content: String,
    pub focusable: bool,
    pub tab_index: Option<i32>,
}

impl AccessibleElement {
    /// Create new element
    pub fn new(role: AriaRole, content: impl Into<String>) -> Self {
        Self {
            id: generate_element_id(),
            role,
            label: None,
            description: None,
            states: HashMap::new(),
            properties: HashMap::new(),
            children: Vec::new(),
            parent: None,
            content: content.into(),
            focusable: role.is_widget(),
            tab_index: None,
        }
    }

    /// Set label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add state
    pub fn with_state(mut self, state: AriaState, value: StateValue) -> Self {
        self.states.insert(state, value);
        self
    }

    /// Add property
    pub fn with_property(mut self, property: AriaProperty, value: impl Into<String>) -> Self {
        self.properties.insert(property, value.into());
        self
    }

    /// Set focusable
    pub fn with_focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }

    /// Set tab index
    pub fn with_tab_index(mut self, index: i32) -> Self {
        self.tab_index = Some(index);
        self.focusable = true;
        self
    }

    /// Add child element ID
    pub fn add_child(&mut self, child_id: impl Into<String>) {
        self.children.push(child_id.into());
    }

    /// Get accessible name (label or content)
    pub fn accessible_name(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.content)
    }

    /// Check if element is disabled
    pub fn is_disabled(&self) -> bool {
        self.states
            .get(&AriaState::Disabled)
            .map(|v| *v == StateValue::True)
            .unwrap_or(false)
    }

    /// Check if element is hidden
    pub fn is_hidden(&self) -> bool {
        self.states
            .get(&AriaState::Hidden)
            .map(|v| *v == StateValue::True)
            .unwrap_or(false)
    }

    /// Check if element is expanded
    pub fn is_expanded(&self) -> bool {
        self.states
            .get(&AriaState::Expanded)
            .map(|v| *v == StateValue::True)
            .unwrap_or(false)
    }

    /// Generate screen reader announcement
    pub fn announce(&self) -> String {
        let mut parts = Vec::new();

        // Role announcement
        if self.role.is_landmark() {
            parts.push(format!("{} landmark", self.role.name()));
        } else {
            parts.push(self.role.name().to_string());
        }

        // Label
        if let Some(label) = &self.label {
            parts.push(label.clone());
        } else if !self.content.is_empty() {
            parts.push(self.content.clone());
        }

        // States
        if self.is_disabled() {
            parts.push("disabled".to_string());
        }
        if self.is_expanded() {
            parts.push("expanded".to_string());
        }
        if let Some(checked) = self.states.get(&AriaState::Checked) {
            match checked {
                StateValue::True => parts.push("checked".to_string()),
                StateValue::Mixed => parts.push("partially checked".to_string()),
                _ => {}
            }
        }
        if let Some(pressed) = self.states.get(&AriaState::Pressed) {
            if *pressed == StateValue::True {
                parts.push("pressed".to_string());
            }
        }

        // Description
        if let Some(desc) = &self.description {
            parts.push(desc.clone());
        }

        parts.join(", ")
    }
}

// ============================================================================
// Live Region
// ============================================================================

/// Live region politeness levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRegionPoliteness {
    Off,
    Polite,
    Assertive,
}

impl LiveRegionPoliteness {
    /// Get string value
    pub fn value(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Polite => "polite",
            Self::Assertive => "assertive",
        }
    }
}

/// A live region for dynamic content announcements
#[derive(Debug, Clone)]
pub struct LiveRegion {
    pub id: String,
    pub politeness: LiveRegionPoliteness,
    pub atomic: bool,
    pub relevant: Vec<LiveRegionRelevant>,
    pub messages: Vec<LiveMessage>,
}

/// What changes are relevant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRegionRelevant {
    Additions,
    Removals,
    Text,
    All,
}

/// A message in a live region
#[derive(Debug, Clone)]
pub struct LiveMessage {
    pub content: String,
    pub timestamp: SystemTime,
    pub announced: bool,
}

impl LiveRegion {
    /// Create new live region
    pub fn new(politeness: LiveRegionPoliteness) -> Self {
        Self {
            id: generate_region_id(),
            politeness,
            atomic: true,
            relevant: vec![LiveRegionRelevant::Additions, LiveRegionRelevant::Text],
            messages: Vec::new(),
        }
    }

    /// Create polite region
    pub fn polite() -> Self {
        Self::new(LiveRegionPoliteness::Polite)
    }

    /// Create assertive region
    pub fn assertive() -> Self {
        Self::new(LiveRegionPoliteness::Assertive)
    }

    /// Set atomic
    pub fn with_atomic(mut self, atomic: bool) -> Self {
        self.atomic = atomic;
        self
    }

    /// Add relevant type
    pub fn with_relevant(mut self, relevant: LiveRegionRelevant) -> Self {
        if !self.relevant.contains(&relevant) {
            self.relevant.push(relevant);
        }
        self
    }

    /// Add message
    pub fn add_message(&mut self, content: impl Into<String>) {
        self.messages.push(LiveMessage {
            content: content.into(),
            timestamp: SystemTime::now(),
            announced: false,
        });
    }

    /// Get pending announcements
    pub fn pending_announcements(&self) -> Vec<&str> {
        self.messages
            .iter()
            .filter(|m| !m.announced)
            .map(|m| m.content.as_str())
            .collect()
    }

    /// Mark all as announced
    pub fn mark_announced(&mut self) {
        for message in &mut self.messages {
            message.announced = true;
        }
    }

    /// Clear old messages (keep last n)
    pub fn trim(&mut self, keep: usize) {
        if self.messages.len() > keep {
            self.messages = self.messages.split_off(self.messages.len() - keep);
        }
    }
}

// ============================================================================
// Focus Management
// ============================================================================

/// Focus management for keyboard navigation
#[derive(Debug)]
pub struct FocusManager {
    focus_order: Vec<String>,
    current_focus: Option<String>,
    focus_history: Vec<String>,
    focus_traps: Vec<FocusTrap>,
}

/// A focus trap (modal, dialog, etc.)
#[derive(Debug, Clone)]
pub struct FocusTrap {
    pub id: String,
    pub container_id: String,
    pub focusable_elements: Vec<String>,
    pub return_focus_to: Option<String>,
    pub active: bool,
}

impl FocusManager {
    /// Create new focus manager
    pub fn new() -> Self {
        Self {
            focus_order: Vec::new(),
            current_focus: None,
            focus_history: Vec::new(),
            focus_traps: Vec::new(),
        }
    }

    /// Set focus order
    pub fn set_focus_order(&mut self, order: Vec<String>) {
        self.focus_order = order;
    }

    /// Add element to focus order
    pub fn add_to_focus_order(&mut self, element_id: impl Into<String>) {
        self.focus_order.push(element_id.into());
    }

    /// Remove element from focus order
    pub fn remove_from_focus_order(&mut self, element_id: &str) {
        self.focus_order.retain(|id| id != element_id);
    }

    /// Get current focus
    pub fn current_focus(&self) -> Option<&str> {
        self.current_focus.as_deref()
    }

    /// Set focus to element
    pub fn focus(&mut self, element_id: impl Into<String>) -> bool {
        let element_id = element_id.into();

        // Check focus traps
        if let Some(trap) = self.active_trap() {
            if !trap.focusable_elements.contains(&element_id) {
                return false;
            }
        }

        // Save to history
        if let Some(current) = &self.current_focus {
            self.focus_history.push(current.clone());
        }

        self.current_focus = Some(element_id);
        true
    }

    /// Move focus to next element
    pub fn focus_next(&mut self) -> Option<&str> {
        let focusable = self.focusable_elements();
        if focusable.is_empty() {
            return None;
        }

        let next_index = match &self.current_focus {
            Some(current) => focusable
                .iter()
                .position(|id| id == current)
                .map(|i| (i + 1) % focusable.len())
                .unwrap_or(0),
            None => 0,
        };

        self.current_focus = Some(focusable[next_index].clone());
        self.current_focus.as_deref()
    }

    /// Move focus to previous element
    pub fn focus_previous(&mut self) -> Option<&str> {
        let focusable = self.focusable_elements();
        if focusable.is_empty() {
            return None;
        }

        let prev_index = match &self.current_focus {
            Some(current) => focusable
                .iter()
                .position(|id| id == current)
                .map(|i| if i == 0 { focusable.len() - 1 } else { i - 1 })
                .unwrap_or(focusable.len() - 1),
            None => focusable.len() - 1,
        };

        self.current_focus = Some(focusable[prev_index].clone());
        self.current_focus.as_deref()
    }

    /// Get focusable elements (considering traps)
    fn focusable_elements(&self) -> Vec<String> {
        if let Some(trap) = self.active_trap() {
            trap.focusable_elements.clone()
        } else {
            self.focus_order.clone()
        }
    }

    /// Move focus to first element
    pub fn focus_first(&mut self) -> Option<&str> {
        let focusable = self.focusable_elements();
        if focusable.is_empty() {
            return None;
        }
        self.current_focus = Some(focusable[0].clone());
        self.current_focus.as_deref()
    }

    /// Move focus to last element
    pub fn focus_last(&mut self) -> Option<&str> {
        let focusable = self.focusable_elements();
        if focusable.is_empty() {
            return None;
        }
        self.current_focus = Some(focusable[focusable.len() - 1].clone());
        self.current_focus.as_deref()
    }

    /// Return to previous focus
    pub fn return_focus(&mut self) -> Option<&str> {
        if let Some(previous) = self.focus_history.pop() {
            self.current_focus = Some(previous);
            self.current_focus.as_deref()
        } else {
            None
        }
    }

    /// Create focus trap
    pub fn create_trap(
        &mut self,
        container_id: impl Into<String>,
        focusable: Vec<String>,
    ) -> String {
        let trap = FocusTrap {
            id: generate_element_id(),
            container_id: container_id.into(),
            focusable_elements: focusable,
            return_focus_to: self.current_focus.clone(),
            active: false,
        };
        let id = trap.id.clone();
        self.focus_traps.push(trap);
        id
    }

    /// Activate focus trap
    pub fn activate_trap(&mut self, trap_id: &str) -> bool {
        for trap in &mut self.focus_traps {
            if trap.id == trap_id {
                trap.active = true;
                trap.return_focus_to = self.current_focus.clone();
                // Focus first element in trap
                if let Some(first) = trap.focusable_elements.first() {
                    self.current_focus = Some(first.clone());
                }
                return true;
            }
        }
        false
    }

    /// Deactivate focus trap
    pub fn deactivate_trap(&mut self, trap_id: &str) -> bool {
        for trap in &mut self.focus_traps {
            if trap.id == trap_id {
                trap.active = false;
                // Return focus
                if let Some(return_to) = trap.return_focus_to.clone() {
                    self.current_focus = Some(return_to);
                }
                return true;
            }
        }
        false
    }

    /// Get active focus trap
    pub fn active_trap(&self) -> Option<&FocusTrap> {
        self.focus_traps.iter().find(|t| t.active)
    }

    /// Check if inside a trap
    pub fn is_trapped(&self) -> bool {
        self.focus_traps.iter().any(|t| t.active)
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Screen Reader Output
// ============================================================================

/// Output mode for screen reader
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Brief,
    Verbose,
    Custom,
}

/// Screen reader output configuration
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub announce_role: bool,
    pub announce_state: bool,
    pub announce_description: bool,
    pub announce_shortcuts: bool,
    pub punctuation_level: PunctuationLevel,
    pub speech_rate: f32,
    pub pitch: f32,
}

/// Punctuation announcement level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunctuationLevel {
    None,
    Some,
    Most,
    All,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            mode: OutputMode::Verbose,
            announce_role: true,
            announce_state: true,
            announce_description: true,
            announce_shortcuts: false,
            punctuation_level: PunctuationLevel::Some,
            speech_rate: 1.0,
            pitch: 1.0,
        }
    }
}

impl OutputConfig {
    /// Create brief mode config
    pub fn brief() -> Self {
        Self {
            mode: OutputMode::Brief,
            announce_role: false,
            announce_state: true,
            announce_description: false,
            announce_shortcuts: false,
            punctuation_level: PunctuationLevel::None,
            speech_rate: 1.2,
            pitch: 1.0,
        }
    }

    /// Create verbose mode config
    pub fn verbose() -> Self {
        Self {
            mode: OutputMode::Verbose,
            announce_role: true,
            announce_state: true,
            announce_description: true,
            announce_shortcuts: true,
            punctuation_level: PunctuationLevel::Most,
            speech_rate: 1.0,
            pitch: 1.0,
        }
    }
}

/// Screen reader output formatter
#[derive(Debug)]
pub struct ScreenReaderOutput {
    config: OutputConfig,
    queue: Vec<Announcement>,
}

/// An announcement to be spoken
#[derive(Debug, Clone)]
pub struct Announcement {
    pub text: String,
    pub priority: AnnouncementPriority,
    pub interruptible: bool,
    pub timestamp: SystemTime,
}

/// Announcement priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnnouncementPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl ScreenReaderOutput {
    /// Create new output
    pub fn new(config: OutputConfig) -> Self {
        Self {
            config,
            queue: Vec::new(),
        }
    }

    /// Format element for announcement
    pub fn format_element(&self, element: &AccessibleElement) -> String {
        let mut parts = Vec::new();

        // Content/label first
        if let Some(label) = &element.label {
            parts.push(label.clone());
        } else if !element.content.is_empty() {
            parts.push(element.content.clone());
        }

        // Role
        if self.config.announce_role {
            if element.role.is_landmark() {
                parts.push(format!("{} landmark", element.role.name()));
            } else if element.role != AriaRole::Region {
                parts.push(element.role.name().to_string());
            }
        }

        // States
        if self.config.announce_state {
            if element.is_disabled() {
                parts.push("disabled".to_string());
            }
            if element.is_expanded() {
                parts.push("expanded".to_string());
            } else if element.states.contains_key(&AriaState::Expanded) {
                parts.push("collapsed".to_string());
            }
            if let Some(checked) = element.states.get(&AriaState::Checked) {
                match checked {
                    StateValue::True => parts.push("checked".to_string()),
                    StateValue::False => parts.push("not checked".to_string()),
                    StateValue::Mixed => parts.push("partially checked".to_string()),
                    _ => {}
                }
            }
        }

        // Description
        if self.config.announce_description {
            if let Some(desc) = &element.description {
                parts.push(desc.clone());
            }
        }

        // Keyboard shortcuts
        if self.config.announce_shortcuts {
            if let Some(shortcut) = element.properties.get(&AriaProperty::Keyshortcuts) {
                parts.push(format!("shortcut: {}", shortcut));
            }
        }

        parts.join(", ")
    }

    /// Queue announcement
    pub fn queue_announcement(&mut self, text: impl Into<String>, priority: AnnouncementPriority) {
        self.queue.push(Announcement {
            text: text.into(),
            priority,
            interruptible: priority < AnnouncementPriority::Critical,
            timestamp: SystemTime::now(),
        });

        // Sort by priority
        self.queue.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Queue element announcement
    pub fn queue_element(&mut self, element: &AccessibleElement, priority: AnnouncementPriority) {
        let text = self.format_element(element);
        self.queue_announcement(text, priority);
    }

    /// Get next announcement
    pub fn next_announcement(&mut self) -> Option<Announcement> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }

    /// Peek next announcement
    pub fn peek_announcement(&self) -> Option<&Announcement> {
        self.queue.first()
    }

    /// Clear queue
    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Get queue length
    pub fn queue_length(&self) -> usize {
        self.queue.len()
    }

    /// Interrupt current announcement (remove non-critical)
    pub fn interrupt(&mut self) {
        self.queue.retain(|a| !a.interruptible);
    }

    /// Format punctuation based on level
    pub fn format_punctuation(&self, text: &str) -> String {
        match self.config.punctuation_level {
            PunctuationLevel::None => text
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace())
                .collect(),
            PunctuationLevel::Some => text
                .replace('.', " period ")
                .replace('!', " exclamation ")
                .replace('?', " question mark "),
            PunctuationLevel::Most => text
                .replace('.', " period ")
                .replace(',', " comma ")
                .replace('!', " exclamation ")
                .replace('?', " question mark ")
                .replace(':', " colon ")
                .replace(';', " semicolon "),
            PunctuationLevel::All => text
                .chars()
                .map(|c| match c {
                    '.' => " period ".to_string(),
                    ',' => " comma ".to_string(),
                    '!' => " exclamation ".to_string(),
                    '?' => " question mark ".to_string(),
                    ':' => " colon ".to_string(),
                    ';' => " semicolon ".to_string(),
                    '-' => " dash ".to_string(),
                    '_' => " underscore ".to_string(),
                    '(' => " open paren ".to_string(),
                    ')' => " close paren ".to_string(),
                    '[' => " open bracket ".to_string(),
                    ']' => " close bracket ".to_string(),
                    '{' => " open brace ".to_string(),
                    '}' => " close brace ".to_string(),
                    '@' => " at ".to_string(),
                    '#' => " hash ".to_string(),
                    '$' => " dollar ".to_string(),
                    '%' => " percent ".to_string(),
                    '&' => " ampersand ".to_string(),
                    '*' => " asterisk ".to_string(),
                    '+' => " plus ".to_string(),
                    '=' => " equals ".to_string(),
                    '<' => " less than ".to_string(),
                    '>' => " greater than ".to_string(),
                    '/' => " slash ".to_string(),
                    '\\' => " backslash ".to_string(),
                    '|' => " pipe ".to_string(),
                    '"' => " quote ".to_string(),
                    '\'' => " apostrophe ".to_string(),
                    '`' => " backtick ".to_string(),
                    '~' => " tilde ".to_string(),
                    '^' => " caret ".to_string(),
                    _ => c.to_string(),
                })
                .collect(),
        }
    }
}

impl Default for ScreenReaderOutput {
    fn default() -> Self {
        Self::new(OutputConfig::default())
    }
}

// ============================================================================
// Accessibility Tree
// ============================================================================

/// Accessibility tree for screen reader navigation
#[derive(Debug)]
pub struct AccessibilityTree {
    elements: HashMap<String, AccessibleElement>,
    root_id: Option<String>,
    live_regions: HashMap<String, LiveRegion>,
}

impl AccessibilityTree {
    /// Create new tree
    pub fn new() -> Self {
        Self {
            elements: HashMap::new(),
            root_id: None,
            live_regions: HashMap::new(),
        }
    }

    /// Set root element
    pub fn set_root(&mut self, element: AccessibleElement) {
        self.root_id = Some(element.id.clone());
        self.elements.insert(element.id.clone(), element);
    }

    /// Add element
    pub fn add_element(&mut self, element: AccessibleElement) -> String {
        let id = element.id.clone();
        self.elements.insert(id.clone(), element);
        id
    }

    /// Add element as child of parent
    pub fn add_child(&mut self, parent_id: &str, mut element: AccessibleElement) -> Option<String> {
        element.parent = Some(parent_id.to_string());
        let id = element.id.clone();

        if let Some(parent) = self.elements.get_mut(parent_id) {
            parent.children.push(id.clone());
        } else {
            return None;
        }

        self.elements.insert(id.clone(), element);
        Some(id)
    }

    /// Get element
    pub fn get_element(&self, id: &str) -> Option<&AccessibleElement> {
        self.elements.get(id)
    }

    /// Get element mut
    pub fn get_element_mut(&mut self, id: &str) -> Option<&mut AccessibleElement> {
        self.elements.get_mut(id)
    }

    /// Get root element
    pub fn root(&self) -> Option<&AccessibleElement> {
        self.root_id.as_ref().and_then(|id| self.elements.get(id))
    }

    /// Get children of element
    pub fn children(&self, id: &str) -> Vec<&AccessibleElement> {
        self.elements
            .get(id)
            .map(|e| {
                e.children
                    .iter()
                    .filter_map(|child_id| self.elements.get(child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get parent of element
    pub fn parent(&self, id: &str) -> Option<&AccessibleElement> {
        self.elements
            .get(id)
            .and_then(|e| e.parent.as_ref())
            .and_then(|parent_id| self.elements.get(parent_id))
    }

    /// Find elements by role
    pub fn find_by_role(&self, role: AriaRole) -> Vec<&AccessibleElement> {
        self.elements.values().filter(|e| e.role == role).collect()
    }

    /// Find landmarks
    pub fn landmarks(&self) -> Vec<&AccessibleElement> {
        self.elements
            .values()
            .filter(|e| e.role.is_landmark())
            .collect()
    }

    /// Find focusable elements
    pub fn focusable_elements(&self) -> Vec<&AccessibleElement> {
        self.elements
            .values()
            .filter(|e| e.focusable && !e.is_hidden() && !e.is_disabled())
            .collect()
    }

    /// Get focus order
    pub fn focus_order(&self) -> Vec<String> {
        let mut focusable: Vec<_> = self
            .focusable_elements()
            .into_iter()
            .map(|e| (e.id.clone(), e.tab_index.unwrap_or(0)))
            .collect();

        // Sort by tab index (positive first, then 0s in document order)
        focusable.sort_by(|a, b| match (a.1, b.1) {
            (x, y) if x > 0 && y > 0 => x.cmp(&y),
            (x, _) if x > 0 => std::cmp::Ordering::Less,
            (_, y) if y > 0 => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });

        focusable.into_iter().map(|(id, _)| id).collect()
    }

    /// Add live region
    pub fn add_live_region(&mut self, region: LiveRegion) -> String {
        let id = region.id.clone();
        self.live_regions.insert(id.clone(), region);
        id
    }

    /// Get live region
    pub fn get_live_region(&self, id: &str) -> Option<&LiveRegion> {
        self.live_regions.get(id)
    }

    /// Get live region mut
    pub fn get_live_region_mut(&mut self, id: &str) -> Option<&mut LiveRegion> {
        self.live_regions.get_mut(id)
    }

    /// Get all live regions
    pub fn live_regions(&self) -> Vec<&LiveRegion> {
        self.live_regions.values().collect()
    }

    /// Collect pending announcements from live regions
    pub fn collect_live_announcements(&mut self) -> Vec<String> {
        let mut announcements = Vec::new();

        for region in self.live_regions.values_mut() {
            for announcement in region.pending_announcements() {
                announcements.push(announcement.to_string());
            }
            region.mark_announced();
        }

        announcements
    }

    /// Remove element
    pub fn remove_element(&mut self, id: &str) -> Option<AccessibleElement> {
        // Get parent ID first (clone to avoid borrow conflict)
        let parent_id = self.elements.get(id).and_then(|e| e.parent.clone());

        // Remove from parent's children
        if let Some(pid) = parent_id {
            if let Some(parent) = self.elements.get_mut(&pid) {
                parent.children.retain(|child_id| child_id != id);
            }
        }

        self.elements.remove(id)
    }

    /// Get element count
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }
}

impl Default for AccessibilityTree {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Screen Reader Integration
// ============================================================================

/// Main screen reader integration
#[derive(Debug)]
pub struct ScreenReaderIntegration {
    tree: AccessibilityTree,
    focus: FocusManager,
    output: ScreenReaderOutput,
    virtual_cursor: Option<String>,
}

impl ScreenReaderIntegration {
    /// Create new integration
    pub fn new() -> Self {
        Self {
            tree: AccessibilityTree::new(),
            focus: FocusManager::new(),
            output: ScreenReaderOutput::default(),
            virtual_cursor: None,
        }
    }

    /// Create with config
    pub fn with_config(config: OutputConfig) -> Self {
        Self {
            tree: AccessibilityTree::new(),
            focus: FocusManager::new(),
            output: ScreenReaderOutput::new(config),
            virtual_cursor: None,
        }
    }

    /// Get tree
    pub fn tree(&self) -> &AccessibilityTree {
        &self.tree
    }

    /// Get tree mut
    pub fn tree_mut(&mut self) -> &mut AccessibilityTree {
        &mut self.tree
    }

    /// Get focus manager
    pub fn focus(&self) -> &FocusManager {
        &self.focus
    }

    /// Get focus manager mut
    pub fn focus_mut(&mut self) -> &mut FocusManager {
        &mut self.focus
    }

    /// Get output
    pub fn output(&self) -> &ScreenReaderOutput {
        &self.output
    }

    /// Get output mut
    pub fn output_mut(&mut self) -> &mut ScreenReaderOutput {
        &mut self.output
    }

    /// Initialize focus order from tree
    pub fn init_focus_order(&mut self) {
        let order = self.tree.focus_order();
        self.focus.set_focus_order(order);
    }

    /// Move virtual cursor to next element
    pub fn next_element(&mut self) -> Option<&AccessibleElement> {
        // Simple depth-first traversal
        let elements: Vec<_> = self.tree.elements.keys().cloned().collect();
        if elements.is_empty() {
            return None;
        }

        let next_index = match &self.virtual_cursor {
            Some(current) => elements
                .iter()
                .position(|id| id == current)
                .map(|i| (i + 1) % elements.len())
                .unwrap_or(0),
            None => 0,
        };

        self.virtual_cursor = Some(elements[next_index].clone());
        self.tree.get_element(&elements[next_index])
    }

    /// Move virtual cursor to previous element
    pub fn previous_element(&mut self) -> Option<&AccessibleElement> {
        let elements: Vec<_> = self.tree.elements.keys().cloned().collect();
        if elements.is_empty() {
            return None;
        }

        let prev_index = match &self.virtual_cursor {
            Some(current) => elements
                .iter()
                .position(|id| id == current)
                .map(|i| if i == 0 { elements.len() - 1 } else { i - 1 })
                .unwrap_or(elements.len() - 1),
            None => elements.len() - 1,
        };

        self.virtual_cursor = Some(elements[prev_index].clone());
        self.tree.get_element(&elements[prev_index])
    }

    /// Get current virtual cursor element
    pub fn current_element(&self) -> Option<&AccessibleElement> {
        self.virtual_cursor
            .as_ref()
            .and_then(|id| self.tree.get_element(id))
    }

    /// Navigate to next landmark
    pub fn next_landmark(&mut self) -> Option<&AccessibleElement> {
        let landmarks = self.tree.landmarks();
        if landmarks.is_empty() {
            return None;
        }

        let current_id = self.virtual_cursor.as_deref();
        let next_index = match current_id {
            Some(id) => landmarks
                .iter()
                .position(|l| l.id == id)
                .map(|i| (i + 1) % landmarks.len())
                .unwrap_or(0),
            None => 0,
        };

        let next = landmarks[next_index];
        self.virtual_cursor = Some(next.id.clone());
        Some(next)
    }

    /// Navigate to next heading
    pub fn next_heading(&mut self) -> Option<&AccessibleElement> {
        let headings = self.tree.find_by_role(AriaRole::Heading);
        if headings.is_empty() {
            return None;
        }

        let current_id = self.virtual_cursor.as_deref();
        let next_index = match current_id {
            Some(id) => headings
                .iter()
                .position(|h| h.id == id)
                .map(|i| (i + 1) % headings.len())
                .unwrap_or(0),
            None => 0,
        };

        let next = headings[next_index];
        self.virtual_cursor = Some(next.id.clone());
        Some(next)
    }

    /// Announce current element
    pub fn announce_current(&mut self) {
        // Clone the element to avoid borrow conflict
        let element = self.current_element().cloned();
        if let Some(ref elem) = element {
            self.output
                .queue_element(elem, AnnouncementPriority::Normal);
        }
    }

    /// Announce live region updates
    pub fn announce_live_updates(&mut self) {
        let announcements = self.tree.collect_live_announcements();
        for text in announcements {
            self.output
                .queue_announcement(text, AnnouncementPriority::High);
        }
    }

    /// Focus and announce element
    pub fn focus_and_announce(&mut self, element_id: &str) -> bool {
        if self.focus.focus(element_id.to_string()) {
            if let Some(element) = self.tree.get_element(element_id) {
                self.output
                    .queue_element(element, AnnouncementPriority::Normal);
            }
            true
        } else {
            false
        }
    }

    /// Handle Tab key
    pub fn handle_tab(&mut self, shift: bool) -> Option<&str> {
        let result = if shift {
            self.focus.focus_previous()
        } else {
            self.focus.focus_next()
        };

        if let Some(id) = result {
            if let Some(element) = self.tree.get_element(id) {
                self.output
                    .queue_element(element, AnnouncementPriority::Normal);
            }
        }

        result
    }
}

impl Default for ScreenReaderIntegration {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aria_role_name() {
        assert_eq!(AriaRole::Button.name(), "button");
        assert_eq!(AriaRole::Main.name(), "main");
        assert_eq!(AriaRole::Navigation.name(), "navigation");
    }

    #[test]
    fn test_aria_role_is_landmark() {
        assert!(AriaRole::Main.is_landmark());
        assert!(AriaRole::Navigation.is_landmark());
        assert!(AriaRole::Banner.is_landmark());
        assert!(!AriaRole::Button.is_landmark());
    }

    #[test]
    fn test_aria_role_is_widget() {
        assert!(AriaRole::Button.is_widget());
        assert!(AriaRole::Link.is_widget());
        assert!(AriaRole::Checkbox.is_widget());
        assert!(!AriaRole::Main.is_widget());
    }

    #[test]
    fn test_aria_role_is_live_region() {
        assert!(AriaRole::Alert.is_live_region());
        assert!(AriaRole::Status.is_live_region());
        assert!(AriaRole::Log.is_live_region());
        assert!(!AriaRole::Button.is_live_region());
    }

    #[test]
    fn test_aria_state_name() {
        assert_eq!(AriaState::Busy.name(), "aria-busy");
        assert_eq!(AriaState::Checked.name(), "aria-checked");
        assert_eq!(AriaState::Disabled.name(), "aria-disabled");
    }

    #[test]
    fn test_state_value_to_str() {
        assert_eq!(StateValue::True.to_str(), "true");
        assert_eq!(StateValue::False.to_str(), "false");
        assert_eq!(StateValue::Mixed.to_str(), "mixed");
        assert_eq!(StateValue::Text("custom".to_string()).to_str(), "custom");
    }

    #[test]
    fn test_accessible_element_creation() {
        let element = AccessibleElement::new(AriaRole::Button, "Click me");

        assert_eq!(element.role, AriaRole::Button);
        assert_eq!(element.content, "Click me");
        assert!(element.focusable);
    }

    #[test]
    fn test_accessible_element_builder() {
        let element = AccessibleElement::new(AriaRole::Checkbox, "Accept terms")
            .with_label("I accept the terms and conditions")
            .with_description("You must accept to continue")
            .with_state(AriaState::Checked, StateValue::False)
            .with_property(AriaProperty::Required, "true");

        assert!(element.label.is_some());
        assert!(element.description.is_some());
        assert!(element.states.contains_key(&AriaState::Checked));
        assert!(element.properties.contains_key(&AriaProperty::Required));
    }

    #[test]
    fn test_accessible_element_accessible_name() {
        let with_label = AccessibleElement::new(AriaRole::Button, "btn").with_label("Submit form");
        assert_eq!(with_label.accessible_name(), "Submit form");

        let without_label = AccessibleElement::new(AriaRole::Button, "Submit");
        assert_eq!(without_label.accessible_name(), "Submit");
    }

    #[test]
    fn test_accessible_element_is_disabled() {
        let enabled = AccessibleElement::new(AriaRole::Button, "Click");
        assert!(!enabled.is_disabled());

        let disabled = AccessibleElement::new(AriaRole::Button, "Click")
            .with_state(AriaState::Disabled, StateValue::True);
        assert!(disabled.is_disabled());
    }

    #[test]
    fn test_accessible_element_is_hidden() {
        let visible = AccessibleElement::new(AriaRole::Button, "Click");
        assert!(!visible.is_hidden());

        let hidden = AccessibleElement::new(AriaRole::Button, "Click")
            .with_state(AriaState::Hidden, StateValue::True);
        assert!(hidden.is_hidden());
    }

    #[test]
    fn test_accessible_element_announce() {
        let element = AccessibleElement::new(AriaRole::Button, "Submit")
            .with_state(AriaState::Disabled, StateValue::True);

        let announcement = element.announce();
        assert!(announcement.contains("button"));
        assert!(announcement.contains("Submit"));
        assert!(announcement.contains("disabled"));
    }

    #[test]
    fn test_live_region_creation() {
        let region = LiveRegion::polite();
        assert_eq!(region.politeness, LiveRegionPoliteness::Polite);

        let assertive = LiveRegion::assertive();
        assert_eq!(assertive.politeness, LiveRegionPoliteness::Assertive);
    }

    #[test]
    fn test_live_region_messages() {
        let mut region = LiveRegion::polite();

        region.add_message("First message");
        region.add_message("Second message");

        let pending = region.pending_announcements();
        assert_eq!(pending.len(), 2);

        region.mark_announced();
        assert!(region.pending_announcements().is_empty());
    }

    #[test]
    fn test_live_region_trim() {
        let mut region = LiveRegion::polite();

        for i in 0..10 {
            region.add_message(format!("Message {}", i));
        }

        region.trim(5);
        assert_eq!(region.messages.len(), 5);
    }

    #[test]
    fn test_focus_manager() {
        let mut manager = FocusManager::new();

        manager.add_to_focus_order("btn1");
        manager.add_to_focus_order("btn2");
        manager.add_to_focus_order("btn3");

        assert!(manager.current_focus().is_none());

        let next = manager.focus_next();
        assert_eq!(next, Some("btn1"));

        let next = manager.focus_next();
        assert_eq!(next, Some("btn2"));
    }

    #[test]
    fn test_focus_manager_previous() {
        let mut manager = FocusManager::new();

        manager.add_to_focus_order("btn1");
        manager.add_to_focus_order("btn2");
        manager.add_to_focus_order("btn3");

        manager.focus_last();
        assert_eq!(manager.current_focus(), Some("btn3"));

        manager.focus_previous();
        assert_eq!(manager.current_focus(), Some("btn2"));
    }

    #[test]
    fn test_focus_manager_first_last() {
        let mut manager = FocusManager::new();

        manager.add_to_focus_order("btn1");
        manager.add_to_focus_order("btn2");
        manager.add_to_focus_order("btn3");

        manager.focus_first();
        assert_eq!(manager.current_focus(), Some("btn1"));

        manager.focus_last();
        assert_eq!(manager.current_focus(), Some("btn3"));
    }

    #[test]
    fn test_focus_manager_return_focus() {
        let mut manager = FocusManager::new();

        manager.add_to_focus_order("btn1");
        manager.add_to_focus_order("btn2");

        manager.focus("btn1");
        manager.focus("btn2");

        let returned = manager.return_focus();
        assert_eq!(returned, Some("btn1"));
    }

    #[test]
    fn test_focus_trap() {
        let mut manager = FocusManager::new();

        manager.add_to_focus_order("main-btn");
        manager.focus("main-btn");

        let trap_id = manager.create_trap(
            "dialog",
            vec!["dialog-btn1".to_string(), "dialog-btn2".to_string()],
        );
        manager.activate_trap(&trap_id);

        assert!(manager.is_trapped());
        assert_eq!(manager.current_focus(), Some("dialog-btn1"));

        // Can't focus outside trap
        assert!(!manager.focus("main-btn"));

        manager.deactivate_trap(&trap_id);
        assert!(!manager.is_trapped());
        assert_eq!(manager.current_focus(), Some("main-btn"));
    }

    #[test]
    fn test_output_config_modes() {
        let brief = OutputConfig::brief();
        assert_eq!(brief.mode, OutputMode::Brief);
        assert!(!brief.announce_role);

        let verbose = OutputConfig::verbose();
        assert_eq!(verbose.mode, OutputMode::Verbose);
        assert!(verbose.announce_role);
    }

    #[test]
    fn test_screen_reader_output_format_element() {
        let output = ScreenReaderOutput::new(OutputConfig::verbose());

        let element = AccessibleElement::new(AriaRole::Button, "Submit")
            .with_state(AriaState::Disabled, StateValue::True);

        let formatted = output.format_element(&element);
        assert!(formatted.contains("Submit"));
        assert!(formatted.contains("button"));
        assert!(formatted.contains("disabled"));
    }

    #[test]
    fn test_screen_reader_output_queue() {
        let mut output = ScreenReaderOutput::default();

        output.queue_announcement("First", AnnouncementPriority::Low);
        output.queue_announcement("Second", AnnouncementPriority::High);
        output.queue_announcement("Third", AnnouncementPriority::Normal);

        // Should be sorted by priority
        let first = output.next_announcement().unwrap();
        assert_eq!(first.text, "Second");
        assert_eq!(first.priority, AnnouncementPriority::High);
    }

    #[test]
    fn test_screen_reader_output_interrupt() {
        let mut output = ScreenReaderOutput::default();

        output.queue_announcement("Normal", AnnouncementPriority::Normal);
        output.queue_announcement("Critical", AnnouncementPriority::Critical);

        output.interrupt();

        // Only critical should remain
        assert_eq!(output.queue_length(), 1);
        let announcement = output.next_announcement().unwrap();
        assert_eq!(announcement.priority, AnnouncementPriority::Critical);
    }

    #[test]
    fn test_format_punctuation_none() {
        let output = ScreenReaderOutput::new(OutputConfig {
            punctuation_level: PunctuationLevel::None,
            ..Default::default()
        });

        let result = output.format_punctuation("Hello, world!");
        assert!(!result.contains(','));
        assert!(!result.contains('!'));
    }

    #[test]
    fn test_format_punctuation_all() {
        let output = ScreenReaderOutput::new(OutputConfig {
            punctuation_level: PunctuationLevel::All,
            ..Default::default()
        });

        let result = output.format_punctuation("x+y");
        assert!(result.contains("plus"));
    }

    #[test]
    fn test_accessibility_tree() {
        let mut tree = AccessibilityTree::new();

        let root = AccessibleElement::new(AriaRole::Main, "Main content");
        tree.set_root(root);

        let button = AccessibleElement::new(AriaRole::Button, "Click");
        let root_id = tree.root().unwrap().id.clone();
        tree.add_child(&root_id, button);

        assert!(tree.root().is_some());
        assert_eq!(tree.element_count(), 2);
    }

    #[test]
    fn test_accessibility_tree_find_by_role() {
        let mut tree = AccessibilityTree::new();

        tree.add_element(AccessibleElement::new(AriaRole::Button, "Btn1"));
        tree.add_element(AccessibleElement::new(AriaRole::Button, "Btn2"));
        tree.add_element(AccessibleElement::new(AriaRole::Link, "Link1"));

        let buttons = tree.find_by_role(AriaRole::Button);
        assert_eq!(buttons.len(), 2);
    }

    #[test]
    fn test_accessibility_tree_landmarks() {
        let mut tree = AccessibilityTree::new();

        tree.add_element(AccessibleElement::new(AriaRole::Main, "Main"));
        tree.add_element(AccessibleElement::new(AriaRole::Navigation, "Nav"));
        tree.add_element(AccessibleElement::new(AriaRole::Button, "Btn"));

        let landmarks = tree.landmarks();
        assert_eq!(landmarks.len(), 2);
    }

    #[test]
    fn test_accessibility_tree_focusable_elements() {
        let mut tree = AccessibilityTree::new();

        tree.add_element(AccessibleElement::new(AriaRole::Button, "Btn").with_focusable(true));
        tree.add_element(AccessibleElement::new(AriaRole::Link, "Link").with_focusable(true));
        tree.add_element(
            AccessibleElement::new(AriaRole::Button, "Disabled")
                .with_state(AriaState::Disabled, StateValue::True),
        );

        let focusable = tree.focusable_elements();
        assert_eq!(focusable.len(), 2);
    }

    #[test]
    fn test_accessibility_tree_focus_order() {
        let mut tree = AccessibilityTree::new();

        tree.add_element(AccessibleElement::new(AriaRole::Button, "Btn1").with_tab_index(0));
        tree.add_element(AccessibleElement::new(AriaRole::Button, "Btn2").with_tab_index(1));
        tree.add_element(AccessibleElement::new(AriaRole::Button, "Btn3").with_tab_index(2));

        let order = tree.focus_order();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn test_accessibility_tree_live_regions() {
        let mut tree = AccessibilityTree::new();

        let mut region = LiveRegion::assertive();
        region.add_message("Alert!");

        tree.add_live_region(region);

        let announcements = tree.collect_live_announcements();
        assert_eq!(announcements.len(), 1);
        assert_eq!(announcements[0], "Alert!");
    }

    #[test]
    fn test_screen_reader_integration() {
        let mut sr = ScreenReaderIntegration::new();

        sr.tree_mut()
            .add_element(AccessibleElement::new(AriaRole::Button, "Btn1"));
        sr.tree_mut()
            .add_element(AccessibleElement::new(AriaRole::Button, "Btn2"));

        sr.init_focus_order();

        let _result = sr.handle_tab(false);
        assert!(sr.output().queue_length() > 0);
    }

    #[test]
    fn test_screen_reader_next_element() {
        let mut sr = ScreenReaderIntegration::new();

        sr.tree_mut()
            .add_element(AccessibleElement::new(AriaRole::Button, "Btn1"));
        sr.tree_mut()
            .add_element(AccessibleElement::new(AriaRole::Button, "Btn2"));

        let first = sr.next_element();
        assert!(first.is_some());
        let first_id = first.unwrap().id.clone();

        let second = sr.next_element();
        assert!(second.is_some());
        let second_id = second.unwrap().id.clone();

        assert_ne!(first_id, second_id);
    }

    #[test]
    fn test_screen_reader_next_landmark() {
        let mut sr = ScreenReaderIntegration::new();

        sr.tree_mut()
            .add_element(AccessibleElement::new(AriaRole::Main, "Main"));
        sr.tree_mut()
            .add_element(AccessibleElement::new(AriaRole::Navigation, "Nav"));
        sr.tree_mut()
            .add_element(AccessibleElement::new(AriaRole::Button, "Btn"));

        let landmark = sr.next_landmark();
        assert!(landmark.is_some());
        assert!(landmark.unwrap().role.is_landmark());
    }

    #[test]
    fn test_screen_reader_focus_and_announce() {
        let mut sr = ScreenReaderIntegration::new();

        let element = AccessibleElement::new(AriaRole::Button, "Test");
        let element_id = element.id.clone();
        sr.tree_mut().add_element(element);
        sr.focus_mut().add_to_focus_order(&element_id);

        let result = sr.focus_and_announce(&element_id);
        assert!(result);
        assert_eq!(sr.focus().current_focus(), Some(element_id.as_str()));
    }

    #[test]
    fn test_aria_property_name() {
        assert_eq!(AriaProperty::Label.name(), "aria-label");
        assert_eq!(AriaProperty::LabelledBy.name(), "aria-labelledby");
        assert_eq!(AriaProperty::DescribedBy.name(), "aria-describedby");
    }

    #[test]
    fn test_live_region_politeness_value() {
        assert_eq!(LiveRegionPoliteness::Off.value(), "off");
        assert_eq!(LiveRegionPoliteness::Polite.value(), "polite");
        assert_eq!(LiveRegionPoliteness::Assertive.value(), "assertive");
    }

    #[test]
    fn test_announcement_priority_ordering() {
        assert!(AnnouncementPriority::Critical > AnnouncementPriority::High);
        assert!(AnnouncementPriority::High > AnnouncementPriority::Normal);
        assert!(AnnouncementPriority::Normal > AnnouncementPriority::Low);
    }

    #[test]
    fn test_focus_manager_remove_from_order() {
        let mut manager = FocusManager::new();

        manager.add_to_focus_order("btn1");
        manager.add_to_focus_order("btn2");
        manager.add_to_focus_order("btn3");

        manager.remove_from_focus_order("btn2");

        manager.focus_first();
        manager.focus_next();
        assert_eq!(manager.current_focus(), Some("btn3"));
    }

    #[test]
    fn test_accessibility_tree_remove_element() {
        let mut tree = AccessibilityTree::new();

        let root = AccessibleElement::new(AriaRole::Main, "Main");
        let root_id = root.id.clone();
        tree.set_root(root);

        let child = AccessibleElement::new(AriaRole::Button, "Child");
        let child_id = tree.add_child(&root_id, child).unwrap();

        assert_eq!(tree.element_count(), 2);

        tree.remove_element(&child_id);
        assert_eq!(tree.element_count(), 1);
    }

    #[test]
    fn test_accessible_element_with_tab_index() {
        let element = AccessibleElement::new(AriaRole::Button, "Btn").with_tab_index(5);

        assert_eq!(element.tab_index, Some(5));
        assert!(element.focusable);
    }

    #[test]
    fn test_live_region_with_atomic() {
        let region = LiveRegion::polite().with_atomic(false);
        assert!(!region.atomic);
    }

    #[test]
    fn test_live_region_with_relevant() {
        let region = LiveRegion::polite().with_relevant(LiveRegionRelevant::Removals);

        assert!(region.relevant.contains(&LiveRegionRelevant::Removals));
    }

    // Additional coverage tests
    #[test]
    fn test_aria_role_all_names() {
        // Document structure roles
        assert_eq!(AriaRole::Document.name(), "document");
        assert_eq!(AriaRole::Complementary.name(), "complementary");
        assert_eq!(AriaRole::ContentInfo.name(), "contentinfo");
        assert_eq!(AriaRole::Article.name(), "article");
        assert_eq!(AriaRole::Section.name(), "section");
        assert_eq!(AriaRole::Region.name(), "region");

        // Widget roles
        assert_eq!(AriaRole::Radio.name(), "radio");
        assert_eq!(AriaRole::Textbox.name(), "textbox");
        assert_eq!(AriaRole::Listbox.name(), "listbox");
        assert_eq!(AriaRole::Option.name(), "option");
        assert_eq!(AriaRole::Combobox.name(), "combobox");
        assert_eq!(AriaRole::Slider.name(), "slider");
        assert_eq!(AriaRole::Spinbutton.name(), "spinbutton");
        assert_eq!(AriaRole::Switch.name(), "switch");
        assert_eq!(AriaRole::Tab.name(), "tab");
        assert_eq!(AriaRole::Tabpanel.name(), "tabpanel");
        assert_eq!(AriaRole::Tablist.name(), "tablist");
        assert_eq!(AriaRole::Menu.name(), "menu");
        assert_eq!(AriaRole::Menuitem.name(), "menuitem");
        assert_eq!(AriaRole::Menubar.name(), "menubar");
        assert_eq!(AriaRole::Tree.name(), "tree");
        assert_eq!(AriaRole::Treeitem.name(), "treeitem");
        assert_eq!(AriaRole::Grid.name(), "grid");
        assert_eq!(AriaRole::Gridcell.name(), "gridcell");
        assert_eq!(AriaRole::Row.name(), "row");
        assert_eq!(AriaRole::Rowgroup.name(), "rowgroup");
        assert_eq!(AriaRole::Columnheader.name(), "columnheader");
        assert_eq!(AriaRole::Rowheader.name(), "rowheader");

        // Live regions
        assert_eq!(AriaRole::Timer.name(), "timer");
        assert_eq!(AriaRole::Marquee.name(), "marquee");
        assert_eq!(AriaRole::Progressbar.name(), "progressbar");

        // Other
        assert_eq!(AriaRole::Dialog.name(), "dialog");
        assert_eq!(AriaRole::Alertdialog.name(), "alertdialog");
        assert_eq!(AriaRole::Tooltip.name(), "tooltip");
        assert_eq!(AriaRole::Form.name(), "form");
        assert_eq!(AriaRole::Search.name(), "search");
        assert_eq!(AriaRole::Separator.name(), "separator");
        assert_eq!(AriaRole::Img.name(), "img");
        assert_eq!(AriaRole::Heading.name(), "heading");
        assert_eq!(AriaRole::List.name(), "list");
        assert_eq!(AriaRole::Listitem.name(), "listitem");
        assert_eq!(AriaRole::Term.name(), "term");
        assert_eq!(AriaRole::Definition.name(), "definition");
        assert_eq!(AriaRole::Group.name(), "group");
        assert_eq!(AriaRole::Note.name(), "note");
        assert_eq!(AriaRole::Custom(123).name(), "custom");
    }

    #[test]
    fn test_aria_state_all_names() {
        assert_eq!(AriaState::Expanded.name(), "aria-expanded");
        assert_eq!(AriaState::Hidden.name(), "aria-hidden");
        assert_eq!(AriaState::Invalid.name(), "aria-invalid");
        assert_eq!(AriaState::Pressed.name(), "aria-pressed");
        assert_eq!(AriaState::Selected.name(), "aria-selected");
        assert_eq!(AriaState::Current.name(), "aria-current");
        assert_eq!(AriaState::Grabbed.name(), "aria-grabbed");
    }

    #[test]
    fn test_aria_property_all_names() {
        assert_eq!(AriaProperty::Controls.name(), "aria-controls");
        assert_eq!(AriaProperty::Owns.name(), "aria-owns");
        assert_eq!(AriaProperty::FlowTo.name(), "aria-flowto");
        assert_eq!(
            AriaProperty::ActiveDescendant.name(),
            "aria-activedescendant"
        );
        assert_eq!(AriaProperty::Atomic.name(), "aria-atomic");
        assert_eq!(AriaProperty::Autocomplete.name(), "aria-autocomplete");
        assert_eq!(AriaProperty::Colcount.name(), "aria-colcount");
        assert_eq!(AriaProperty::Colindex.name(), "aria-colindex");
        assert_eq!(AriaProperty::Colspan.name(), "aria-colspan");
        assert_eq!(AriaProperty::Haspopup.name(), "aria-haspopup");
        assert_eq!(AriaProperty::Keyshortcuts.name(), "aria-keyshortcuts");
        assert_eq!(AriaProperty::Level.name(), "aria-level");
        assert_eq!(AriaProperty::Live.name(), "aria-live");
        assert_eq!(AriaProperty::Modal.name(), "aria-modal");
        assert_eq!(AriaProperty::Multiline.name(), "aria-multiline");
        assert_eq!(AriaProperty::Multiselectable.name(), "aria-multiselectable");
        assert_eq!(AriaProperty::Orientation.name(), "aria-orientation");
        assert_eq!(AriaProperty::Placeholder.name(), "aria-placeholder");
        assert_eq!(AriaProperty::Posinset.name(), "aria-posinset");
        assert_eq!(AriaProperty::Readonly.name(), "aria-readonly");
        assert_eq!(AriaProperty::Relevant.name(), "aria-relevant");
        assert_eq!(AriaProperty::Required.name(), "aria-required");
        assert_eq!(AriaProperty::Roledescription.name(), "aria-roledescription");
        assert_eq!(AriaProperty::Rowcount.name(), "aria-rowcount");
        assert_eq!(AriaProperty::Rowindex.name(), "aria-rowindex");
        assert_eq!(AriaProperty::Rowspan.name(), "aria-rowspan");
        assert_eq!(AriaProperty::Setsize.name(), "aria-setsize");
        assert_eq!(AriaProperty::Sort.name(), "aria-sort");
        assert_eq!(AriaProperty::Valuemax.name(), "aria-valuemax");
        assert_eq!(AriaProperty::Valuemin.name(), "aria-valuemin");
        assert_eq!(AriaProperty::Valuenow.name(), "aria-valuenow");
        assert_eq!(AriaProperty::Valuetext.name(), "aria-valuetext");
    }

    #[test]
    fn test_screen_reader_output_queue_element() {
        let mut output = ScreenReaderOutput::default();
        let element = AccessibleElement::new(AriaRole::Button, "Test Button");

        output.queue_element(&element, AnnouncementPriority::Normal);
        assert_eq!(output.queue_length(), 1);
    }

    #[test]
    fn test_screen_reader_output_peek_announcement() {
        let mut output = ScreenReaderOutput::default();
        output.queue_announcement("Test", AnnouncementPriority::Normal);

        let peeked = output.peek_announcement();
        assert!(peeked.is_some());
        assert_eq!(peeked.unwrap().text, "Test");

        // Peek should not remove
        assert_eq!(output.queue_length(), 1);
    }

    #[test]
    fn test_screen_reader_output_clear_queue() {
        let mut output = ScreenReaderOutput::default();
        output.queue_announcement("One", AnnouncementPriority::Normal);
        output.queue_announcement("Two", AnnouncementPriority::High);

        output.clear_queue();
        assert_eq!(output.queue_length(), 0);
    }

    #[test]
    fn test_format_punctuation_some() {
        let output = ScreenReaderOutput::new(OutputConfig {
            punctuation_level: PunctuationLevel::Some,
            ..Default::default()
        });

        let result = output.format_punctuation("Hello. World!");
        assert!(result.contains("period"));
        assert!(result.contains("exclamation"));
    }

    #[test]
    fn test_format_punctuation_most() {
        let output = ScreenReaderOutput::new(OutputConfig {
            punctuation_level: PunctuationLevel::Most,
            ..Default::default()
        });

        let result = output.format_punctuation("Hello, world: test;");
        assert!(result.contains("comma"));
        assert!(result.contains("colon"));
        assert!(result.contains("semicolon"));
    }

    #[test]
    fn test_accessible_element_add_child() {
        let mut element = AccessibleElement::new(AriaRole::Menu, "Menu");
        element.add_child("child-1");
        element.add_child("child-2");

        assert_eq!(element.children.len(), 2);
        assert!(element.children.contains(&"child-1".to_string()));
    }

    #[test]
    fn test_accessible_element_is_expanded() {
        let collapsed = AccessibleElement::new(AriaRole::Menu, "Menu")
            .with_state(AriaState::Expanded, StateValue::False);
        assert!(!collapsed.is_expanded());

        let expanded = AccessibleElement::new(AriaRole::Menu, "Menu")
            .with_state(AriaState::Expanded, StateValue::True);
        assert!(expanded.is_expanded());
    }

    #[test]
    fn test_accessible_element_announce_with_label() {
        let element = AccessibleElement::new(AriaRole::Main, "content")
            .with_label("Main Content")
            .with_description("Primary content area");

        let announcement = element.announce();
        assert!(announcement.contains("Main Content"));
        assert!(announcement.contains("landmark"));
    }

    #[test]
    fn test_accessible_element_announce_checked_mixed() {
        let element = AccessibleElement::new(AriaRole::Checkbox, "Select All")
            .with_state(AriaState::Checked, StateValue::Mixed);

        let announcement = element.announce();
        assert!(announcement.contains("partially checked"));
    }

    #[test]
    fn test_accessible_element_announce_pressed() {
        let element = AccessibleElement::new(AriaRole::Button, "Toggle")
            .with_state(AriaState::Pressed, StateValue::True);

        let announcement = element.announce();
        assert!(announcement.contains("pressed"));
    }

    #[test]
    fn test_focus_manager_set_focus_order() {
        let mut manager = FocusManager::new();
        manager.set_focus_order(vec!["a".to_string(), "b".to_string(), "c".to_string()]);

        manager.focus_first();
        assert_eq!(manager.current_focus(), Some("a"));
    }

    #[test]
    fn test_focus_manager_focus_wrap_around() {
        let mut manager = FocusManager::new();
        manager.add_to_focus_order("btn1");
        manager.add_to_focus_order("btn2");

        manager.focus_last();
        assert_eq!(manager.current_focus(), Some("btn2"));

        manager.focus_next(); // Should wrap to first
        assert_eq!(manager.current_focus(), Some("btn1"));

        manager.focus_previous(); // Should wrap to last
        assert_eq!(manager.current_focus(), Some("btn2"));
    }

    #[test]
    fn test_focus_manager_empty_operations() {
        let mut manager = FocusManager::new();

        assert!(manager.focus_next().is_none());
        assert!(manager.focus_previous().is_none());
        assert!(manager.focus_first().is_none());
        assert!(manager.focus_last().is_none());
        assert!(manager.return_focus().is_none());
    }

    #[test]
    fn test_focus_trap_deactivate_nonexistent() {
        let mut manager = FocusManager::new();
        assert!(!manager.deactivate_trap("nonexistent"));
    }

    #[test]
    fn test_focus_trap_activate_nonexistent() {
        let mut manager = FocusManager::new();
        assert!(!manager.activate_trap("nonexistent"));
    }

    #[test]
    fn test_focus_manager_default() {
        let manager = FocusManager::default();
        assert!(manager.current_focus().is_none());
    }

    #[test]
    fn test_accessibility_tree_children() {
        let mut tree = AccessibilityTree::new();

        let root = AccessibleElement::new(AriaRole::Main, "Main");
        let root_id = root.id.clone();
        tree.set_root(root);

        tree.add_child(&root_id, AccessibleElement::new(AriaRole::Button, "Btn1"));
        tree.add_child(&root_id, AccessibleElement::new(AriaRole::Button, "Btn2"));

        let children = tree.children(&root_id);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_accessibility_tree_get_element_mut() {
        let mut tree = AccessibilityTree::new();

        let element = AccessibleElement::new(AriaRole::Button, "Click");
        let id = tree.add_element(element);

        if let Some(elem) = tree.get_element_mut(&id) {
            elem.label = Some("Updated".to_string());
        }

        assert_eq!(
            tree.get_element(&id).unwrap().label.as_deref(),
            Some("Updated")
        );
    }

    #[test]
    fn test_accessibility_tree_add_child_nonexistent_parent() {
        let mut tree = AccessibilityTree::new();

        let child = AccessibleElement::new(AriaRole::Button, "Child");
        let result = tree.add_child("nonexistent", child);

        assert!(result.is_none());
    }

    #[test]
    fn test_live_region_relevant_all() {
        let region = LiveRegion::polite().with_relevant(LiveRegionRelevant::All);

        assert!(region.relevant.contains(&LiveRegionRelevant::All));
    }

    #[test]
    fn test_live_region_relevant_duplicate() {
        let region = LiveRegion::polite()
            .with_relevant(LiveRegionRelevant::Additions)
            .with_relevant(LiveRegionRelevant::Additions); // Duplicate

        // Should only have Additions once plus default Text
        let additions_count = region
            .relevant
            .iter()
            .filter(|r| **r == LiveRegionRelevant::Additions)
            .count();
        assert_eq!(additions_count, 1);
    }

    #[test]
    fn test_output_mode_custom() {
        let config = OutputConfig {
            mode: OutputMode::Custom,
            ..Default::default()
        };
        assert_eq!(config.mode, OutputMode::Custom);
    }

    #[test]
    fn test_announcement_interruptible() {
        let mut output = ScreenReaderOutput::default();

        // Low priority is interruptible
        output.queue_announcement("Low", AnnouncementPriority::Low);
        let low = output.next_announcement().unwrap();
        assert!(low.interruptible);

        // Critical is not interruptible
        output.queue_announcement("Critical", AnnouncementPriority::Critical);
        let critical = output.next_announcement().unwrap();
        assert!(!critical.interruptible);
    }

    #[test]
    fn test_format_element_with_shortcuts() {
        let output = ScreenReaderOutput::new(OutputConfig {
            announce_shortcuts: true,
            ..Default::default()
        });

        let element = AccessibleElement::new(AriaRole::Button, "Save")
            .with_property(AriaProperty::Keyshortcuts, "Ctrl+S");

        let formatted = output.format_element(&element);
        assert!(formatted.contains("shortcut"));
        assert!(formatted.contains("Ctrl+S"));
    }

    #[test]
    fn test_format_element_collapsed() {
        let output = ScreenReaderOutput::new(OutputConfig::verbose());

        let element = AccessibleElement::new(AriaRole::Menu, "Options")
            .with_state(AriaState::Expanded, StateValue::False);

        let formatted = output.format_element(&element);
        assert!(formatted.contains("collapsed"));
    }

    #[test]
    fn test_format_element_checked_states() {
        let output = ScreenReaderOutput::new(OutputConfig::verbose());

        let checked = AccessibleElement::new(AriaRole::Checkbox, "A")
            .with_state(AriaState::Checked, StateValue::True);
        assert!(output.format_element(&checked).contains("checked"));

        let unchecked = AccessibleElement::new(AriaRole::Checkbox, "B")
            .with_state(AriaState::Checked, StateValue::False);
        assert!(output.format_element(&unchecked).contains("not checked"));
    }

    #[test]
    fn test_accessibility_tree_default() {
        let tree = AccessibilityTree::default();
        assert!(tree.root().is_none());
        assert_eq!(tree.element_count(), 0);
    }

    #[test]
    fn test_focus_trap_fields() {
        let trap = FocusTrap {
            id: "trap-1".to_string(),
            container_id: "dialog".to_string(),
            focusable_elements: vec!["btn1".to_string(), "btn2".to_string()],
            return_focus_to: Some("main-btn".to_string()),
            active: false,
        };

        assert_eq!(trap.id, "trap-1");
        assert_eq!(trap.focusable_elements.len(), 2);
        assert!(!trap.active);
    }

    #[test]
    fn test_live_message_fields() {
        let msg = LiveMessage {
            content: "Hello".to_string(),
            timestamp: SystemTime::now(),
            announced: false,
        };

        assert_eq!(msg.content, "Hello");
        assert!(!msg.announced);
    }

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert_eq!(config.mode, OutputMode::Verbose);
        assert!(config.announce_role);
        assert!(config.announce_state);
        assert!(config.announce_description);
        assert!(!config.announce_shortcuts);
        assert_eq!(config.punctuation_level, PunctuationLevel::Some);
        assert!((config.speech_rate - 1.0).abs() < f32::EPSILON);
        assert!((config.pitch - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_generate_element_id_unique() {
        let id1 = generate_element_id();
        let id2 = generate_element_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("elem-"));
    }

    #[test]
    fn test_generate_region_id_unique() {
        let id1 = generate_region_id();
        let id2 = generate_region_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("region-"));
    }
}
