//! Image/Screenshot Understanding
//!
//! Multi-modal visual input processing:
//! - Screenshot analysis for UI bugs
//! - Diagram interpretation
//! - Image-to-code generation
//! - Visual diff comparison

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global counter for unique IDs
static ANALYSIS_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_analysis_id() -> String {
    format!(
        "img_{}_{:x}",
        ANALYSIS_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
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
// Image Types
// ============================================================================

/// Supported image formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
    Bmp,
    Unknown,
}

impl ImageFormat {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "png" => ImageFormat::Png,
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "gif" => ImageFormat::Gif,
            "webp" => ImageFormat::Webp,
            "svg" => ImageFormat::Svg,
            "bmp" => ImageFormat::Bmp,
            _ => ImageFormat::Unknown,
        }
    }

    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(Self::from_extension)
            .unwrap_or(ImageFormat::Unknown)
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Svg => "image/svg+xml",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Unknown => "application/octet-stream",
        }
    }
}

/// Image dimensions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

impl ImageDimensions {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn aspect_ratio(&self) -> f64 {
        if self.height == 0 {
            0.0
        } else {
            self.width as f64 / self.height as f64
        }
    }

    pub fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    pub fn is_landscape(&self) -> bool {
        self.width > self.height
    }

    pub fn is_portrait(&self) -> bool {
        self.height > self.width
    }

    pub fn is_square(&self) -> bool {
        self.width == self.height
    }
}

/// Image metadata
#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub path: Option<PathBuf>,
    pub format: ImageFormat,
    pub dimensions: Option<ImageDimensions>,
    pub file_size: Option<u64>,
    pub color_depth: Option<u8>,
    pub has_alpha: bool,
    pub exif_data: HashMap<String, String>,
}

impl ImageMetadata {
    pub fn new(format: ImageFormat) -> Self {
        Self {
            path: None,
            format,
            dimensions: None,
            file_size: None,
            color_depth: None,
            has_alpha: false,
            exif_data: HashMap::new(),
        }
    }

    /// Builder: set path
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Builder: set dimensions
    pub fn with_dimensions(mut self, width: u32, height: u32) -> Self {
        self.dimensions = Some(ImageDimensions::new(width, height));
        self
    }

    /// Builder: set file size
    pub fn with_file_size(mut self, size: u64) -> Self {
        self.file_size = Some(size);
        self
    }

    /// Builder: set color depth
    pub fn with_color_depth(mut self, depth: u8) -> Self {
        self.color_depth = Some(depth);
        self
    }

    /// Builder: set alpha channel
    pub fn with_alpha(mut self) -> Self {
        self.has_alpha = true;
        self
    }

    /// Builder: add EXIF data
    pub fn with_exif(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.exif_data.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// UI Element Detection
// ============================================================================

/// UI element type detected in screenshot
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UiElementType {
    Button,
    TextInput,
    TextArea,
    Checkbox,
    RadioButton,
    Dropdown,
    Slider,
    Toggle,
    Link,
    Image,
    Icon,
    Label,
    Heading,
    Paragraph,
    List,
    Table,
    Card,
    Modal,
    Navbar,
    Sidebar,
    Footer,
    Form,
    Container,
    Unknown(String),
}

impl UiElementType {
    pub fn as_str(&self) -> &str {
        match self {
            UiElementType::Button => "button",
            UiElementType::TextInput => "text_input",
            UiElementType::TextArea => "text_area",
            UiElementType::Checkbox => "checkbox",
            UiElementType::RadioButton => "radio_button",
            UiElementType::Dropdown => "dropdown",
            UiElementType::Slider => "slider",
            UiElementType::Toggle => "toggle",
            UiElementType::Link => "link",
            UiElementType::Image => "image",
            UiElementType::Icon => "icon",
            UiElementType::Label => "label",
            UiElementType::Heading => "heading",
            UiElementType::Paragraph => "paragraph",
            UiElementType::List => "list",
            UiElementType::Table => "table",
            UiElementType::Card => "card",
            UiElementType::Modal => "modal",
            UiElementType::Navbar => "navbar",
            UiElementType::Sidebar => "sidebar",
            UiElementType::Footer => "footer",
            UiElementType::Form => "form",
            UiElementType::Container => "container",
            UiElementType::Unknown(s) => s.as_str(),
        }
    }
}

/// Bounding box for an element
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl BoundingBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let max_x = (self.x + self.width).max(other.x + other.width);
        let max_y = (self.y + self.height).max(other.y + other.height);
        BoundingBox::new(x, y, max_x - x, max_y - y)
    }
}

/// Detected UI element
#[derive(Debug, Clone)]
pub struct UiElement {
    /// Element type
    pub element_type: UiElementType,
    /// Bounding box
    pub bounds: BoundingBox,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Detected text content
    pub text: Option<String>,
    /// Element properties
    pub properties: HashMap<String, String>,
    /// Child elements
    pub children: Vec<UiElement>,
}

impl UiElement {
    pub fn new(element_type: UiElementType, bounds: BoundingBox, confidence: f32) -> Self {
        Self {
            element_type,
            bounds,
            confidence,
            text: None,
            properties: HashMap::new(),
            children: Vec::new(),
        }
    }

    /// Builder: set text
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Builder: add property
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Builder: add child
    pub fn with_child(mut self, child: UiElement) -> Self {
        self.children.push(child);
        self
    }

    /// Check if element is interactive
    pub fn is_interactive(&self) -> bool {
        matches!(
            self.element_type,
            UiElementType::Button
                | UiElementType::TextInput
                | UiElementType::TextArea
                | UiElementType::Checkbox
                | UiElementType::RadioButton
                | UiElementType::Dropdown
                | UiElementType::Slider
                | UiElementType::Toggle
                | UiElementType::Link
        )
    }
}

// ============================================================================
// UI Bug Detection
// ============================================================================

/// UI bug severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BugSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl BugSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            BugSeverity::Info => "info",
            BugSeverity::Low => "low",
            BugSeverity::Medium => "medium",
            BugSeverity::High => "high",
            BugSeverity::Critical => "critical",
        }
    }
}

/// Type of UI bug detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiBugType {
    /// Overlapping elements
    Overlap,
    /// Truncated text
    TextTruncation,
    /// Alignment issues
    Alignment,
    /// Spacing inconsistency
    Spacing,
    /// Color contrast issues
    Contrast,
    /// Missing element
    MissingElement,
    /// Broken layout
    BrokenLayout,
    /// Overflow content
    Overflow,
    /// Empty state not handled
    EmptyState,
    /// Loading state issues
    LoadingState,
    /// Responsive design issue
    ResponsiveIssue,
    /// Accessibility issue
    Accessibility,
    /// Visual regression
    VisualRegression,
    /// Custom bug type
    Custom(String),
}

impl UiBugType {
    pub fn as_str(&self) -> &str {
        match self {
            UiBugType::Overlap => "overlap",
            UiBugType::TextTruncation => "text_truncation",
            UiBugType::Alignment => "alignment",
            UiBugType::Spacing => "spacing",
            UiBugType::Contrast => "contrast",
            UiBugType::MissingElement => "missing_element",
            UiBugType::BrokenLayout => "broken_layout",
            UiBugType::Overflow => "overflow",
            UiBugType::EmptyState => "empty_state",
            UiBugType::LoadingState => "loading_state",
            UiBugType::ResponsiveIssue => "responsive_issue",
            UiBugType::Accessibility => "accessibility",
            UiBugType::VisualRegression => "visual_regression",
            UiBugType::Custom(s) => s.as_str(),
        }
    }
}

/// Detected UI bug
#[derive(Debug, Clone)]
pub struct UiBug {
    /// Bug type
    pub bug_type: UiBugType,
    /// Severity
    pub severity: BugSeverity,
    /// Description
    pub description: String,
    /// Location in image
    pub location: Option<BoundingBox>,
    /// Related elements
    pub related_elements: Vec<String>,
    /// Suggested fix
    pub suggested_fix: Option<String>,
    /// Confidence score
    pub confidence: f32,
}

impl UiBug {
    pub fn new(bug_type: UiBugType, severity: BugSeverity, description: impl Into<String>) -> Self {
        Self {
            bug_type,
            severity,
            description: description.into(),
            location: None,
            related_elements: Vec::new(),
            suggested_fix: None,
            confidence: 1.0,
        }
    }

    /// Builder: set location
    pub fn at_location(mut self, location: BoundingBox) -> Self {
        self.location = Some(location);
        self
    }

    /// Builder: add related element
    pub fn related(mut self, element: impl Into<String>) -> Self {
        self.related_elements.push(element.into());
        self
    }

    /// Builder: set suggested fix
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.suggested_fix = Some(fix.into());
        self
    }

    /// Builder: set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// Screenshot analyzer for UI bugs
#[derive(Debug, Default)]
pub struct ScreenshotAnalyzer {
    /// Detection thresholds
    overlap_threshold: f32,
    spacing_tolerance: f32,
    contrast_ratio_min: f32,
}

impl ScreenshotAnalyzer {
    pub fn new() -> Self {
        Self {
            overlap_threshold: 0.1,  // 10% overlap triggers warning
            spacing_tolerance: 4.0,  // 4px spacing inconsistency tolerance
            contrast_ratio_min: 4.5, // WCAG AA minimum
        }
    }

    /// Builder: set overlap threshold
    pub fn with_overlap_threshold(mut self, threshold: f32) -> Self {
        self.overlap_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Builder: set spacing tolerance
    pub fn with_spacing_tolerance(mut self, tolerance: f32) -> Self {
        self.spacing_tolerance = tolerance;
        self
    }

    /// Builder: set minimum contrast ratio
    pub fn with_contrast_min(mut self, ratio: f32) -> Self {
        self.contrast_ratio_min = ratio;
        self
    }

    /// Analyze elements for overlaps
    pub fn detect_overlaps(&self, elements: &[UiElement]) -> Vec<UiBug> {
        let mut bugs = Vec::new();

        for (i, elem1) in elements.iter().enumerate() {
            for elem2 in elements.iter().skip(i + 1) {
                if elem1.bounds.intersects(&elem2.bounds) {
                    let overlap_area = self.calculate_overlap_area(&elem1.bounds, &elem2.bounds);
                    let smaller_area = elem1.bounds.area().min(elem2.bounds.area());

                    if smaller_area > 0.0 && overlap_area / smaller_area > self.overlap_threshold {
                        bugs.push(
                            UiBug::new(
                                UiBugType::Overlap,
                                BugSeverity::Medium,
                                format!(
                                    "{} and {} elements overlap",
                                    elem1.element_type.as_str(),
                                    elem2.element_type.as_str()
                                ),
                            )
                            .at_location(elem1.bounds.union(&elem2.bounds))
                            .related(elem1.element_type.as_str())
                            .related(elem2.element_type.as_str())
                            .with_fix("Adjust element positions or z-index to prevent overlap"),
                        );
                    }
                }
            }
        }

        bugs
    }

    /// Calculate overlap area between two bounding boxes
    fn calculate_overlap_area(&self, a: &BoundingBox, b: &BoundingBox) -> f32 {
        let x_overlap = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
        let y_overlap = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);

        if x_overlap > 0.0 && y_overlap > 0.0 {
            x_overlap * y_overlap
        } else {
            0.0
        }
    }

    /// Detect alignment issues
    pub fn detect_alignment_issues(&self, elements: &[UiElement]) -> Vec<UiBug> {
        let mut bugs = Vec::new();

        // Group elements by approximate horizontal position
        let mut horizontal_groups: HashMap<i32, Vec<&UiElement>> = HashMap::new();
        for elem in elements {
            let rounded_y = (elem.bounds.y / 10.0).round() as i32 * 10;
            horizontal_groups.entry(rounded_y).or_default().push(elem);
        }

        // Check alignment within groups
        for (_, group) in horizontal_groups {
            if group.len() > 1 {
                let y_values: Vec<f32> = group.iter().map(|e| e.bounds.y).collect();
                let y_min = y_values.iter().cloned().fold(f32::INFINITY, f32::min);
                let y_max = y_values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

                if y_max - y_min > self.spacing_tolerance {
                    bugs.push(
                        UiBug::new(
                            UiBugType::Alignment,
                            BugSeverity::Low,
                            format!("Elements in row are misaligned by {:.1}px", y_max - y_min),
                        )
                        .with_fix("Align elements to a common baseline or flexbox"),
                    );
                }
            }
        }

        bugs
    }

    /// Full analysis of screenshot
    pub fn analyze(&self, elements: &[UiElement]) -> ScreenshotAnalysis {
        let mut bugs = Vec::new();

        // Run all detections
        bugs.extend(self.detect_overlaps(elements));
        bugs.extend(self.detect_alignment_issues(elements));

        ScreenshotAnalysis {
            id: generate_analysis_id(),
            elements: elements.to_vec(),
            bugs,
            analyzed_at: current_timestamp(),
        }
    }
}

/// Screenshot analysis result
#[derive(Debug, Clone)]
pub struct ScreenshotAnalysis {
    /// Analysis ID
    pub id: String,
    /// Detected elements
    pub elements: Vec<UiElement>,
    /// Detected bugs
    pub bugs: Vec<UiBug>,
    /// Analysis timestamp
    pub analyzed_at: u64,
}

impl ScreenshotAnalysis {
    /// Get bugs by severity
    pub fn bugs_by_severity(&self, severity: BugSeverity) -> Vec<&UiBug> {
        self.bugs
            .iter()
            .filter(|b| b.severity == severity)
            .collect()
    }

    /// Get critical and high severity bugs
    pub fn critical_bugs(&self) -> Vec<&UiBug> {
        self.bugs
            .iter()
            .filter(|b| b.severity >= BugSeverity::High)
            .collect()
    }

    /// Check if analysis passed (no critical/high bugs)
    pub fn passed(&self) -> bool {
        self.critical_bugs().is_empty()
    }

    /// Get summary
    pub fn summary(&self) -> AnalysisSummary {
        let mut by_severity: HashMap<BugSeverity, usize> = HashMap::new();
        let mut by_type: HashMap<String, usize> = HashMap::new();

        for bug in &self.bugs {
            *by_severity.entry(bug.severity).or_insert(0) += 1;
            *by_type
                .entry(bug.bug_type.as_str().to_string())
                .or_insert(0) += 1;
        }

        AnalysisSummary {
            total_elements: self.elements.len(),
            total_bugs: self.bugs.len(),
            bugs_by_severity: by_severity,
            bugs_by_type: by_type,
            passed: self.passed(),
        }
    }
}

/// Analysis summary
#[derive(Debug)]
pub struct AnalysisSummary {
    pub total_elements: usize,
    pub total_bugs: usize,
    pub bugs_by_severity: HashMap<BugSeverity, usize>,
    pub bugs_by_type: HashMap<String, usize>,
    pub passed: bool,
}

// ============================================================================
// Diagram Interpretation
// ============================================================================

/// Diagram type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramType {
    Flowchart,
    SequenceDiagram,
    ClassDiagram,
    EntityRelationship,
    StateDiagram,
    Wireframe,
    Architecture,
    Network,
    Mindmap,
    Gantt,
    Unknown,
}

impl DiagramType {
    pub fn as_str(&self) -> &str {
        match self {
            DiagramType::Flowchart => "flowchart",
            DiagramType::SequenceDiagram => "sequence_diagram",
            DiagramType::ClassDiagram => "class_diagram",
            DiagramType::EntityRelationship => "entity_relationship",
            DiagramType::StateDiagram => "state_diagram",
            DiagramType::Wireframe => "wireframe",
            DiagramType::Architecture => "architecture",
            DiagramType::Network => "network",
            DiagramType::Mindmap => "mindmap",
            DiagramType::Gantt => "gantt",
            DiagramType::Unknown => "unknown",
        }
    }
}

/// Diagram node
#[derive(Debug, Clone)]
pub struct DiagramNode {
    /// Node ID
    pub id: String,
    /// Node label/text
    pub label: String,
    /// Node type/shape
    pub shape: String,
    /// Position
    pub position: Option<(f32, f32)>,
    /// Properties
    pub properties: HashMap<String, String>,
}

impl DiagramNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shape: "rectangle".to_string(),
            position: None,
            properties: HashMap::new(),
        }
    }

    /// Builder: set shape
    pub fn with_shape(mut self, shape: impl Into<String>) -> Self {
        self.shape = shape.into();
        self
    }

    /// Builder: set position
    pub fn at_position(mut self, x: f32, y: f32) -> Self {
        self.position = Some((x, y));
        self
    }

    /// Builder: add property
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }
}

/// Diagram edge/connection
#[derive(Debug, Clone)]
pub struct DiagramEdge {
    /// Source node ID
    pub from: String,
    /// Target node ID
    pub to: String,
    /// Edge label
    pub label: Option<String>,
    /// Edge type
    pub edge_type: String,
    /// Properties
    pub properties: HashMap<String, String>,
}

impl DiagramEdge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
            edge_type: "arrow".to_string(),
            properties: HashMap::new(),
        }
    }

    /// Builder: set label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Builder: set edge type
    pub fn with_type(mut self, edge_type: impl Into<String>) -> Self {
        self.edge_type = edge_type.into();
        self
    }

    /// Builder: add property
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }
}

/// Parsed diagram
#[derive(Debug, Clone)]
pub struct ParsedDiagram {
    /// Diagram type
    pub diagram_type: DiagramType,
    /// Title
    pub title: Option<String>,
    /// Nodes
    pub nodes: Vec<DiagramNode>,
    /// Edges
    pub edges: Vec<DiagramEdge>,
    /// Confidence score
    pub confidence: f32,
}

impl ParsedDiagram {
    pub fn new(diagram_type: DiagramType) -> Self {
        Self {
            diagram_type,
            title: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            confidence: 1.0,
        }
    }

    /// Builder: set title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Builder: add node
    pub fn with_node(mut self, node: DiagramNode) -> Self {
        self.nodes.push(node);
        self
    }

    /// Builder: add edge
    pub fn with_edge(mut self, edge: DiagramEdge) -> Self {
        self.edges.push(edge);
        self
    }

    /// Builder: set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Convert to Mermaid syntax
    pub fn to_mermaid(&self) -> String {
        let mut output = String::new();

        match self.diagram_type {
            DiagramType::Flowchart => {
                output.push_str("flowchart TD\n");
                for node in &self.nodes {
                    output.push_str(&format!("    {}[{}]\n", node.id, node.label));
                }
                for edge in &self.edges {
                    let arrow = if let Some(ref label) = edge.label {
                        format!("-->|{}|", label)
                    } else {
                        "-->".to_string()
                    };
                    output.push_str(&format!("    {} {} {}\n", edge.from, arrow, edge.to));
                }
            }
            DiagramType::SequenceDiagram => {
                output.push_str("sequenceDiagram\n");
                for edge in &self.edges {
                    let label = edge.label.as_deref().unwrap_or("");
                    output.push_str(&format!("    {}->>{}:{}\n", edge.from, edge.to, label));
                }
            }
            DiagramType::ClassDiagram => {
                output.push_str("classDiagram\n");
                for node in &self.nodes {
                    output.push_str(&format!("    class {} {{\n", node.id));
                    output.push_str(&format!("        {}\n", node.label));
                    output.push_str("    }\n");
                }
                for edge in &self.edges {
                    output.push_str(&format!("    {} --> {}\n", edge.from, edge.to));
                }
            }
            _ => {
                // Generic output
                output.push_str("graph TD\n");
                for node in &self.nodes {
                    output.push_str(&format!("    {}[{}]\n", node.id, node.label));
                }
                for edge in &self.edges {
                    output.push_str(&format!("    {} --> {}\n", edge.from, edge.to));
                }
            }
        }

        output
    }

    /// Convert to PlantUML syntax
    pub fn to_plantuml(&self) -> String {
        let mut output = String::new();
        output.push_str("@startuml\n");

        if let Some(ref title) = self.title {
            output.push_str(&format!("title {}\n", title));
        }

        match self.diagram_type {
            DiagramType::SequenceDiagram => {
                for edge in &self.edges {
                    let label = edge.label.as_deref().unwrap_or("");
                    output.push_str(&format!("{} -> {}:{}\n", edge.from, edge.to, label));
                }
            }
            DiagramType::ClassDiagram => {
                for node in &self.nodes {
                    output.push_str(&format!("class {} {{\n", node.id));
                    output.push_str(&format!("    {}\n", node.label));
                    output.push_str("}\n");
                }
                for edge in &self.edges {
                    output.push_str(&format!("{} --> {}\n", edge.from, edge.to));
                }
            }
            _ => {
                for node in &self.nodes {
                    output.push_str(&format!("({}) as {}\n", node.label, node.id));
                }
                for edge in &self.edges {
                    output.push_str(&format!("{} --> {}\n", edge.from, edge.to));
                }
            }
        }

        output.push_str("@enduml\n");
        output
    }
}

/// Diagram interpreter
#[derive(Debug, Default)]
pub struct DiagramInterpreter;

impl DiagramInterpreter {
    pub fn new() -> Self {
        Self
    }

    /// Detect diagram type from visual features
    pub fn detect_type(&self, elements: &[UiElement]) -> DiagramType {
        // Simple heuristics for diagram type detection
        let has_arrows = elements.iter().any(|e| {
            e.properties
                .get("type")
                .map(|t| t.contains("arrow"))
                .unwrap_or(false)
        });

        let has_boxes = elements.iter().any(|e| {
            matches!(
                e.element_type,
                UiElementType::Container | UiElementType::Card
            )
        });

        let has_swimlanes = elements.iter().any(|e| {
            e.properties
                .get("type")
                .map(|t| t.contains("lane"))
                .unwrap_or(false)
        });

        if has_swimlanes {
            DiagramType::SequenceDiagram
        } else if has_arrows && has_boxes {
            DiagramType::Flowchart
        } else if has_boxes {
            DiagramType::ClassDiagram
        } else {
            DiagramType::Unknown
        }
    }

    /// Parse elements into a diagram structure
    pub fn parse(&self, elements: &[UiElement]) -> ParsedDiagram {
        let diagram_type = self.detect_type(elements);
        let mut diagram = ParsedDiagram::new(diagram_type);

        // Extract nodes from container/card elements
        for (i, elem) in elements.iter().enumerate() {
            if matches!(
                elem.element_type,
                UiElementType::Container | UiElementType::Card
            ) {
                let node = DiagramNode::new(
                    format!("node_{}", i),
                    elem.text.clone().unwrap_or_else(|| format!("Node {}", i)),
                );
                diagram = diagram.with_node(node);
            }
        }

        diagram
    }
}

// ============================================================================
// Visual Diff Comparison
// ============================================================================

/// Diff region type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffType {
    /// Element added
    Added,
    /// Element removed
    Removed,
    /// Element modified
    Modified,
    /// Element moved
    Moved,
    /// No change
    Unchanged,
}

impl DiffType {
    pub fn as_str(&self) -> &str {
        match self {
            DiffType::Added => "added",
            DiffType::Removed => "removed",
            DiffType::Modified => "modified",
            DiffType::Moved => "moved",
            DiffType::Unchanged => "unchanged",
        }
    }
}

/// A difference between two images
#[derive(Debug, Clone)]
pub struct VisualDiff {
    /// Difference type
    pub diff_type: DiffType,
    /// Region in before image
    pub before_region: Option<BoundingBox>,
    /// Region in after image
    pub after_region: Option<BoundingBox>,
    /// Description
    pub description: String,
    /// Pixel difference percentage
    pub pixel_diff_percent: f32,
}

impl VisualDiff {
    pub fn added(region: BoundingBox, description: impl Into<String>) -> Self {
        Self {
            diff_type: DiffType::Added,
            before_region: None,
            after_region: Some(region),
            description: description.into(),
            pixel_diff_percent: 100.0,
        }
    }

    pub fn removed(region: BoundingBox, description: impl Into<String>) -> Self {
        Self {
            diff_type: DiffType::Removed,
            before_region: Some(region),
            after_region: None,
            description: description.into(),
            pixel_diff_percent: 100.0,
        }
    }

    pub fn modified(
        before: BoundingBox,
        after: BoundingBox,
        description: impl Into<String>,
        diff_percent: f32,
    ) -> Self {
        Self {
            diff_type: DiffType::Modified,
            before_region: Some(before),
            after_region: Some(after),
            description: description.into(),
            pixel_diff_percent: diff_percent,
        }
    }

    pub fn moved(before: BoundingBox, after: BoundingBox, description: impl Into<String>) -> Self {
        Self {
            diff_type: DiffType::Moved,
            before_region: Some(before),
            after_region: Some(after),
            description: description.into(),
            pixel_diff_percent: 0.0,
        }
    }
}

/// Visual diff comparison result
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Differences found
    pub diffs: Vec<VisualDiff>,
    /// Overall similarity (0.0 - 1.0)
    pub similarity: f32,
    /// Total pixels compared
    pub total_pixels: u64,
    /// Different pixels
    pub diff_pixels: u64,
}

impl DiffResult {
    pub fn new(
        diffs: Vec<VisualDiff>,
        similarity: f32,
        total_pixels: u64,
        diff_pixels: u64,
    ) -> Self {
        Self {
            diffs,
            similarity,
            total_pixels,
            diff_pixels,
        }
    }

    /// Check if images are identical
    pub fn is_identical(&self) -> bool {
        self.diffs.is_empty() && self.similarity >= 1.0
    }

    /// Check if images are similar (within threshold)
    pub fn is_similar(&self, threshold: f32) -> bool {
        self.similarity >= threshold
    }

    /// Get diffs by type
    pub fn diffs_by_type(&self, diff_type: DiffType) -> Vec<&VisualDiff> {
        self.diffs
            .iter()
            .filter(|d| d.diff_type == diff_type)
            .collect()
    }

    /// Get diff percentage
    pub fn diff_percentage(&self) -> f32 {
        if self.total_pixels == 0 {
            0.0
        } else {
            (self.diff_pixels as f32 / self.total_pixels as f32) * 100.0
        }
    }
}

/// Visual diff comparator
#[derive(Debug)]
pub struct VisualDiffComparator {
    /// Tolerance for pixel comparison (0-255)
    pub color_tolerance: u8,
    /// Minimum region size to report
    pub min_region_size: u32,
    /// Anti-aliasing detection
    pub detect_antialiasing: bool,
}

impl Default for VisualDiffComparator {
    fn default() -> Self {
        Self {
            color_tolerance: 10,
            min_region_size: 100,
            detect_antialiasing: true,
        }
    }
}

impl VisualDiffComparator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set color tolerance
    pub fn with_tolerance(mut self, tolerance: u8) -> Self {
        self.color_tolerance = tolerance;
        self
    }

    /// Builder: set minimum region size
    pub fn with_min_region(mut self, size: u32) -> Self {
        self.min_region_size = size;
        self
    }

    /// Builder: enable/disable anti-aliasing detection
    pub fn detect_antialiasing(mut self, detect: bool) -> Self {
        self.detect_antialiasing = detect;
        self
    }

    /// Compare two sets of UI elements
    pub fn compare_elements(&self, before: &[UiElement], after: &[UiElement]) -> DiffResult {
        let mut diffs = Vec::new();

        // Find removed elements (in before but not in after)
        for before_elem in before {
            let found = after.iter().any(|a| self.elements_match(before_elem, a));
            if !found {
                diffs.push(VisualDiff::removed(
                    before_elem.bounds,
                    format!("{} element removed", before_elem.element_type.as_str()),
                ));
            }
        }

        // Find added elements (in after but not in before)
        for after_elem in after {
            let found = before.iter().any(|b| self.elements_match(b, after_elem));
            if !found {
                diffs.push(VisualDiff::added(
                    after_elem.bounds,
                    format!("{} element added", after_elem.element_type.as_str()),
                ));
            }
        }

        // Find modified elements
        for before_elem in before {
            for after_elem in after {
                if self.elements_similar(before_elem, after_elem)
                    && !self.elements_identical(before_elem, after_elem)
                {
                    if self.elements_moved(before_elem, after_elem) {
                        diffs.push(VisualDiff::moved(
                            before_elem.bounds,
                            after_elem.bounds,
                            format!("{} element moved", before_elem.element_type.as_str()),
                        ));
                    } else {
                        diffs.push(VisualDiff::modified(
                            before_elem.bounds,
                            after_elem.bounds,
                            format!("{} element modified", before_elem.element_type.as_str()),
                            50.0,
                        ));
                    }
                }
            }
        }

        let similarity = if (before.is_empty() && after.is_empty()) || diffs.is_empty() {
            1.0
        } else {
            let total = before.len().max(after.len()) as f32;
            let diff_count = diffs.len() as f32;
            1.0 - (diff_count / total).min(1.0)
        };

        DiffResult::new(diffs, similarity, 0, 0)
    }

    /// Check if two elements match exactly
    fn elements_match(&self, a: &UiElement, b: &UiElement) -> bool {
        self.elements_identical(a, b)
    }

    /// Check if elements are similar (same type and text)
    fn elements_similar(&self, a: &UiElement, b: &UiElement) -> bool {
        a.element_type == b.element_type && a.text == b.text
    }

    /// Check if elements are identical (including position)
    fn elements_identical(&self, a: &UiElement, b: &UiElement) -> bool {
        a.element_type == b.element_type
            && a.text == b.text
            && (a.bounds.x - b.bounds.x).abs() < 1.0
            && (a.bounds.y - b.bounds.y).abs() < 1.0
            && (a.bounds.width - b.bounds.width).abs() < 1.0
            && (a.bounds.height - b.bounds.height).abs() < 1.0
    }

    /// Check if element was moved
    fn elements_moved(&self, a: &UiElement, b: &UiElement) -> bool {
        self.elements_similar(a, b) && !self.elements_identical(a, b)
    }
}

// ============================================================================
// Image-to-Code Generation
// ============================================================================

/// Code generation framework/language
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeFramework {
    Html,
    React,
    Vue,
    Angular,
    Svelte,
    Flutter,
    SwiftUi,
    Compose,
    Tailwind,
    Custom(String),
}

impl CodeFramework {
    pub fn as_str(&self) -> &str {
        match self {
            CodeFramework::Html => "html",
            CodeFramework::React => "react",
            CodeFramework::Vue => "vue",
            CodeFramework::Angular => "angular",
            CodeFramework::Svelte => "svelte",
            CodeFramework::Flutter => "flutter",
            CodeFramework::SwiftUi => "swiftui",
            CodeFramework::Compose => "compose",
            CodeFramework::Tailwind => "tailwind",
            CodeFramework::Custom(s) => s.as_str(),
        }
    }
}

/// Generated code from image
#[derive(Debug, Clone)]
pub struct GeneratedCode {
    /// Target framework
    pub framework: CodeFramework,
    /// Generated code
    pub code: String,
    /// Component name
    pub component_name: Option<String>,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Styles (if separate)
    pub styles: Option<String>,
}

impl GeneratedCode {
    pub fn new(framework: CodeFramework, code: impl Into<String>) -> Self {
        Self {
            framework,
            code: code.into(),
            component_name: None,
            dependencies: Vec::new(),
            styles: None,
        }
    }

    /// Builder: set component name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.component_name = Some(name.into());
        self
    }

    /// Builder: add dependency
    pub fn with_dependency(mut self, dep: impl Into<String>) -> Self {
        self.dependencies.push(dep.into());
        self
    }

    /// Builder: set styles
    pub fn with_styles(mut self, styles: impl Into<String>) -> Self {
        self.styles = Some(styles.into());
        self
    }
}

/// Image to code generator
#[derive(Debug)]
pub struct ImageToCodeGenerator {
    framework: CodeFramework,
    use_semantic_html: bool,
    use_flexbox: bool,
    use_grid: bool,
}

impl Default for ImageToCodeGenerator {
    fn default() -> Self {
        Self::new(CodeFramework::Html)
    }
}

impl ImageToCodeGenerator {
    pub fn new(framework: CodeFramework) -> Self {
        Self {
            framework,
            use_semantic_html: true,
            use_flexbox: true,
            use_grid: false,
        }
    }

    /// Builder: use semantic HTML
    pub fn semantic_html(mut self, use_it: bool) -> Self {
        self.use_semantic_html = use_it;
        self
    }

    /// Builder: use flexbox
    pub fn flexbox(mut self, use_it: bool) -> Self {
        self.use_flexbox = use_it;
        self
    }

    /// Builder: use CSS grid
    pub fn grid(mut self, use_it: bool) -> Self {
        self.use_grid = use_it;
        self
    }

    /// Generate code from UI elements
    pub fn generate(&self, elements: &[UiElement]) -> GeneratedCode {
        match self.framework {
            CodeFramework::Html | CodeFramework::Tailwind => self.generate_html(elements),
            CodeFramework::React => self.generate_react(elements),
            _ => self.generate_html(elements),
        }
    }

    /// Generate HTML code
    fn generate_html(&self, elements: &[UiElement]) -> GeneratedCode {
        let mut code = String::new();
        code.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        code.push_str("    <meta charset=\"UTF-8\">\n");
        code.push_str(
            "    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n",
        );
        code.push_str("</head>\n<body>\n");

        for elem in elements {
            code.push_str(&self.element_to_html(elem, 1));
        }

        code.push_str("</body>\n</html>\n");

        GeneratedCode::new(self.framework.clone(), code)
    }

    /// Convert element to HTML
    fn element_to_html(&self, elem: &UiElement, indent: usize) -> String {
        let indent_str = "    ".repeat(indent);
        let tag = self.element_to_tag(elem);
        let text = elem.text.as_deref().unwrap_or("");

        if elem.children.is_empty() {
            if text.is_empty() {
                format!("{}<{} />\n", indent_str, tag)
            } else {
                format!("{}<{}>{}</{}>\n", indent_str, tag, text, tag)
            }
        } else {
            let mut html = format!("{}<{}>\n", indent_str, tag);
            if !text.is_empty() {
                html.push_str(&format!("{}    {}\n", indent_str, text));
            }
            for child in &elem.children {
                html.push_str(&self.element_to_html(child, indent + 1));
            }
            html.push_str(&format!("{}</{}>\n", indent_str, tag));
            html
        }
    }

    /// Get HTML tag for element type
    fn element_to_tag(&self, elem: &UiElement) -> &'static str {
        if self.use_semantic_html {
            match elem.element_type {
                UiElementType::Button => "button",
                UiElementType::TextInput => "input",
                UiElementType::TextArea => "textarea",
                UiElementType::Checkbox => "input type=\"checkbox\"",
                UiElementType::Link => "a",
                UiElementType::Image => "img",
                UiElementType::Heading => "h1",
                UiElementType::Paragraph => "p",
                UiElementType::List => "ul",
                UiElementType::Table => "table",
                UiElementType::Navbar => "nav",
                UiElementType::Footer => "footer",
                UiElementType::Form => "form",
                UiElementType::Card => "article",
                _ => "div",
            }
        } else {
            "div"
        }
    }

    /// Generate React component
    fn generate_react(&self, elements: &[UiElement]) -> GeneratedCode {
        let mut code = String::new();
        code.push_str("import React from 'react';\n\n");
        code.push_str("export default function Component() {\n");
        code.push_str("    return (\n");
        code.push_str("        <div>\n");

        for elem in elements {
            code.push_str(&self.element_to_jsx(elem, 3));
        }

        code.push_str("        </div>\n");
        code.push_str("    );\n");
        code.push_str("}\n");

        GeneratedCode::new(self.framework.clone(), code)
            .with_name("Component")
            .with_dependency("react")
    }

    /// Convert element to JSX
    fn element_to_jsx(&self, elem: &UiElement, indent: usize) -> String {
        let indent_str = "    ".repeat(indent);
        let tag = self.element_to_jsx_tag(elem);
        let text = elem.text.as_deref().unwrap_or("");

        if elem.children.is_empty() {
            if text.is_empty() {
                format!("{}<{} />\n", indent_str, tag)
            } else {
                format!("{}<{}>{}</{}>\n", indent_str, tag, text, tag)
            }
        } else {
            let mut jsx = format!("{}<{}>\n", indent_str, tag);
            if !text.is_empty() {
                jsx.push_str(&format!("{}    {{{}}}\n", indent_str, text));
            }
            for child in &elem.children {
                jsx.push_str(&self.element_to_jsx(child, indent + 1));
            }
            jsx.push_str(&format!("{}</{}>\n", indent_str, tag));
            jsx
        }
    }

    /// Get JSX tag for element type
    fn element_to_jsx_tag(&self, elem: &UiElement) -> &'static str {
        match elem.element_type {
            UiElementType::Button => "button",
            UiElementType::TextInput => "input",
            UiElementType::TextArea => "textarea",
            UiElementType::Checkbox => "input type=\"checkbox\"",
            UiElementType::Link => "a",
            UiElementType::Image => "img",
            UiElementType::Heading => "h1",
            UiElementType::Paragraph => "p",
            UiElementType::List => "ul",
            UiElementType::Table => "table",
            UiElementType::Navbar => "nav",
            UiElementType::Footer => "footer",
            UiElementType::Form => "form",
            _ => "div",
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Image Format Tests

    #[test]
    fn test_image_format_from_extension() {
        assert_eq!(ImageFormat::from_extension("png"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_extension("JPG"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_extension("jpeg"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_extension("xyz"), ImageFormat::Unknown);
    }

    #[test]
    fn test_image_format_mime_type() {
        assert_eq!(ImageFormat::Png.mime_type(), "image/png");
        assert_eq!(ImageFormat::Jpeg.mime_type(), "image/jpeg");
        assert_eq!(ImageFormat::Svg.mime_type(), "image/svg+xml");
    }

    // Image Dimensions Tests

    #[test]
    fn test_image_dimensions() {
        let dims = ImageDimensions::new(1920, 1080);
        assert_eq!(dims.width, 1920);
        assert_eq!(dims.height, 1080);
        assert!(dims.is_landscape());
        assert!(!dims.is_portrait());
        assert!(!dims.is_square());
    }

    #[test]
    fn test_image_dimensions_aspect_ratio() {
        let dims = ImageDimensions::new(1920, 1080);
        let ratio = dims.aspect_ratio();
        assert!((ratio - 16.0 / 9.0).abs() < 0.01);
    }

    #[test]
    fn test_image_dimensions_pixel_count() {
        let dims = ImageDimensions::new(100, 100);
        assert_eq!(dims.pixel_count(), 10000);
    }

    // Bounding Box Tests

    #[test]
    fn test_bounding_box_creation() {
        let bbox = BoundingBox::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(bbox.x, 10.0);
        assert_eq!(bbox.y, 20.0);
        assert_eq!(bbox.width, 100.0);
        assert_eq!(bbox.height, 50.0);
    }

    #[test]
    fn test_bounding_box_center() {
        let bbox = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        let (cx, cy) = bbox.center();
        assert_eq!(cx, 50.0);
        assert_eq!(cy, 50.0);
    }

    #[test]
    fn test_bounding_box_area() {
        let bbox = BoundingBox::new(0.0, 0.0, 10.0, 20.0);
        assert_eq!(bbox.area(), 200.0);
    }

    #[test]
    fn test_bounding_box_contains() {
        let bbox = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        assert!(bbox.contains(50.0, 50.0));
        assert!(bbox.contains(0.0, 0.0));
        assert!(bbox.contains(100.0, 100.0));
        assert!(!bbox.contains(150.0, 50.0));
    }

    #[test]
    fn test_bounding_box_intersects() {
        let a = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        let b = BoundingBox::new(50.0, 50.0, 100.0, 100.0);
        let c = BoundingBox::new(200.0, 200.0, 50.0, 50.0);

        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn test_bounding_box_union() {
        let a = BoundingBox::new(0.0, 0.0, 50.0, 50.0);
        let b = BoundingBox::new(25.0, 25.0, 50.0, 50.0);
        let union = a.union(&b);

        assert_eq!(union.x, 0.0);
        assert_eq!(union.y, 0.0);
        assert_eq!(union.width, 75.0);
        assert_eq!(union.height, 75.0);
    }

    // UI Element Tests

    #[test]
    fn test_ui_element_creation() {
        let elem = UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 40.0),
            0.95,
        )
        .with_text("Click me");

        assert_eq!(elem.element_type, UiElementType::Button);
        assert_eq!(elem.confidence, 0.95);
        assert_eq!(elem.text, Some("Click me".to_string()));
    }

    #[test]
    fn test_ui_element_is_interactive() {
        let button = UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 40.0),
            0.95,
        );
        let label = UiElement::new(
            UiElementType::Label,
            BoundingBox::new(0.0, 0.0, 100.0, 20.0),
            0.95,
        );

        assert!(button.is_interactive());
        assert!(!label.is_interactive());
    }

    // UI Bug Tests

    #[test]
    fn test_ui_bug_creation() {
        let bug = UiBug::new(UiBugType::Overlap, BugSeverity::Medium, "Elements overlap")
            .at_location(BoundingBox::new(0.0, 0.0, 100.0, 100.0))
            .with_fix("Adjust margins");

        assert_eq!(bug.bug_type, UiBugType::Overlap);
        assert_eq!(bug.severity, BugSeverity::Medium);
        assert!(bug.location.is_some());
        assert!(bug.suggested_fix.is_some());
    }

    #[test]
    fn test_bug_severity_ordering() {
        assert!(BugSeverity::Critical > BugSeverity::High);
        assert!(BugSeverity::High > BugSeverity::Medium);
        assert!(BugSeverity::Medium > BugSeverity::Low);
        assert!(BugSeverity::Low > BugSeverity::Info);
    }

    // Screenshot Analyzer Tests

    #[test]
    fn test_screenshot_analyzer_creation() {
        let analyzer = ScreenshotAnalyzer::new()
            .with_overlap_threshold(0.2)
            .with_spacing_tolerance(8.0);

        assert_eq!(analyzer.overlap_threshold, 0.2);
        assert_eq!(analyzer.spacing_tolerance, 8.0);
    }

    #[test]
    fn test_detect_overlaps() {
        let analyzer = ScreenshotAnalyzer::new();

        let elements = vec![
            UiElement::new(
                UiElementType::Button,
                BoundingBox::new(0.0, 0.0, 100.0, 50.0),
                0.95,
            ),
            UiElement::new(
                UiElementType::Button,
                BoundingBox::new(50.0, 0.0, 100.0, 50.0),
                0.95,
            ),
        ];

        let bugs = analyzer.detect_overlaps(&elements);
        assert!(!bugs.is_empty());
        assert_eq!(bugs[0].bug_type, UiBugType::Overlap);
    }

    #[test]
    fn test_analyze_screenshot() {
        let analyzer = ScreenshotAnalyzer::new();
        let elements = vec![UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 50.0),
            0.95,
        )];

        let analysis = analyzer.analyze(&elements);
        assert!(!analysis.id.is_empty());
        assert_eq!(analysis.elements.len(), 1);
    }

    #[test]
    fn test_analysis_summary() {
        let analyzer = ScreenshotAnalyzer::new();
        let elements = vec![
            UiElement::new(
                UiElementType::Button,
                BoundingBox::new(0.0, 0.0, 100.0, 50.0),
                0.95,
            ),
            UiElement::new(
                UiElementType::Button,
                BoundingBox::new(80.0, 0.0, 100.0, 50.0),
                0.95,
            ),
        ];

        let analysis = analyzer.analyze(&elements);
        let summary = analysis.summary();

        assert_eq!(summary.total_elements, 2);
    }

    // Diagram Tests

    #[test]
    fn test_diagram_node_creation() {
        let node = DiagramNode::new("n1", "Start")
            .with_shape("circle")
            .at_position(100.0, 100.0);

        assert_eq!(node.id, "n1");
        assert_eq!(node.label, "Start");
        assert_eq!(node.shape, "circle");
        assert_eq!(node.position, Some((100.0, 100.0)));
    }

    #[test]
    fn test_diagram_edge_creation() {
        let edge = DiagramEdge::new("n1", "n2")
            .with_label("next")
            .with_type("dashed");

        assert_eq!(edge.from, "n1");
        assert_eq!(edge.to, "n2");
        assert_eq!(edge.label, Some("next".to_string()));
        assert_eq!(edge.edge_type, "dashed");
    }

    #[test]
    fn test_parsed_diagram() {
        let diagram = ParsedDiagram::new(DiagramType::Flowchart)
            .with_title("Process Flow")
            .with_node(DiagramNode::new("start", "Start"))
            .with_node(DiagramNode::new("end", "End"))
            .with_edge(DiagramEdge::new("start", "end"));

        assert_eq!(diagram.diagram_type, DiagramType::Flowchart);
        assert_eq!(diagram.nodes.len(), 2);
        assert_eq!(diagram.edges.len(), 1);
    }

    #[test]
    fn test_diagram_to_mermaid() {
        let diagram = ParsedDiagram::new(DiagramType::Flowchart)
            .with_node(DiagramNode::new("A", "Start"))
            .with_node(DiagramNode::new("B", "End"))
            .with_edge(DiagramEdge::new("A", "B"));

        let mermaid = diagram.to_mermaid();
        assert!(mermaid.contains("flowchart TD"));
        assert!(mermaid.contains("A[Start]"));
        assert!(mermaid.contains("B[End]"));
        assert!(mermaid.contains("A --> B"));
    }

    #[test]
    fn test_diagram_to_plantuml() {
        let diagram = ParsedDiagram::new(DiagramType::Flowchart).with_title("Test Diagram");

        let plantuml = diagram.to_plantuml();
        assert!(plantuml.contains("@startuml"));
        assert!(plantuml.contains("title Test Diagram"));
        assert!(plantuml.contains("@enduml"));
    }

    // Visual Diff Tests

    #[test]
    fn test_visual_diff_added() {
        let diff = VisualDiff::added(BoundingBox::new(0.0, 0.0, 100.0, 100.0), "New button added");

        assert_eq!(diff.diff_type, DiffType::Added);
        assert!(diff.before_region.is_none());
        assert!(diff.after_region.is_some());
    }

    #[test]
    fn test_visual_diff_removed() {
        let diff = VisualDiff::removed(BoundingBox::new(0.0, 0.0, 100.0, 100.0), "Button removed");

        assert_eq!(diff.diff_type, DiffType::Removed);
        assert!(diff.before_region.is_some());
        assert!(diff.after_region.is_none());
    }

    #[test]
    fn test_diff_comparator() {
        let comparator = VisualDiffComparator::new()
            .with_tolerance(20)
            .with_min_region(50);

        assert_eq!(comparator.color_tolerance, 20);
        assert_eq!(comparator.min_region_size, 50);
    }

    #[test]
    fn test_compare_identical_elements() {
        let comparator = VisualDiffComparator::new();

        let elements = vec![UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 50.0),
            0.95,
        )
        .with_text("Click")];

        let result = comparator.compare_elements(&elements, &elements);
        assert!(result.is_identical());
    }

    #[test]
    fn test_compare_different_elements() {
        let comparator = VisualDiffComparator::new();

        let before = vec![UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 50.0),
            0.95,
        )
        .with_text("Old")];

        let after = vec![UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 50.0),
            0.95,
        )
        .with_text("New")];

        let result = comparator.compare_elements(&before, &after);
        assert!(!result.diffs.is_empty());
    }

    #[test]
    fn test_diff_result_percentage() {
        let result = DiffResult::new(Vec::new(), 0.9, 1000, 100);
        assert_eq!(result.diff_percentage(), 10.0);
    }

    // Image-to-Code Tests

    #[test]
    fn test_code_generator_html() {
        let generator = ImageToCodeGenerator::new(CodeFramework::Html);
        let elements = vec![UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 50.0),
            0.95,
        )
        .with_text("Submit")];

        let code = generator.generate(&elements);
        assert!(code.code.contains("<button>"));
        assert!(code.code.contains("Submit"));
        assert!(code.code.contains("</button>"));
    }

    #[test]
    fn test_code_generator_react() {
        let generator = ImageToCodeGenerator::new(CodeFramework::React);
        let elements = vec![UiElement::new(
            UiElementType::Button,
            BoundingBox::new(0.0, 0.0, 100.0, 50.0),
            0.95,
        )
        .with_text("Click")];

        let code = generator.generate(&elements);
        assert!(code.code.contains("import React"));
        assert!(code.code.contains("<button>"));
    }

    #[test]
    fn test_generated_code_with_dependencies() {
        let code = GeneratedCode::new(CodeFramework::React, "const App = () => {};")
            .with_name("App")
            .with_dependency("react")
            .with_styles(".app { color: red; }");

        assert_eq!(code.component_name, Some("App".to_string()));
        assert_eq!(code.dependencies.len(), 1);
        assert!(code.styles.is_some());
    }

    // Image Metadata Tests

    #[test]
    fn test_image_metadata_builder() {
        let metadata = ImageMetadata::new(ImageFormat::Png)
            .with_dimensions(800, 600)
            .with_file_size(1024)
            .with_color_depth(24)
            .with_alpha()
            .with_exif("camera", "Test Camera");

        assert_eq!(metadata.format, ImageFormat::Png);
        assert_eq!(metadata.dimensions.unwrap().width, 800);
        assert_eq!(metadata.file_size, Some(1024));
        assert!(metadata.has_alpha);
        assert!(metadata.exif_data.contains_key("camera"));
    }

    // Diagram Type Tests

    #[test]
    fn test_diagram_type_as_str() {
        assert_eq!(DiagramType::Flowchart.as_str(), "flowchart");
        assert_eq!(DiagramType::SequenceDiagram.as_str(), "sequence_diagram");
        assert_eq!(DiagramType::ClassDiagram.as_str(), "class_diagram");
    }

    // Code Framework Tests

    #[test]
    fn test_code_framework_as_str() {
        assert_eq!(CodeFramework::React.as_str(), "react");
        assert_eq!(CodeFramework::Vue.as_str(), "vue");
        assert_eq!(CodeFramework::Tailwind.as_str(), "tailwind");
    }

    // Unique ID Tests

    #[test]
    fn test_unique_analysis_ids() {
        let id1 = generate_analysis_id();
        let id2 = generate_analysis_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_ui_element_type_as_str() {
        assert_eq!(UiElementType::Button.as_str(), "button");
        assert_eq!(UiElementType::TextInput.as_str(), "text_input");
        assert_eq!(
            UiElementType::Unknown("custom".to_string()).as_str(),
            "custom"
        );
    }

    #[test]
    fn test_ui_bug_type_as_str() {
        assert_eq!(UiBugType::Overlap.as_str(), "overlap");
        assert_eq!(UiBugType::Contrast.as_str(), "contrast");
        assert_eq!(UiBugType::Custom("test".to_string()).as_str(), "test");
    }

    #[test]
    fn test_diff_type_as_str() {
        assert_eq!(DiffType::Added.as_str(), "added");
        assert_eq!(DiffType::Removed.as_str(), "removed");
        assert_eq!(DiffType::Modified.as_str(), "modified");
    }

    #[test]
    fn test_bug_severity_as_str() {
        assert_eq!(BugSeverity::Info.as_str(), "info");
        assert_eq!(BugSeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_image_dimensions_square() {
        let square = ImageDimensions::new(100, 100);
        assert!(square.is_square());
        assert!(!square.is_landscape());
        assert!(!square.is_portrait());
    }

    #[test]
    fn test_image_dimensions_portrait() {
        let portrait = ImageDimensions::new(100, 200);
        assert!(portrait.is_portrait());
        assert!(!portrait.is_landscape());
    }

    #[test]
    fn test_diagram_interpreter_detection() {
        let interpreter = DiagramInterpreter::new();
        let elements: Vec<UiElement> = Vec::new();
        let diagram_type = interpreter.detect_type(&elements);
        assert_eq!(diagram_type, DiagramType::Unknown);
    }

    #[test]
    fn test_diff_result_is_similar() {
        let result = DiffResult::new(Vec::new(), 0.95, 1000, 50);
        assert!(result.is_similar(0.9));
        assert!(!result.is_similar(0.99));
    }
}
