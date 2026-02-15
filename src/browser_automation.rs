//! Browser Automation Tool
//!
//! Headless browser control features:
//! - Web scraping
//! - UI testing automation
//! - Screenshot capture
//! - Form filling
//! - API exploration

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global counters for unique IDs
static SESSION_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static ELEMENT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_session_id() -> String {
    format!(
        "browser_{}_{:x}",
        SESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_element_id() -> String {
    format!(
        "elem_{}_{:x}",
        ELEMENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_test_id() -> String {
    format!(
        "test_{}_{:x}",
        TEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================================
// Browser Configuration
// ============================================================================

/// Browser type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserType {
    #[default]
    Chrome,
    Firefox,
    Safari,
    Edge,
    Webkit,
}

impl BrowserType {
    pub fn as_str(&self) -> &str {
        match self {
            BrowserType::Chrome => "chrome",
            BrowserType::Firefox => "firefox",
            BrowserType::Safari => "safari",
            BrowserType::Edge => "edge",
            BrowserType::Webkit => "webkit",
        }
    }
}

/// Viewport size
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Desktop viewport (1920x1080)
    pub fn desktop() -> Self {
        Self::new(1920, 1080)
    }

    /// Laptop viewport (1366x768)
    pub fn laptop() -> Self {
        Self::new(1366, 768)
    }

    /// Tablet viewport (768x1024)
    pub fn tablet() -> Self {
        Self::new(768, 1024)
    }

    /// Mobile viewport (375x667)
    pub fn mobile() -> Self {
        Self::new(375, 667)
    }

    /// Mobile landscape viewport (667x375)
    pub fn mobile_landscape() -> Self {
        Self::new(667, 375)
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::desktop()
    }
}

/// Browser configuration
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Browser type
    pub browser_type: BrowserType,
    /// Run in headless mode
    pub headless: bool,
    /// Viewport size
    pub viewport: Viewport,
    /// User agent string
    pub user_agent: Option<String>,
    /// Default timeout in milliseconds
    pub timeout_ms: u64,
    /// Block images
    pub block_images: bool,
    /// Block JavaScript
    pub block_javascript: bool,
    /// Accept cookies
    pub accept_cookies: bool,
    /// Proxy settings
    pub proxy: Option<String>,
    /// Extra HTTP headers
    pub extra_headers: HashMap<String, String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            browser_type: BrowserType::default(),
            headless: true,
            viewport: Viewport::default(),
            user_agent: None,
            timeout_ms: 30000,
            block_images: false,
            block_javascript: false,
            accept_cookies: true,
            proxy: None,
            extra_headers: HashMap::new(),
        }
    }
}

impl BrowserConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set browser type
    pub fn with_browser(mut self, browser: BrowserType) -> Self {
        self.browser_type = browser;
        self
    }

    /// Builder: set headless mode
    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    /// Builder: set viewport
    pub fn with_viewport(mut self, viewport: Viewport) -> Self {
        self.viewport = viewport;
        self
    }

    /// Builder: set user agent
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Builder: set timeout
    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// Builder: block images
    pub fn block_images(mut self) -> Self {
        self.block_images = true;
        self
    }

    /// Builder: block javascript
    pub fn block_javascript(mut self) -> Self {
        self.block_javascript = true;
        self
    }

    /// Builder: set proxy
    pub fn with_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = Some(proxy.into());
        self
    }

    /// Builder: add header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// Page Elements
// ============================================================================

/// Element selector type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorType {
    Css,
    XPath,
    Id,
    Name,
    ClassName,
    TagName,
    LinkText,
    PartialLinkText,
}

impl SelectorType {
    pub fn as_str(&self) -> &str {
        match self {
            SelectorType::Css => "css",
            SelectorType::XPath => "xpath",
            SelectorType::Id => "id",
            SelectorType::Name => "name",
            SelectorType::ClassName => "class",
            SelectorType::TagName => "tag",
            SelectorType::LinkText => "link_text",
            SelectorType::PartialLinkText => "partial_link_text",
        }
    }
}

/// Element locator
#[derive(Debug, Clone)]
pub struct Locator {
    pub selector_type: SelectorType,
    pub value: String,
}

impl Locator {
    pub fn new(selector_type: SelectorType, value: impl Into<String>) -> Self {
        Self {
            selector_type,
            value: value.into(),
        }
    }

    /// Create CSS selector
    pub fn css(selector: impl Into<String>) -> Self {
        Self::new(SelectorType::Css, selector)
    }

    /// Create XPath selector
    pub fn xpath(path: impl Into<String>) -> Self {
        Self::new(SelectorType::XPath, path)
    }

    /// Create ID selector
    pub fn id(id: impl Into<String>) -> Self {
        Self::new(SelectorType::Id, id)
    }

    /// Create name selector
    pub fn name(name: impl Into<String>) -> Self {
        Self::new(SelectorType::Name, name)
    }

    /// Create class name selector
    pub fn class(class: impl Into<String>) -> Self {
        Self::new(SelectorType::ClassName, class)
    }

    /// Create tag name selector
    pub fn tag(tag: impl Into<String>) -> Self {
        Self::new(SelectorType::TagName, tag)
    }
}

/// Element bounding box
#[derive(Debug, Clone, Copy)]
pub struct ElementBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ElementBounds {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// A page element
#[derive(Debug, Clone)]
pub struct PageElement {
    /// Internal ID
    pub id: String,
    /// Tag name
    pub tag_name: String,
    /// Text content
    pub text: Option<String>,
    /// Inner HTML
    pub inner_html: Option<String>,
    /// Attributes
    pub attributes: HashMap<String, String>,
    /// Bounding box
    pub bounds: Option<ElementBounds>,
    /// Is visible
    pub visible: bool,
    /// Is enabled
    pub enabled: bool,
}

impl PageElement {
    pub fn new(tag_name: impl Into<String>) -> Self {
        Self {
            id: generate_element_id(),
            tag_name: tag_name.into(),
            text: None,
            inner_html: None,
            attributes: HashMap::new(),
            bounds: None,
            visible: true,
            enabled: true,
        }
    }

    /// Builder: set text
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Builder: set inner HTML
    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.inner_html = Some(html.into());
        self
    }

    /// Builder: add attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Builder: set bounds
    pub fn with_bounds(mut self, bounds: ElementBounds) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Builder: set visibility
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Builder: set enabled
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Get attribute value
    pub fn get_attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }

    /// Check if element has attribute
    pub fn has_attribute(&self, key: &str) -> bool {
        self.attributes.contains_key(key)
    }

    /// Check if element is a form input
    pub fn is_input(&self) -> bool {
        matches!(
            self.tag_name.to_lowercase().as_str(),
            "input" | "textarea" | "select"
        )
    }

    /// Check if element is a button
    pub fn is_button(&self) -> bool {
        self.tag_name.to_lowercase() == "button"
            || (self.tag_name.to_lowercase() == "input"
                && self
                    .get_attribute("type")
                    .map(|t| t == "submit" || t == "button")
                    .unwrap_or(false))
    }

    /// Check if element is a link
    pub fn is_link(&self) -> bool {
        self.tag_name.to_lowercase() == "a"
    }
}

// ============================================================================
// Browser Actions
// ============================================================================

/// Mouse button
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

impl MouseButton {
    pub fn as_str(&self) -> &str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
        }
    }
}

/// Keyboard modifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyModifier {
    Shift,
    Control,
    Alt,
    Meta,
}

impl KeyModifier {
    pub fn as_str(&self) -> &str {
        match self {
            KeyModifier::Shift => "Shift",
            KeyModifier::Control => "Control",
            KeyModifier::Alt => "Alt",
            KeyModifier::Meta => "Meta",
        }
    }
}

/// Browser action type
#[derive(Debug, Clone)]
pub enum BrowserAction {
    /// Navigate to URL
    Navigate(String),
    /// Click on element
    Click(Locator),
    /// Double click on element
    DoubleClick(Locator),
    /// Right click on element
    RightClick(Locator),
    /// Hover over element
    Hover(Locator),
    /// Type text into element
    Type { locator: Locator, text: String },
    /// Clear element
    Clear(Locator),
    /// Select option in dropdown
    Select { locator: Locator, value: String },
    /// Press key
    PressKey(String),
    /// Take screenshot
    Screenshot(Option<String>),
    /// Wait for element
    WaitForElement { locator: Locator, timeout_ms: u64 },
    /// Wait for navigation
    WaitForNavigation(u64),
    /// Wait for time
    Wait(u64),
    /// Scroll to element
    ScrollTo(Locator),
    /// Execute JavaScript
    ExecuteScript(String),
    /// Go back
    GoBack,
    /// Go forward
    GoForward,
    /// Refresh page
    Refresh,
}

impl BrowserAction {
    pub fn action_type(&self) -> &str {
        match self {
            BrowserAction::Navigate(_) => "navigate",
            BrowserAction::Click(_) => "click",
            BrowserAction::DoubleClick(_) => "double_click",
            BrowserAction::RightClick(_) => "right_click",
            BrowserAction::Hover(_) => "hover",
            BrowserAction::Type { .. } => "type",
            BrowserAction::Clear(_) => "clear",
            BrowserAction::Select { .. } => "select",
            BrowserAction::PressKey(_) => "press_key",
            BrowserAction::Screenshot(_) => "screenshot",
            BrowserAction::WaitForElement { .. } => "wait_for_element",
            BrowserAction::WaitForNavigation(_) => "wait_for_navigation",
            BrowserAction::Wait(_) => "wait",
            BrowserAction::ScrollTo(_) => "scroll_to",
            BrowserAction::ExecuteScript(_) => "execute_script",
            BrowserAction::GoBack => "go_back",
            BrowserAction::GoForward => "go_forward",
            BrowserAction::Refresh => "refresh",
        }
    }
}

/// Action result
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub success: bool,
    pub action_type: String,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub screenshot_path: Option<String>,
    pub extracted_data: Option<String>,
}

impl ActionResult {
    pub fn success(action_type: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            success: true,
            action_type: action_type.into(),
            duration_ms,
            error: None,
            screenshot_path: None,
            extracted_data: None,
        }
    }

    pub fn failure(action_type: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            success: false,
            action_type: action_type.into(),
            duration_ms: 0,
            error: Some(error.into()),
            screenshot_path: None,
            extracted_data: None,
        }
    }

    pub fn with_screenshot(mut self, path: impl Into<String>) -> Self {
        self.screenshot_path = Some(path.into());
        self
    }

    pub fn with_data(mut self, data: impl Into<String>) -> Self {
        self.extracted_data = Some(data.into());
        self
    }
}

// ============================================================================
// Form Handling
// ============================================================================

/// Form field type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormFieldType {
    Text,
    Password,
    Email,
    Number,
    Tel,
    Url,
    Search,
    Date,
    Time,
    Checkbox,
    Radio,
    Select,
    Textarea,
    File,
    Hidden,
}

impl FormFieldType {
    pub fn as_str(&self) -> &str {
        match self {
            FormFieldType::Text => "text",
            FormFieldType::Password => "password",
            FormFieldType::Email => "email",
            FormFieldType::Number => "number",
            FormFieldType::Tel => "tel",
            FormFieldType::Url => "url",
            FormFieldType::Search => "search",
            FormFieldType::Date => "date",
            FormFieldType::Time => "time",
            FormFieldType::Checkbox => "checkbox",
            FormFieldType::Radio => "radio",
            FormFieldType::Select => "select",
            FormFieldType::Textarea => "textarea",
            FormFieldType::File => "file",
            FormFieldType::Hidden => "hidden",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "password" => FormFieldType::Password,
            "email" => FormFieldType::Email,
            "number" => FormFieldType::Number,
            "tel" => FormFieldType::Tel,
            "url" => FormFieldType::Url,
            "search" => FormFieldType::Search,
            "date" => FormFieldType::Date,
            "time" => FormFieldType::Time,
            "checkbox" => FormFieldType::Checkbox,
            "radio" => FormFieldType::Radio,
            "select" | "select-one" | "select-multiple" => FormFieldType::Select,
            "textarea" => FormFieldType::Textarea,
            "file" => FormFieldType::File,
            "hidden" => FormFieldType::Hidden,
            _ => FormFieldType::Text,
        }
    }
}

/// Form field
#[derive(Debug, Clone)]
pub struct FormField {
    pub name: String,
    pub field_type: FormFieldType,
    pub label: Option<String>,
    pub value: Option<String>,
    pub required: bool,
    pub options: Vec<String>,
    pub locator: Locator,
}

impl FormField {
    pub fn new(name: impl Into<String>, field_type: FormFieldType, locator: Locator) -> Self {
        Self {
            name: name.into(),
            field_type,
            label: None,
            value: None,
            required: false,
            options: Vec::new(),
            locator,
        }
    }

    /// Builder: set label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Builder: set value
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Builder: set required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Builder: add option
    pub fn with_option(mut self, option: impl Into<String>) -> Self {
        self.options.push(option.into());
        self
    }
}

/// Detected form
#[derive(Debug, Clone)]
pub struct DetectedForm {
    pub id: Option<String>,
    pub name: Option<String>,
    pub action: Option<String>,
    pub method: String,
    pub fields: Vec<FormField>,
}

impl DetectedForm {
    pub fn new() -> Self {
        Self {
            id: None,
            name: None,
            action: None,
            method: "GET".to_string(),
            fields: Vec::new(),
        }
    }

    /// Builder: set ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Builder: set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Builder: set action
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Builder: set method
    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = method.into();
        self
    }

    /// Builder: add field
    pub fn with_field(mut self, field: FormField) -> Self {
        self.fields.push(field);
        self
    }

    /// Get required fields
    pub fn required_fields(&self) -> Vec<&FormField> {
        self.fields.iter().filter(|f| f.required).collect()
    }

    /// Get field by name
    pub fn get_field(&self, name: &str) -> Option<&FormField> {
        self.fields.iter().find(|f| f.name == name)
    }
}

impl Default for DetectedForm {
    fn default() -> Self {
        Self::new()
    }
}

/// Form filler
#[derive(Debug, Default)]
pub struct FormFiller {
    values: HashMap<String, String>,
}

impl FormFiller {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a field value
    pub fn set(&mut self, field_name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(field_name.into(), value.into());
    }

    /// Get a field value
    pub fn get(&self, field_name: &str) -> Option<&str> {
        self.values.get(field_name).map(|s| s.as_str())
    }

    /// Generate actions to fill a form
    pub fn fill_actions(&self, form: &DetectedForm) -> Vec<BrowserAction> {
        let mut actions = Vec::new();

        for field in &form.fields {
            if let Some(value) = self.values.get(&field.name) {
                match field.field_type {
                    FormFieldType::Checkbox | FormFieldType::Radio => {
                        if value == "true" || value == "1" || value == "on" {
                            actions.push(BrowserAction::Click(field.locator.clone()));
                        }
                    }
                    FormFieldType::Select => {
                        actions.push(BrowserAction::Select {
                            locator: field.locator.clone(),
                            value: value.clone(),
                        });
                    }
                    _ => {
                        actions.push(BrowserAction::Clear(field.locator.clone()));
                        actions.push(BrowserAction::Type {
                            locator: field.locator.clone(),
                            text: value.clone(),
                        });
                    }
                }
            }
        }

        actions
    }
}

// ============================================================================
// Web Scraping
// ============================================================================

/// Scraping target
#[derive(Debug, Clone)]
pub struct ScrapeTarget {
    pub name: String,
    pub locator: Locator,
    pub extract: ExtractType,
    pub multiple: bool,
}

/// Extraction type
#[derive(Debug, Clone)]
pub enum ExtractType {
    Text,
    Html,
    Attribute(String),
    Link,
    Image,
}

impl ScrapeTarget {
    pub fn new(name: impl Into<String>, locator: Locator, extract: ExtractType) -> Self {
        Self {
            name: name.into(),
            locator,
            extract,
            multiple: false,
        }
    }

    /// Extract multiple elements
    pub fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }
}

/// Scraped data
#[derive(Debug, Clone)]
pub struct ScrapedData {
    pub name: String,
    pub values: Vec<String>,
}

impl ScrapedData {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            values: Vec::new(),
        }
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.values.push(value.into());
        self
    }

    pub fn first(&self) -> Option<&str> {
        self.values.first().map(|s| s.as_str())
    }

    pub fn count(&self) -> usize {
        self.values.len()
    }
}

/// Scraping result
#[derive(Debug, Clone)]
pub struct ScrapeResult {
    pub url: String,
    pub title: Option<String>,
    pub data: HashMap<String, ScrapedData>,
    pub scraped_at: u64,
}

impl ScrapeResult {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            title: None,
            data: HashMap::new(),
            scraped_at: current_timestamp(),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn add_data(&mut self, data: ScrapedData) {
        self.data.insert(data.name.clone(), data);
    }

    pub fn get(&self, name: &str) -> Option<&ScrapedData> {
        self.data.get(name)
    }
}

// ============================================================================
// UI Testing
// ============================================================================

/// Test assertion type
#[derive(Debug, Clone)]
pub enum Assertion {
    /// Element exists
    ElementExists(Locator),
    /// Element does not exist
    ElementNotExists(Locator),
    /// Element is visible
    ElementVisible(Locator),
    /// Element is hidden
    ElementHidden(Locator),
    /// Element has text
    ElementHasText { locator: Locator, text: String },
    /// Element contains text
    ElementContainsText { locator: Locator, text: String },
    /// Element has attribute
    ElementHasAttribute {
        locator: Locator,
        attribute: String,
        value: String,
    },
    /// Page title equals
    TitleEquals(String),
    /// Page title contains
    TitleContains(String),
    /// URL equals
    UrlEquals(String),
    /// URL contains
    UrlContains(String),
    /// Element count
    ElementCount { locator: Locator, expected: usize },
}

impl Assertion {
    pub fn assertion_type(&self) -> &str {
        match self {
            Assertion::ElementExists(_) => "element_exists",
            Assertion::ElementNotExists(_) => "element_not_exists",
            Assertion::ElementVisible(_) => "element_visible",
            Assertion::ElementHidden(_) => "element_hidden",
            Assertion::ElementHasText { .. } => "element_has_text",
            Assertion::ElementContainsText { .. } => "element_contains_text",
            Assertion::ElementHasAttribute { .. } => "element_has_attribute",
            Assertion::TitleEquals(_) => "title_equals",
            Assertion::TitleContains(_) => "title_contains",
            Assertion::UrlEquals(_) => "url_equals",
            Assertion::UrlContains(_) => "url_contains",
            Assertion::ElementCount { .. } => "element_count",
        }
    }
}

/// Test step
#[derive(Debug, Clone)]
pub struct TestStep {
    pub name: String,
    pub action: Option<BrowserAction>,
    pub assertion: Option<Assertion>,
    pub screenshot_on_failure: bool,
}

impl TestStep {
    pub fn action(name: impl Into<String>, action: BrowserAction) -> Self {
        Self {
            name: name.into(),
            action: Some(action),
            assertion: None,
            screenshot_on_failure: false,
        }
    }

    pub fn assertion(name: impl Into<String>, assertion: Assertion) -> Self {
        Self {
            name: name.into(),
            action: None,
            assertion: Some(assertion),
            screenshot_on_failure: false,
        }
    }

    pub fn with_screenshot_on_failure(mut self) -> Self {
        self.screenshot_on_failure = true;
        self
    }
}

/// Test case
#[derive(Debug, Clone)]
pub struct TestCase {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<TestStep>,
    pub setup_steps: Vec<TestStep>,
    pub teardown_steps: Vec<TestStep>,
    pub tags: Vec<String>,
}

impl TestCase {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: generate_test_id(),
            name: name.into(),
            description: None,
            steps: Vec::new(),
            setup_steps: Vec::new(),
            teardown_steps: Vec::new(),
            tags: Vec::new(),
        }
    }

    /// Builder: set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Builder: add step
    pub fn step(mut self, step: TestStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Builder: add setup step
    pub fn setup(mut self, step: TestStep) -> Self {
        self.setup_steps.push(step);
        self
    }

    /// Builder: add teardown step
    pub fn teardown(mut self, step: TestStep) -> Self {
        self.teardown_steps.push(step);
        self
    }

    /// Builder: add tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

/// Test result status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

impl TestStatus {
    pub fn as_str(&self) -> &str {
        match self {
            TestStatus::Passed => "passed",
            TestStatus::Failed => "failed",
            TestStatus::Skipped => "skipped",
            TestStatus::Error => "error",
        }
    }
}

/// Step result
#[derive(Debug, Clone)]
pub struct StepResult {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub screenshot: Option<String>,
}

impl StepResult {
    pub fn passed(name: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Passed,
            duration_ms,
            error: None,
            screenshot: None,
        }
    }

    pub fn failed(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: TestStatus::Failed,
            duration_ms: 0,
            error: Some(error.into()),
            screenshot: None,
        }
    }

    pub fn with_screenshot(mut self, path: impl Into<String>) -> Self {
        self.screenshot = Some(path.into());
        self
    }
}

/// Test run result
#[derive(Debug, Clone)]
pub struct TestRunResult {
    pub test_id: String,
    pub test_name: String,
    pub status: TestStatus,
    pub step_results: Vec<StepResult>,
    pub total_duration_ms: u64,
    pub started_at: u64,
    pub finished_at: u64,
}

impl TestRunResult {
    pub fn new(test_id: impl Into<String>, test_name: impl Into<String>) -> Self {
        Self {
            test_id: test_id.into(),
            test_name: test_name.into(),
            status: TestStatus::Passed,
            step_results: Vec::new(),
            total_duration_ms: 0,
            started_at: current_timestamp(),
            finished_at: current_timestamp(),
        }
    }

    pub fn add_step_result(&mut self, result: StepResult) {
        if result.status == TestStatus::Failed || result.status == TestStatus::Error {
            self.status = result.status;
        }
        self.total_duration_ms += result.duration_ms;
        self.step_results.push(result);
    }

    pub fn finish(&mut self) {
        self.finished_at = current_timestamp();
    }

    pub fn passed_steps(&self) -> usize {
        self.step_results
            .iter()
            .filter(|r| r.status == TestStatus::Passed)
            .count()
    }

    pub fn failed_steps(&self) -> usize {
        self.step_results
            .iter()
            .filter(|r| r.status == TestStatus::Failed || r.status == TestStatus::Error)
            .count()
    }
}

// ============================================================================
// Browser Session
// ============================================================================

/// Browser session
#[derive(Debug)]
pub struct BrowserSession {
    pub id: String,
    pub config: BrowserConfig,
    pub current_url: Option<String>,
    pub page_title: Option<String>,
    pub cookies: HashMap<String, String>,
    pub action_history: Vec<BrowserAction>,
    pub started_at: u64,
    pub is_open: bool,
}

impl BrowserSession {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            id: generate_session_id(),
            config,
            current_url: None,
            page_title: None,
            cookies: HashMap::new(),
            action_history: Vec::new(),
            started_at: current_timestamp(),
            is_open: true,
        }
    }

    /// Record an action
    pub fn record_action(&mut self, action: BrowserAction) {
        self.action_history.push(action);
    }

    /// Set current URL
    pub fn set_url(&mut self, url: impl Into<String>) {
        self.current_url = Some(url.into());
    }

    /// Set page title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.page_title = Some(title.into());
    }

    /// Add cookie
    pub fn add_cookie(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.cookies.insert(name.into(), value.into());
    }

    /// Get cookie
    pub fn get_cookie(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(|s| s.as_str())
    }

    /// Close session
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Get action count
    pub fn action_count(&self) -> usize {
        self.action_history.len()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Browser Config Tests

    #[test]
    fn test_browser_type_as_str() {
        assert_eq!(BrowserType::Chrome.as_str(), "chrome");
        assert_eq!(BrowserType::Firefox.as_str(), "firefox");
    }

    #[test]
    fn test_viewport_presets() {
        let desktop = Viewport::desktop();
        assert_eq!(desktop.width, 1920);
        assert_eq!(desktop.height, 1080);

        let mobile = Viewport::mobile();
        assert_eq!(mobile.width, 375);
        assert_eq!(mobile.height, 667);
    }

    #[test]
    fn test_browser_config_default() {
        let config = BrowserConfig::default();
        assert!(config.headless);
        assert!(!config.block_images);
        assert!(config.accept_cookies);
    }

    #[test]
    fn test_browser_config_builder() {
        let config = BrowserConfig::new()
            .with_browser(BrowserType::Firefox)
            .headless(false)
            .with_viewport(Viewport::mobile())
            .block_images()
            .with_header("X-Custom", "value");

        assert_eq!(config.browser_type, BrowserType::Firefox);
        assert!(!config.headless);
        assert!(config.block_images);
        assert!(config.extra_headers.contains_key("X-Custom"));
    }

    // Locator Tests

    #[test]
    fn test_locator_css() {
        let locator = Locator::css(".my-class");
        assert_eq!(locator.selector_type, SelectorType::Css);
        assert_eq!(locator.value, ".my-class");
    }

    #[test]
    fn test_locator_id() {
        let locator = Locator::id("submit-btn");
        assert_eq!(locator.selector_type, SelectorType::Id);
    }

    // Page Element Tests

    #[test]
    fn test_page_element_creation() {
        let elem = PageElement::new("button")
            .with_text("Click me")
            .with_attribute("type", "submit")
            .visible(true)
            .enabled(true);

        assert_eq!(elem.tag_name, "button");
        assert_eq!(elem.text, Some("Click me".to_string()));
        assert!(elem.visible);
        assert!(elem.is_button());
    }

    #[test]
    fn test_page_element_is_input() {
        let input = PageElement::new("input");
        let div = PageElement::new("div");

        assert!(input.is_input());
        assert!(!div.is_input());
    }

    #[test]
    fn test_page_element_is_link() {
        let link = PageElement::new("a");
        assert!(link.is_link());
    }

    // Action Tests

    #[test]
    fn test_action_type() {
        let action = BrowserAction::Click(Locator::css(".btn"));
        assert_eq!(action.action_type(), "click");

        let nav = BrowserAction::Navigate("https://example.com".to_string());
        assert_eq!(nav.action_type(), "navigate");
    }

    #[test]
    fn test_action_result_success() {
        let result = ActionResult::success("click", 100);
        assert!(result.success);
        assert_eq!(result.duration_ms, 100);
    }

    #[test]
    fn test_action_result_failure() {
        let result = ActionResult::failure("click", "Element not found");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    // Form Tests

    #[test]
    fn test_form_field_type_from_str() {
        assert_eq!(FormFieldType::from_str("password"), FormFieldType::Password);
        assert_eq!(FormFieldType::from_str("email"), FormFieldType::Email);
        assert_eq!(FormFieldType::from_str("unknown"), FormFieldType::Text);
    }

    #[test]
    fn test_detected_form() {
        let form = DetectedForm::new()
            .with_id("login-form")
            .with_method("POST")
            .with_field(
                FormField::new("username", FormFieldType::Text, Locator::name("username"))
                    .required(),
            );

        assert_eq!(form.method, "POST");
        assert_eq!(form.fields.len(), 1);
        assert_eq!(form.required_fields().len(), 1);
    }

    #[test]
    fn test_form_filler() {
        let mut filler = FormFiller::new();
        filler.set("username", "testuser");
        filler.set("password", "secret");

        assert_eq!(filler.get("username"), Some("testuser"));
    }

    #[test]
    fn test_form_filler_fill_actions() {
        let mut filler = FormFiller::new();
        filler.set("username", "test");

        let form = DetectedForm::new().with_field(FormField::new(
            "username",
            FormFieldType::Text,
            Locator::name("username"),
        ));

        let actions = filler.fill_actions(&form);
        assert_eq!(actions.len(), 2); // Clear + Type
    }

    // Scraping Tests

    #[test]
    fn test_scrape_target() {
        let target = ScrapeTarget::new("title", Locator::css("h1"), ExtractType::Text).multiple();
        assert_eq!(target.name, "title");
        assert!(target.multiple);
    }

    #[test]
    fn test_scraped_data() {
        let data = ScrapedData::new("prices")
            .with_value("$10.00")
            .with_value("$20.00");

        assert_eq!(data.count(), 2);
        assert_eq!(data.first(), Some("$10.00"));
    }

    #[test]
    fn test_scrape_result() {
        let mut result = ScrapeResult::new("https://example.com").with_title("Example");

        result.add_data(ScrapedData::new("heading").with_value("Hello"));

        assert!(result.get("heading").is_some());
    }

    // Test Case Tests

    #[test]
    fn test_test_step_action() {
        let step = TestStep::action("Click login", BrowserAction::Click(Locator::id("login")));
        assert!(step.action.is_some());
        assert!(step.assertion.is_none());
    }

    #[test]
    fn test_test_step_assertion() {
        let step = TestStep::assertion("Check title", Assertion::TitleEquals("Home".to_string()));
        assert!(step.action.is_none());
        assert!(step.assertion.is_some());
    }

    #[test]
    fn test_test_case_creation() {
        let test = TestCase::new("Login Test")
            .with_description("Tests the login flow")
            .setup(TestStep::action(
                "Navigate",
                BrowserAction::Navigate("https://app.com".to_string()),
            ))
            .step(TestStep::action(
                "Click login",
                BrowserAction::Click(Locator::id("login")),
            ))
            .step(TestStep::assertion(
                "Check dashboard",
                Assertion::UrlContains("dashboard".to_string()),
            ))
            .tag("smoke");

        assert_eq!(test.name, "Login Test");
        assert_eq!(test.setup_steps.len(), 1);
        assert_eq!(test.steps.len(), 2);
        assert_eq!(test.tags.len(), 1);
    }

    #[test]
    fn test_step_result_passed() {
        let result = StepResult::passed("Click button", 50);
        assert_eq!(result.status, TestStatus::Passed);
        assert_eq!(result.duration_ms, 50);
    }

    #[test]
    fn test_test_run_result() {
        let mut result = TestRunResult::new("test_1", "Login Test");
        result.add_step_result(StepResult::passed("Step 1", 100));
        result.add_step_result(StepResult::passed("Step 2", 50));
        result.finish();

        assert_eq!(result.status, TestStatus::Passed);
        assert_eq!(result.passed_steps(), 2);
        assert_eq!(result.total_duration_ms, 150);
    }

    #[test]
    fn test_test_run_result_failure() {
        let mut result = TestRunResult::new("test_1", "Login Test");
        result.add_step_result(StepResult::passed("Step 1", 100));
        result.add_step_result(StepResult::failed("Step 2", "Element not found"));

        assert_eq!(result.status, TestStatus::Failed);
        assert_eq!(result.failed_steps(), 1);
    }

    // Browser Session Tests

    #[test]
    fn test_browser_session_creation() {
        let config = BrowserConfig::default();
        let session = BrowserSession::new(config);

        assert!(session.is_open);
        assert!(session.current_url.is_none());
    }

    #[test]
    fn test_browser_session_actions() {
        let mut session = BrowserSession::new(BrowserConfig::default());

        session.record_action(BrowserAction::Navigate("https://example.com".to_string()));
        session.set_url("https://example.com");
        session.set_title("Example");

        assert_eq!(session.action_count(), 1);
        assert_eq!(session.current_url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_browser_session_cookies() {
        let mut session = BrowserSession::new(BrowserConfig::default());
        session.add_cookie("session_id", "abc123");

        assert_eq!(session.get_cookie("session_id"), Some("abc123"));
        assert_eq!(session.get_cookie("unknown"), None);
    }

    #[test]
    fn test_browser_session_close() {
        let mut session = BrowserSession::new(BrowserConfig::default());
        assert!(session.is_open);

        session.close();
        assert!(!session.is_open);
    }

    // Unique ID Tests

    #[test]
    fn test_unique_session_ids() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_unique_element_ids() {
        let id1 = generate_element_id();
        let id2 = generate_element_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_unique_test_ids() {
        let id1 = generate_test_id();
        let id2 = generate_test_id();
        assert_ne!(id1, id2);
    }

    // Type String Tests

    #[test]
    fn test_selector_type_as_str() {
        assert_eq!(SelectorType::Css.as_str(), "css");
        assert_eq!(SelectorType::XPath.as_str(), "xpath");
    }

    #[test]
    fn test_mouse_button_as_str() {
        assert_eq!(MouseButton::Left.as_str(), "left");
        assert_eq!(MouseButton::Right.as_str(), "right");
    }

    #[test]
    fn test_key_modifier_as_str() {
        assert_eq!(KeyModifier::Control.as_str(), "Control");
        assert_eq!(KeyModifier::Shift.as_str(), "Shift");
    }

    #[test]
    fn test_form_field_type_as_str() {
        assert_eq!(FormFieldType::Text.as_str(), "text");
        assert_eq!(FormFieldType::Password.as_str(), "password");
    }

    #[test]
    fn test_test_status_as_str() {
        assert_eq!(TestStatus::Passed.as_str(), "passed");
        assert_eq!(TestStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn test_assertion_type() {
        let assertion = Assertion::ElementExists(Locator::css(".btn"));
        assert_eq!(assertion.assertion_type(), "element_exists");

        let title = Assertion::TitleEquals("Home".to_string());
        assert_eq!(title.assertion_type(), "title_equals");
    }

    #[test]
    fn test_element_bounds_center() {
        let bounds = ElementBounds::new(100.0, 100.0, 200.0, 100.0);
        let (cx, cy) = bounds.center();
        assert_eq!(cx, 200.0);
        assert_eq!(cy, 150.0);
    }
}
