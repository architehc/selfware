//! STRIDE Analyzer and Security Scanner
//!
//! Provides threat pattern detection and attack surface mapping capabilities.

use std::collections::HashMap;
use std::path::Path;

use super::types::*;

/// STRIDE analyzer
#[derive(Debug)]
pub struct StrideAnalyzer {
    /// Threat patterns
    patterns: HashMap<StrideCategory, Vec<ThreatPattern>>,
}

impl Default for StrideAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl StrideAnalyzer {
    /// Create a new analyzer with default patterns
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        // Spoofing patterns
        patterns.insert(
            StrideCategory::Spoofing,
            vec![
                ThreatPattern::new(
                    "Missing Authentication",
                    vec!["no auth", "unauthenticated", "anonymous"],
                ),
                ThreatPattern::new(
                    "Weak Authentication",
                    vec!["basic auth", "plaintext password"],
                ),
                ThreatPattern::new("Session Hijacking", vec!["session", "cookie", "token"]),
            ],
        );

        // Tampering patterns
        patterns.insert(
            StrideCategory::Tampering,
            vec![
                ThreatPattern::new(
                    "Missing Input Validation",
                    vec!["user input", "form", "request"],
                ),
                ThreatPattern::new("SQL Injection", vec!["sql", "query", "database"]),
                ThreatPattern::new("Command Injection", vec!["exec", "shell", "command"]),
            ],
        );

        // Repudiation patterns
        patterns.insert(
            StrideCategory::Repudiation,
            vec![
                ThreatPattern::new("Missing Audit Log", vec!["log", "audit", "track"]),
                ThreatPattern::new("No Transaction Records", vec!["transaction", "payment"]),
            ],
        );

        // Information Disclosure patterns
        patterns.insert(
            StrideCategory::InformationDisclosure,
            vec![
                ThreatPattern::new(
                    "Sensitive Data Exposure",
                    vec!["password", "secret", "key", "token"],
                ),
                ThreatPattern::new(
                    "Verbose Error Messages",
                    vec!["error", "exception", "stack trace"],
                ),
                ThreatPattern::new("Information Leakage", vec!["debug", "verbose", "print"]),
            ],
        );

        // Denial of Service patterns
        patterns.insert(
            StrideCategory::DenialOfService,
            vec![
                ThreatPattern::new("Resource Exhaustion", vec!["loop", "memory", "cpu"]),
                ThreatPattern::new("Missing Rate Limiting", vec!["api", "endpoint", "request"]),
            ],
        );

        // Elevation of Privilege patterns
        patterns.insert(
            StrideCategory::ElevationOfPrivilege,
            vec![
                ThreatPattern::new("Missing Authorization", vec!["admin", "role", "permission"]),
                ThreatPattern::new("Privilege Escalation", vec!["sudo", "root", "elevated"]),
            ],
        );

        Self { patterns }
    }

    /// Analyze code for threats
    pub fn analyze(&self, content: &str, file_path: &Path) -> Vec<Threat> {
        let mut threats = Vec::new();
        let lower_content = content.to_lowercase();
        let lines: Vec<&str> = content.lines().collect();

        for (category, category_patterns) in &self.patterns {
            for pattern in category_patterns {
                if pattern.matches(&lower_content) {
                    // Find line numbers where pattern matches
                    for (line_num, line) in lines.iter().enumerate() {
                        let lower_line = line.to_lowercase();
                        if pattern.keywords.iter().any(|kw| lower_line.contains(kw)) {
                            let threat = Threat::new(&pattern.name, *category)
                                .with_description(format!(
                                    "Potential {} vulnerability detected",
                                    pattern.name
                                ))
                                .with_source(file_path.to_path_buf(), line_num + 1);
                            threats.push(threat);
                            break; // One threat per pattern per file
                        }
                    }
                }
            }
        }

        threats
    }

    /// Get patterns for a category
    pub fn get_patterns(&self, category: StrideCategory) -> &[ThreatPattern] {
        self.patterns
            .get(&category)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Threat pattern for detection
#[derive(Debug, Clone)]
pub struct ThreatPattern {
    /// Pattern name
    pub name: String,
    /// Keywords to match
    pub keywords: Vec<String>,
}

impl ThreatPattern {
    /// Create a new pattern
    pub fn new(name: impl Into<String>, keywords: Vec<&str>) -> Self {
        Self {
            name: name.into(),
            keywords: keywords.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Check if pattern matches content
    pub fn matches(&self, content: &str) -> bool {
        self.keywords.iter().any(|kw| content.contains(kw))
    }
}

/// Attack surface mapper
#[derive(Debug)]
pub struct AttackSurfaceMapper {
    /// Entry point detectors
    detectors: Vec<EntryPointDetector>,
}

impl Default for AttackSurfaceMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl AttackSurfaceMapper {
    /// Create a new mapper
    pub fn new() -> Self {
        let detectors = vec![
            EntryPointDetector::new(
                EntryPointType::RestApi,
                vec![
                    "#[get",
                    "#[post",
                    "#[put",
                    "#[delete",
                    "app.get",
                    "app.post",
                    "router.get",
                    "HttpGet",
                    "HttpPost",
                    "@GetMapping",
                    "@PostMapping",
                ],
            ),
            EntryPointDetector::new(
                EntryPointType::GraphQL,
                vec!["graphql", "Query", "Mutation", "Resolver"],
            ),
            EntryPointDetector::new(
                EntryPointType::Database,
                vec!["query", "execute", "SELECT", "INSERT", "UPDATE", "DELETE"],
            ),
            EntryPointDetector::new(
                EntryPointType::FileUpload,
                vec!["upload", "multipart", "file_field", "save_file"],
            ),
            EntryPointDetector::new(
                EntryPointType::Cli,
                vec!["clap", "structopt", "argparse", "cli", "args"],
            ),
        ];

        Self { detectors }
    }

    /// Map attack surface from code
    pub fn map(&self, content: &str) -> Vec<EntryPoint> {
        let mut entry_points = Vec::new();

        for detector in &self.detectors {
            if let Some(entry) = detector.detect(content) {
                entry_points.push(entry);
            }
        }

        entry_points
    }
}

/// Entry point detector
#[derive(Debug)]
pub struct EntryPointDetector {
    /// Entry point type
    entry_type: EntryPointType,
    /// Patterns to detect
    patterns: Vec<String>,
}

impl EntryPointDetector {
    /// Create a new detector
    pub fn new(entry_type: EntryPointType, patterns: Vec<&str>) -> Self {
        Self {
            entry_type,
            patterns: patterns.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Detect entry point
    pub fn detect(&self, content: &str) -> Option<EntryPoint> {
        for pattern in &self.patterns {
            if content.contains(pattern) {
                return Some(EntryPoint::new(
                    format!("{} endpoint", self.entry_type),
                    self.entry_type,
                ));
            }
        }
        None
    }
}

/// Code scanner for security issues
#[derive(Debug)]
pub struct SecurityScanner {
    /// STRIDE analyzer
    stride_analyzer: StrideAnalyzer,
    /// Attack surface mapper
    surface_mapper: AttackSurfaceMapper,
}

impl Default for SecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityScanner {
    /// Create a new scanner
    pub fn new() -> Self {
        Self {
            stride_analyzer: StrideAnalyzer::new(),
            surface_mapper: AttackSurfaceMapper::new(),
        }
    }

    /// Scan a file
    pub fn scan_file(&self, content: &str, file_path: &Path) -> ScanResult {
        let threats = self.stride_analyzer.analyze(content, file_path);
        let entry_points = self.surface_mapper.map(content);

        ScanResult {
            file: file_path.to_path_buf(),
            threats,
            entry_points,
        }
    }
}

/// Scan result
#[derive(Debug)]
pub struct ScanResult {
    /// File scanned
    pub file: std::path::PathBuf,
    /// Threats found
    pub threats: Vec<Threat>,
    /// Entry points found
    pub entry_points: Vec<EntryPoint>,
}
