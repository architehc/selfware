//! Web task definitions and predefined benchmark scenarios.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single browser action in a web task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebAction {
    /// Navigate to a URL.
    Navigate { url: String },
    /// Click an element by CSS selector.
    Click { selector: String },
    /// Fill a form field.
    Fill { selector: String, value: String },
    /// Extract text from an element and compare against expected.
    Extract { selector: String, expected: String },
    /// Take a screenshot with a label.
    Screenshot { label: String },
    /// Wait for an element to appear.
    WaitFor { selector: String, timeout_ms: u64 },
    /// Scroll the page.
    Scroll {
        direction: ScrollDirection,
        amount: i32,
    },
    /// Press a keyboard key.
    Press { key: String },
    /// Hover over an element.
    Hover { selector: String },
}

/// Scroll direction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Success criteria for evaluating a web task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuccessCriterion {
    /// Current URL must contain this substring.
    UrlContains(String),
    /// Page text content must contain this substring.
    PageContains(String),
    /// An element matching this selector must be visible.
    ElementVisible(String),
    /// Extracted data must match expected value.
    ExtractedDataMatches { key: String, expected: String },
    /// Screenshot must be visually similar to baseline.
    VisualSimilarity { baseline: PathBuf, threshold: f64 },
}

/// A complete web task benchmark definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebTask {
    /// Unique task identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Task description.
    pub description: String,
    /// Ordered sequence of actions.
    pub actions: Vec<WebAction>,
    /// How to determine if the task succeeded.
    pub success_criteria: Vec<SuccessCriterion>,
    /// Timeout for the entire task.
    pub timeout_secs: u64,
}

impl WebTask {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            actions: Vec::new(),
            success_criteria: Vec::new(),
            timeout_secs: 60,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_action(mut self, action: WebAction) -> Self {
        self.actions.push(action);
        self
    }

    pub fn with_criterion(mut self, criterion: SuccessCriterion) -> Self {
        self.success_criteria.push(criterion);
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

// --- Predefined benchmark scenarios ---

/// Search and extract: navigate to a search engine, enter a query, extract results.
pub fn scenario_search_extract() -> WebTask {
    WebTask::new("search-extract", "Search and Extract")
        .with_description("Navigate to a search engine, search for a term, extract result titles")
        .with_action(WebAction::Navigate {
            url: "https://html.duckduckgo.com/html/".into(),
        })
        .with_action(WebAction::Fill {
            selector: "input[name='q']".into(),
            value: "Rust programming language".into(),
        })
        .with_action(WebAction::Click {
            selector: "input[type='submit']".into(),
        })
        .with_action(WebAction::WaitFor {
            selector: ".result__title".into(),
            timeout_ms: 10000,
        })
        .with_action(WebAction::Screenshot {
            label: "search_results".into(),
        })
        .with_action(WebAction::Extract {
            selector: ".result__title".into(),
            expected: "Rust".into(),
        })
        .with_criterion(SuccessCriterion::PageContains("Rust".into()))
        .with_timeout(30)
}

/// Multi-step navigation: follow links across multiple pages.
pub fn scenario_multi_step_navigation() -> WebTask {
    WebTask::new("multi-step-nav", "Multi-Step Navigation")
        .with_description("Navigate across multiple pages following internal links")
        .with_action(WebAction::Navigate {
            url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".into(),
        })
        .with_action(WebAction::WaitFor {
            selector: "#firstHeading".into(),
            timeout_ms: 10000,
        })
        .with_action(WebAction::Screenshot {
            label: "wiki_rust_page".into(),
        })
        .with_action(WebAction::Extract {
            selector: "#firstHeading".into(),
            expected: "Rust".into(),
        })
        .with_action(WebAction::Click {
            selector: "a[href='/wiki/Memory_safety']".into(),
        })
        .with_action(WebAction::WaitFor {
            selector: "#firstHeading".into(),
            timeout_ms: 10000,
        })
        .with_action(WebAction::Screenshot {
            label: "wiki_memory_safety".into(),
        })
        .with_criterion(SuccessCriterion::UrlContains("Memory_safety".into()))
        .with_criterion(SuccessCriterion::PageContains("memory safety".into()))
        .with_timeout(60)
}

/// Form interaction: fill out a multi-field form.
pub fn scenario_form_fill() -> WebTask {
    WebTask::new("form-fill", "Form Interaction")
        .with_description("Fill and submit a multi-field form, verify confirmation")
        .with_action(WebAction::Navigate {
            url: "https://httpbin.org/forms/post".into(),
        })
        .with_action(WebAction::WaitFor {
            selector: "form".into(),
            timeout_ms: 10000,
        })
        .with_action(WebAction::Fill {
            selector: "input[name='custname']".into(),
            value: "Selfware Test".into(),
        })
        .with_action(WebAction::Fill {
            selector: "input[name='custtel']".into(),
            value: "555-0123".into(),
        })
        .with_action(WebAction::Fill {
            selector: "input[name='custemail']".into(),
            value: "test@selfware.dev".into(),
        })
        .with_action(WebAction::Screenshot {
            label: "form_filled".into(),
        })
        .with_action(WebAction::Click {
            selector: "button[type='submit'], input[type='submit']".into(),
        })
        .with_action(WebAction::WaitFor {
            selector: "body".into(),
            timeout_ms: 10000,
        })
        .with_action(WebAction::Screenshot {
            label: "form_submitted".into(),
        })
        .with_criterion(SuccessCriterion::PageContains("Selfware Test".into()))
        .with_timeout(30)
}

/// Data extraction: visit a page, extract structured data.
pub fn scenario_data_extraction() -> WebTask {
    WebTask::new("data-extract", "Data Extraction")
        .with_description("Visit a JSON API endpoint and extract structured data")
        .with_action(WebAction::Navigate {
            url: "https://httpbin.org/json".into(),
        })
        .with_action(WebAction::WaitFor {
            selector: "body".into(),
            timeout_ms: 10000,
        })
        .with_action(WebAction::Extract {
            selector: "body".into(),
            expected: "slideshow".into(),
        })
        .with_action(WebAction::Screenshot {
            label: "json_data".into(),
        })
        .with_criterion(SuccessCriterion::PageContains("slideshow".into()))
        .with_timeout(30)
}

/// Return all predefined scenarios.
pub fn all_scenarios() -> Vec<WebTask> {
    vec![
        scenario_search_extract(),
        scenario_multi_step_navigation(),
        scenario_form_fill(),
        scenario_data_extraction(),
    ]
}

#[cfg(test)]
#[path = "../../../tests/unit/bench_harness/computer_control/tasks/tasks_test.rs"]
mod tests;
