//! Security Scanner
//!
//! Security analysis capabilities:
//! - Secret detection (API keys, passwords, tokens)
//! - Vulnerability detection (SAST-style)
//! - Dependency auditing
//! - Compliance checking

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Severity of a security finding
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecuritySeverity {
    /// Informational finding
    Info,
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

impl SecuritySeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// CVSS-like score
    pub fn score(&self) -> f32 {
        match self {
            Self::Info => 0.0,
            Self::Low => 3.0,
            Self::Medium => 5.5,
            Self::High => 7.5,
            Self::Critical => 9.5,
        }
    }
}

/// Category of security finding
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SecurityCategory {
    /// Hardcoded secrets
    HardcodedSecret,
    /// Injection vulnerability
    Injection,
    /// Authentication issue
    Authentication,
    /// Authorization issue
    Authorization,
    /// Data exposure
    DataExposure,
    /// Cryptographic weakness
    Cryptography,
    /// Configuration issue
    Configuration,
    /// Vulnerable dependency
    Dependency,
    /// Compliance violation
    Compliance,
    /// Code quality security issue
    CodeQuality,
    /// Custom category
    Custom(String),
}

impl SecurityCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::HardcodedSecret => "hardcoded_secret",
            Self::Injection => "injection",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::DataExposure => "data_exposure",
            Self::Cryptography => "cryptography",
            Self::Configuration => "configuration",
            Self::Dependency => "dependency",
            Self::Compliance => "compliance",
            Self::CodeQuality => "code_quality",
            Self::Custom(s) => s,
        }
    }
}

/// A security finding
#[derive(Debug, Clone)]
pub struct SecurityFinding {
    /// Finding ID
    pub id: String,
    /// Title
    pub title: String,
    /// Description
    pub description: String,
    /// Category
    pub category: SecurityCategory,
    /// Severity
    pub severity: SecuritySeverity,
    /// File path
    pub file: Option<PathBuf>,
    /// Line number
    pub line: Option<u32>,
    /// Code snippet
    pub snippet: Option<String>,
    /// Remediation advice
    pub remediation: Option<String>,
    /// CWE ID
    pub cwe: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

impl SecurityFinding {
    pub fn new(title: &str, category: SecurityCategory, severity: SecuritySeverity) -> Self {
        let id = format!(
            "SEC-{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
        );
        Self {
            id,
            title: title.to_string(),
            description: String::new(),
            category,
            severity,
            file: None,
            line: None,
            snippet: None,
            remediation: None,
            cwe: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn with_location(mut self, file: PathBuf, line: u32) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self
    }

    pub fn with_snippet(mut self, snippet: &str) -> Self {
        self.snippet = Some(snippet.to_string());
        self
    }

    pub fn with_remediation(mut self, remediation: &str) -> Self {
        self.remediation = Some(remediation.to_string());
        self
    }

    pub fn with_cwe(mut self, cwe: &str) -> Self {
        self.cwe = Some(cwe.to_string());
        self
    }
}

/// Pattern for detecting secrets
#[derive(Debug, Clone)]
pub struct SecretPattern {
    /// Pattern name
    pub name: String,
    /// Regex pattern
    pub pattern: String,
    /// Severity
    pub severity: SecuritySeverity,
    /// Description
    pub description: String,
}

impl SecretPattern {
    pub fn new(name: &str, pattern: &str, severity: SecuritySeverity) -> Self {
        Self {
            name: name.to_string(),
            pattern: pattern.to_string(),
            severity,
            description: format!("Potential {} detected", name),
        }
    }
}

/// Scanner for hardcoded secrets
pub struct SecretScanner {
    /// Secret patterns
    patterns: Vec<SecretPattern>,
    /// Files to skip
    _skip_files: Vec<String>,
    /// Findings
    findings: Vec<SecurityFinding>,
}

impl SecretScanner {
    pub fn new() -> Self {
        Self {
            patterns: Self::default_patterns(),
            _skip_files: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".env.example".to_string(),
            ],
            findings: Vec::new(),
        }
    }

    fn default_patterns() -> Vec<SecretPattern> {
        vec![
            SecretPattern::new(
                "AWS Access Key",
                r"AKIA[0-9A-Z]{16}",
                SecuritySeverity::Critical,
            ),
            SecretPattern::new(
                "AWS Secret Key",
                r#"(?i)aws(.{0,20})?['"][0-9a-zA-Z/+]{40}['"]"#,
                SecuritySeverity::Critical,
            ),
            SecretPattern::new(
                "GitHub Token",
                r"gh[pousr]_[A-Za-z0-9_]{36,}",
                SecuritySeverity::Critical,
            ),
            SecretPattern::new(
                "Generic API Key",
                r#"(?i)(api[_-]?key|apikey)['"]?\s*[:=]\s*['"][a-zA-Z0-9]{20,}['"]"#,
                SecuritySeverity::High,
            ),
            SecretPattern::new(
                "Private Key",
                r"-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
                SecuritySeverity::Critical,
            ),
            SecretPattern::new(
                "Password in Code",
                r#"(?i)(password|passwd|pwd)['"]?\s*[:=]\s*['"][^'"]{8,}['"]"#,
                SecuritySeverity::High,
            ),
            SecretPattern::new(
                "Bearer Token",
                r"(?i)bearer\s+[a-zA-Z0-9_\-\.]+",
                SecuritySeverity::High,
            ),
            SecretPattern::new(
                "JWT Token",
                r"eyJ[a-zA-Z0-9_-]*\.eyJ[a-zA-Z0-9_-]*\.[a-zA-Z0-9_-]*",
                SecuritySeverity::High,
            ),
            SecretPattern::new(
                "Database URL",
                r"(?i)(postgres|mysql|mongodb)://[^:]+:[^@]+@",
                SecuritySeverity::High,
            ),
            SecretPattern::new(
                "Slack Token",
                r"xox[baprs]-[0-9A-Za-z]{10,}",
                SecuritySeverity::High,
            ),
        ]
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, pattern: SecretPattern) {
        self.patterns.push(pattern);
    }

    /// Scan content for secrets
    pub fn scan_content(&mut self, content: &str, file: Option<&PathBuf>) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            // Skip comments
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("/*") {
                continue;
            }

            for pattern in &self.patterns {
                if let Ok(re) = regex::Regex::new(&pattern.pattern) {
                    for mat in re.find_iter(line) {
                        let mut finding = SecurityFinding::new(
                            &pattern.name,
                            SecurityCategory::HardcodedSecret,
                            pattern.severity,
                        )
                        .with_description(&pattern.description)
                        .with_cwe("CWE-798");

                        if let Some(f) = file {
                            finding = finding.with_location(f.clone(), (line_num + 1) as u32);
                        }

                        // Mask the secret in snippet
                        let masked = Self::mask_secret(mat.as_str());
                        finding = finding.with_snippet(&masked);
                        finding = finding.with_remediation(
                            "Remove hardcoded secret and use environment variables or a secrets manager"
                        );

                        findings.push(finding.clone());
                        self.findings.push(finding);
                    }
                }
            }
        }

        findings
    }

    /// Mask a secret for safe display
    fn mask_secret(secret: &str) -> String {
        if secret.len() <= 8 {
            "*".repeat(secret.len())
        } else {
            format!("{}...{}", &secret[..4], "*".repeat(secret.len() - 4))
        }
    }

    /// Get all findings
    pub fn findings(&self) -> &[SecurityFinding] {
        &self.findings
    }

    /// Clear findings
    pub fn clear(&mut self) {
        self.findings.clear();
    }
}

impl Default for SecretScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// SAST-style vulnerability pattern
#[derive(Debug, Clone)]
pub struct VulnerabilityPattern {
    /// Pattern ID
    pub id: String,
    /// Name
    pub name: String,
    /// Language
    pub language: String,
    /// Pattern to match
    pub pattern: String,
    /// Severity
    pub severity: SecuritySeverity,
    /// CWE ID
    pub cwe: String,
    /// Description
    pub description: String,
    /// Remediation
    pub remediation: String,
}

/// Detector for code vulnerabilities
pub struct VulnerabilityDetector {
    /// Vulnerability patterns
    patterns: Vec<VulnerabilityPattern>,
    /// Findings
    findings: Vec<SecurityFinding>,
}

impl VulnerabilityDetector {
    pub fn new() -> Self {
        Self {
            patterns: Self::default_patterns(),
            findings: Vec::new(),
        }
    }

    fn default_patterns() -> Vec<VulnerabilityPattern> {
        vec![
            VulnerabilityPattern {
                id: "RUST001".to_string(),
                name: "Unsafe Block".to_string(),
                language: "rust".to_string(),
                pattern: r"unsafe\s*\{".to_string(),
                severity: SecuritySeverity::Medium,
                cwe: "CWE-242".to_string(),
                description: "Unsafe block found - requires careful review".to_string(),
                remediation: "Document safety invariants or use safe alternatives".to_string(),
            },
            VulnerabilityPattern {
                id: "RUST002".to_string(),
                name: "Unwrap on Result/Option".to_string(),
                language: "rust".to_string(),
                pattern: r"\.unwrap\(\)".to_string(),
                severity: SecuritySeverity::Low,
                cwe: "CWE-252".to_string(),
                description: "Unwrap can panic on None/Err".to_string(),
                remediation: "Use proper error handling with ? or match".to_string(),
            },
            VulnerabilityPattern {
                id: "RUST003".to_string(),
                name: "SQL Query String Building".to_string(),
                language: "rust".to_string(),
                pattern: r#"format!\s*\(\s*["']SELECT|format!\s*\(\s*["']INSERT|format!\s*\(\s*["']UPDATE|format!\s*\(\s*["']DELETE"#.to_string(),
                severity: SecuritySeverity::High,
                cwe: "CWE-89".to_string(),
                description: "Potential SQL injection vulnerability".to_string(),
                remediation: "Use parameterized queries instead of string formatting".to_string(),
            },
            VulnerabilityPattern {
                id: "RUST004".to_string(),
                name: "Command Injection".to_string(),
                language: "rust".to_string(),
                pattern: r"Command::new\s*\(\s*&?format!".to_string(),
                severity: SecuritySeverity::Critical,
                cwe: "CWE-78".to_string(),
                description: "Potential command injection vulnerability".to_string(),
                remediation: "Use static command strings or validate/sanitize input".to_string(),
            },
            VulnerabilityPattern {
                id: "RUST005".to_string(),
                name: "Path Traversal".to_string(),
                language: "rust".to_string(),
                pattern: r#"Path::new\s*\(\s*&?format!|PathBuf::from\s*\(\s*&?format!"#.to_string(),
                severity: SecuritySeverity::High,
                cwe: "CWE-22".to_string(),
                description: "Potential path traversal vulnerability".to_string(),
                remediation: "Validate paths and use canonicalize()".to_string(),
            },
            VulnerabilityPattern {
                id: "JS001".to_string(),
                name: "eval() Usage".to_string(),
                language: "javascript".to_string(),
                pattern: r"\beval\s*\(".to_string(),
                severity: SecuritySeverity::Critical,
                cwe: "CWE-95".to_string(),
                description: "eval() can execute arbitrary code".to_string(),
                remediation: "Avoid eval() - use safer alternatives".to_string(),
            },
            VulnerabilityPattern {
                id: "JS002".to_string(),
                name: "innerHTML Assignment".to_string(),
                language: "javascript".to_string(),
                pattern: r"\.innerHTML\s*=".to_string(),
                severity: SecuritySeverity::High,
                cwe: "CWE-79".to_string(),
                description: "Potential XSS via innerHTML".to_string(),
                remediation: "Use textContent or sanitize HTML".to_string(),
            },
            VulnerabilityPattern {
                id: "PY001".to_string(),
                name: "Python exec/eval".to_string(),
                language: "python".to_string(),
                pattern: r"\b(exec|eval)\s*\(".to_string(),
                severity: SecuritySeverity::Critical,
                cwe: "CWE-95".to_string(),
                description: "exec/eval can execute arbitrary code".to_string(),
                remediation: "Avoid exec/eval with untrusted input".to_string(),
            },
            VulnerabilityPattern {
                id: "PY002".to_string(),
                name: "Python pickle".to_string(),
                language: "python".to_string(),
                pattern: r"pickle\.(load|loads)\s*\(".to_string(),
                severity: SecuritySeverity::High,
                cwe: "CWE-502".to_string(),
                description: "pickle can deserialize malicious code".to_string(),
                remediation: "Use json or other safe serialization formats".to_string(),
            },
        ]
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, pattern: VulnerabilityPattern) {
        self.patterns.push(pattern);
    }

    /// Scan content for vulnerabilities
    pub fn scan_content(
        &mut self,
        content: &str,
        file: Option<&PathBuf>,
        language: &str,
    ) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        // Filter patterns for this language
        let applicable: Vec<_> = self
            .patterns
            .iter()
            .filter(|p| p.language == language || p.language == "*")
            .collect();

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &applicable {
                if let Ok(re) = regex::Regex::new(&pattern.pattern) {
                    if re.is_match(line) {
                        let mut finding = SecurityFinding::new(
                            &pattern.name,
                            SecurityCategory::CodeQuality,
                            pattern.severity,
                        )
                        .with_description(&pattern.description)
                        .with_cwe(&pattern.cwe)
                        .with_remediation(&pattern.remediation)
                        .with_snippet(line.trim());

                        if let Some(f) = file {
                            finding = finding.with_location(f.clone(), (line_num + 1) as u32);
                        }

                        findings.push(finding.clone());
                        self.findings.push(finding);
                    }
                }
            }
        }

        findings
    }

    /// Get all findings
    pub fn findings(&self) -> &[SecurityFinding] {
        &self.findings
    }

    /// Clear findings
    pub fn clear(&mut self) {
        self.findings.clear();
    }
}

impl Default for VulnerabilityDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// A dependency with security info
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Package name
    pub name: String,
    /// Version
    pub version: String,
    /// Source (crates.io, npm, pypi, etc.)
    pub source: String,
    /// Known vulnerabilities
    pub vulnerabilities: Vec<KnownVulnerability>,
}

impl Dependency {
    pub fn new(name: &str, version: &str, source: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            source: source.to_string(),
            vulnerabilities: Vec::new(),
        }
    }

    pub fn is_vulnerable(&self) -> bool {
        !self.vulnerabilities.is_empty()
    }

    pub fn max_severity(&self) -> Option<SecuritySeverity> {
        self.vulnerabilities.iter().map(|v| v.severity).max()
    }
}

/// A known vulnerability in a dependency
#[derive(Debug, Clone)]
pub struct KnownVulnerability {
    /// CVE or advisory ID
    pub id: String,
    /// Severity
    pub severity: SecuritySeverity,
    /// Description
    pub description: String,
    /// Fixed version
    pub fixed_version: Option<String>,
    /// URL for more info
    pub url: Option<String>,
}

impl KnownVulnerability {
    pub fn new(id: &str, severity: SecuritySeverity, description: &str) -> Self {
        Self {
            id: id.to_string(),
            severity,
            description: description.to_string(),
            fixed_version: None,
            url: None,
        }
    }

    pub fn with_fixed_version(mut self, version: &str) -> Self {
        self.fixed_version = Some(version.to_string());
        self
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self
    }
}

/// Auditor for dependencies
pub struct DependencyAuditor {
    /// Known vulnerable packages (simplified for demo)
    vulnerability_db: HashMap<String, Vec<KnownVulnerability>>,
    /// Scanned dependencies
    dependencies: Vec<Dependency>,
    /// Findings
    findings: Vec<SecurityFinding>,
}

impl DependencyAuditor {
    pub fn new() -> Self {
        Self {
            vulnerability_db: Self::default_db(),
            dependencies: Vec::new(),
            findings: Vec::new(),
        }
    }

    fn default_db() -> HashMap<String, Vec<KnownVulnerability>> {
        let mut db = HashMap::new();

        // Example known vulnerabilities (in practice, this would be populated from a real database)
        db.insert(
            "lodash".to_string(),
            vec![KnownVulnerability::new(
                "CVE-2021-23337",
                SecuritySeverity::High,
                "Command Injection in lodash",
            )
            .with_fixed_version("4.17.21")],
        );

        db.insert(
            "log4j".to_string(),
            vec![KnownVulnerability::new(
                "CVE-2021-44228",
                SecuritySeverity::Critical,
                "Log4Shell RCE vulnerability",
            )
            .with_fixed_version("2.17.0")],
        );

        db
    }

    /// Add a vulnerability to the database
    pub fn add_vulnerability(&mut self, package: &str, vuln: KnownVulnerability) {
        self.vulnerability_db
            .entry(package.to_string())
            .or_default()
            .push(vuln);
    }

    /// Audit a dependency
    pub fn audit_dependency(&mut self, name: &str, version: &str, source: &str) -> Dependency {
        let mut dep = Dependency::new(name, version, source);

        // Check vulnerability database
        if let Some(vulns) = self.vulnerability_db.get(name) {
            for vuln in vulns {
                // In practice, would check version ranges
                dep.vulnerabilities.push(vuln.clone());

                let finding = SecurityFinding::new(
                    &format!("Vulnerable dependency: {}", name),
                    SecurityCategory::Dependency,
                    vuln.severity,
                )
                .with_description(&vuln.description)
                .with_remediation(&format!(
                    "Update {} to version {}",
                    name,
                    vuln.fixed_version.as_deref().unwrap_or("latest")
                ));

                self.findings.push(finding);
            }
        }

        self.dependencies.push(dep.clone());
        dep
    }

    /// Get vulnerable dependencies
    pub fn vulnerable_dependencies(&self) -> Vec<&Dependency> {
        self.dependencies
            .iter()
            .filter(|d| d.is_vulnerable())
            .collect()
    }

    /// Get all findings
    pub fn findings(&self) -> &[SecurityFinding] {
        &self.findings
    }

    /// Clear
    pub fn clear(&mut self) {
        self.dependencies.clear();
        self.findings.clear();
    }
}

impl Default for DependencyAuditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Compliance rule
#[derive(Debug, Clone)]
pub struct ComplianceRule {
    /// Rule ID
    pub id: String,
    /// Standard (OWASP, PCI-DSS, HIPAA, etc.)
    pub standard: String,
    /// Description
    pub description: String,
    /// Check function (simplified as pattern)
    pub pattern: Option<String>,
    /// Severity
    pub severity: SecuritySeverity,
}

impl ComplianceRule {
    pub fn new(id: &str, standard: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            standard: standard.to_string(),
            description: description.to_string(),
            pattern: None,
            severity: SecuritySeverity::Medium,
        }
    }

    pub fn with_pattern(mut self, pattern: &str) -> Self {
        self.pattern = Some(pattern.to_string());
        self
    }

    pub fn with_severity(mut self, severity: SecuritySeverity) -> Self {
        self.severity = severity;
        self
    }
}

/// Checker for compliance rules
pub struct ComplianceChecker {
    /// Compliance rules
    rules: Vec<ComplianceRule>,
    /// Findings
    findings: Vec<SecurityFinding>,
}

impl ComplianceChecker {
    pub fn new() -> Self {
        Self {
            rules: Self::default_rules(),
            findings: Vec::new(),
        }
    }

    fn default_rules() -> Vec<ComplianceRule> {
        vec![
            ComplianceRule::new("OWASP-A01", "OWASP Top 10", "Broken Access Control")
                .with_severity(SecuritySeverity::High),
            ComplianceRule::new("OWASP-A02", "OWASP Top 10", "Cryptographic Failures")
                .with_pattern(r"(?i)(md5|sha1)\s*\(")
                .with_severity(SecuritySeverity::High),
            ComplianceRule::new("OWASP-A03", "OWASP Top 10", "Injection")
                .with_severity(SecuritySeverity::Critical),
            ComplianceRule::new("PCI-DSS-6.5.1", "PCI-DSS", "Address injection flaws")
                .with_severity(SecuritySeverity::High),
            ComplianceRule::new("HIPAA-164.312", "HIPAA", "Encryption of PHI at rest")
                .with_severity(SecuritySeverity::High),
        ]
    }

    /// Add a custom rule
    pub fn add_rule(&mut self, rule: ComplianceRule) {
        self.rules.push(rule);
    }

    /// Check content against rules with patterns
    pub fn check_content(&mut self, content: &str, file: Option<&PathBuf>) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            for rule in &self.rules {
                if let Some(pattern) = &rule.pattern {
                    if let Ok(re) = regex::Regex::new(pattern) {
                        if re.is_match(line) {
                            let mut finding = SecurityFinding::new(
                                &format!("{}: {}", rule.id, rule.description),
                                SecurityCategory::Compliance,
                                rule.severity,
                            )
                            .with_description(&format!("Potential {} violation", rule.standard))
                            .with_snippet(line.trim());

                            if let Some(f) = file {
                                finding = finding.with_location(f.clone(), (line_num + 1) as u32);
                            }

                            findings.push(finding.clone());
                            self.findings.push(finding);
                        }
                    }
                }
            }
        }

        findings
    }

    /// Get applicable standards
    pub fn standards(&self) -> Vec<String> {
        let mut standards: Vec<_> = self.rules.iter().map(|r| r.standard.clone()).collect();
        standards.sort();
        standards.dedup();
        standards
    }

    /// Get all findings
    pub fn findings(&self) -> &[SecurityFinding] {
        &self.findings
    }

    /// Clear
    pub fn clear(&mut self) {
        self.findings.clear();
    }
}

impl Default for ComplianceChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Security scan result
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// All findings
    pub findings: Vec<SecurityFinding>,
    /// Summary by severity
    pub by_severity: HashMap<SecuritySeverity, usize>,
    /// Summary by category
    pub by_category: HashMap<String, usize>,
    /// Scan duration (ms)
    pub duration_ms: u64,
    /// Files scanned
    pub files_scanned: usize,
    /// Lines scanned
    pub lines_scanned: usize,
}

impl ScanResult {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
            by_severity: HashMap::new(),
            by_category: HashMap::new(),
            duration_ms: 0,
            files_scanned: 0,
            lines_scanned: 0,
        }
    }

    /// Total finding count
    pub fn total_findings(&self) -> usize {
        self.findings.len()
    }

    /// Has critical findings?
    pub fn has_critical(&self) -> bool {
        self.by_severity
            .get(&SecuritySeverity::Critical)
            .is_some_and(|&c| c > 0)
    }

    /// Has high findings?
    pub fn has_high(&self) -> bool {
        self.by_severity
            .get(&SecuritySeverity::High)
            .is_some_and(|&c| c > 0)
    }

    /// Overall risk score
    pub fn risk_score(&self) -> f32 {
        self.findings.iter().map(|f| f.severity.score()).sum()
    }
}

impl Default for ScanResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Main security scanner
pub struct SecurityScanner {
    /// Secret scanner
    secret_scanner: RwLock<SecretScanner>,
    /// Vulnerability detector
    vuln_detector: RwLock<VulnerabilityDetector>,
    /// Dependency auditor
    dep_auditor: RwLock<DependencyAuditor>,
    /// Compliance checker
    compliance_checker: RwLock<ComplianceChecker>,
    /// Scan history
    scan_history: RwLock<Vec<ScanResult>>,
}

impl SecurityScanner {
    pub fn new() -> Self {
        Self {
            secret_scanner: RwLock::new(SecretScanner::new()),
            vuln_detector: RwLock::new(VulnerabilityDetector::new()),
            dep_auditor: RwLock::new(DependencyAuditor::new()),
            compliance_checker: RwLock::new(ComplianceChecker::new()),
            scan_history: RwLock::new(Vec::new()),
        }
    }

    /// Scan content for all security issues
    pub fn scan_content(
        &self,
        content: &str,
        file: Option<&PathBuf>,
        language: &str,
    ) -> ScanResult {
        let start = std::time::Instant::now();
        let mut result = ScanResult::new();

        // Scan for secrets
        if let Ok(mut scanner) = self.secret_scanner.write() {
            result.findings.extend(scanner.scan_content(content, file));
        }

        // Scan for vulnerabilities
        if let Ok(mut detector) = self.vuln_detector.write() {
            result
                .findings
                .extend(detector.scan_content(content, file, language));
        }

        // Check compliance
        if let Ok(mut checker) = self.compliance_checker.write() {
            result.findings.extend(checker.check_content(content, file));
        }

        // Calculate summaries
        for finding in &result.findings {
            *result.by_severity.entry(finding.severity).or_insert(0) += 1;
            *result
                .by_category
                .entry(finding.category.as_str().to_string())
                .or_insert(0) += 1;
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        result.files_scanned = 1;
        result.lines_scanned = content.lines().count();

        // Save to history
        if let Ok(mut history) = self.scan_history.write() {
            history.push(result.clone());
            if history.len() > 100 {
                history.remove(0);
            }
        }

        result
    }

    /// Audit a dependency
    pub fn audit_dependency(&self, name: &str, version: &str, source: &str) -> Dependency {
        if let Ok(mut auditor) = self.dep_auditor.write() {
            auditor.audit_dependency(name, version, source)
        } else {
            Dependency::new(name, version, source)
        }
    }

    /// Get scan statistics
    pub fn get_stats(&self) -> ScannerStats {
        let history = self.scan_history.read().ok();
        let total_scans = history.as_ref().map_or(0, |h| h.len());
        let total_findings: usize = history
            .as_ref()
            .map_or(0, |h| h.iter().map(|r| r.total_findings()).sum());

        ScannerStats {
            total_scans,
            total_findings,
            critical_findings: history.as_ref().map_or(0, |h| {
                h.iter()
                    .map(|r| {
                        r.by_severity
                            .get(&SecuritySeverity::Critical)
                            .copied()
                            .unwrap_or(0)
                    })
                    .sum()
            }),
            high_findings: history.as_ref().map_or(0, |h| {
                h.iter()
                    .map(|r| {
                        r.by_severity
                            .get(&SecuritySeverity::High)
                            .copied()
                            .unwrap_or(0)
                    })
                    .sum()
            }),
        }
    }

    /// Generate security report
    pub fn generate_report(&self, result: &ScanResult) -> String {
        let mut report = String::new();
        report.push_str("# Security Scan Report\n\n");

        report.push_str(&format!("- Files scanned: {}\n", result.files_scanned));
        report.push_str(&format!("- Lines scanned: {}\n", result.lines_scanned));
        report.push_str(&format!("- Scan duration: {}ms\n", result.duration_ms));
        report.push_str(&format!("- Total findings: {}\n", result.total_findings()));
        report.push_str(&format!("- Risk score: {:.1}\n\n", result.risk_score()));

        if result.has_critical() {
            report.push_str("## CRITICAL Findings\n");
            for finding in result
                .findings
                .iter()
                .filter(|f| f.severity == SecuritySeverity::Critical)
            {
                report.push_str(&format!(
                    "- **{}**: {}\n",
                    finding.title, finding.description
                ));
                if let Some(file) = &finding.file {
                    report.push_str(&format!(
                        "  Location: {}:{}\n",
                        file.display(),
                        finding.line.unwrap_or(0)
                    ));
                }
            }
            report.push('\n');
        }

        if result.has_high() {
            report.push_str("## HIGH Findings\n");
            for finding in result
                .findings
                .iter()
                .filter(|f| f.severity == SecuritySeverity::High)
            {
                report.push_str(&format!(
                    "- **{}**: {}\n",
                    finding.title, finding.description
                ));
            }
            report.push('\n');
        }

        report.push_str("## Summary by Category\n");
        for (cat, count) in &result.by_category {
            report.push_str(&format!("- {}: {}\n", cat, count));
        }

        report
    }
}

impl Default for SecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Scanner statistics
#[derive(Debug, Clone)]
pub struct ScannerStats {
    pub total_scans: usize,
    pub total_findings: usize,
    pub critical_findings: usize,
    pub high_findings: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_severity_ordering() {
        assert!(SecuritySeverity::Critical > SecuritySeverity::High);
        assert!(SecuritySeverity::High > SecuritySeverity::Medium);
        assert!(SecuritySeverity::Medium > SecuritySeverity::Low);
        assert!(SecuritySeverity::Low > SecuritySeverity::Info);
    }

    #[test]
    fn test_security_severity_score() {
        assert!(SecuritySeverity::Critical.score() > SecuritySeverity::High.score());
    }

    #[test]
    fn test_security_category_as_str() {
        assert_eq!(
            SecurityCategory::HardcodedSecret.as_str(),
            "hardcoded_secret"
        );
        assert_eq!(SecurityCategory::Injection.as_str(), "injection");
    }

    #[test]
    fn test_security_finding_new() {
        let finding = SecurityFinding::new(
            "Test Finding",
            SecurityCategory::HardcodedSecret,
            SecuritySeverity::High,
        );
        assert!(!finding.id.is_empty());
        assert!(finding.timestamp > 0);
    }

    #[test]
    fn test_security_finding_with_location() {
        let finding =
            SecurityFinding::new("Test", SecurityCategory::Injection, SecuritySeverity::High)
                .with_location(PathBuf::from("test.rs"), 42);
        assert_eq!(finding.line, Some(42));
    }

    #[test]
    fn test_secret_scanner_new() {
        let scanner = SecretScanner::new();
        assert!(scanner.findings().is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_aws_key() {
        let mut scanner = SecretScanner::new();
        let content = "aws_key = \"AKIAIOSFODNN7EXAMPLE\"";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_private_key() {
        let mut scanner = SecretScanner::new();
        let content = "-----BEGIN RSA PRIVATE KEY-----";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_skip_comment() {
        let mut scanner = SecretScanner::new();
        let content = "// apikey = 'AKIAIOSFODNN7EXAMPLE'";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_new() {
        let detector = VulnerabilityDetector::new();
        assert!(detector.findings().is_empty());
    }

    #[test]
    fn test_vulnerability_detector_rust_unsafe() {
        let mut detector = VulnerabilityDetector::new();
        let content = "unsafe { ptr::read(addr) }";
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_rust_unwrap() {
        let mut detector = VulnerabilityDetector::new();
        let content = "let x = option.unwrap();";
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_js_eval() {
        let mut detector = VulnerabilityDetector::new();
        let content = "eval(userInput)";
        let findings = detector.scan_content(content, None, "javascript");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_dependency_new() {
        let dep = Dependency::new("test-pkg", "1.0.0", "npm");
        assert_eq!(dep.name, "test-pkg");
        assert!(!dep.is_vulnerable());
    }

    #[test]
    fn test_known_vulnerability_new() {
        let vuln = KnownVulnerability::new("CVE-2021-1234", SecuritySeverity::High, "Test vuln");
        assert_eq!(vuln.id, "CVE-2021-1234");
    }

    #[test]
    fn test_dependency_auditor_new() {
        let auditor = DependencyAuditor::new();
        assert!(auditor.vulnerable_dependencies().is_empty());
    }

    #[test]
    fn test_dependency_auditor_known_vuln() {
        let mut auditor = DependencyAuditor::new();
        let dep = auditor.audit_dependency("lodash", "4.17.0", "npm");
        assert!(dep.is_vulnerable());
    }

    #[test]
    fn test_dependency_auditor_safe_dep() {
        let mut auditor = DependencyAuditor::new();
        let dep = auditor.audit_dependency("safe-package", "1.0.0", "npm");
        assert!(!dep.is_vulnerable());
    }

    #[test]
    fn test_compliance_rule_new() {
        let rule = ComplianceRule::new("R1", "OWASP", "Test rule");
        assert_eq!(rule.id, "R1");
        assert_eq!(rule.standard, "OWASP");
    }

    #[test]
    fn test_compliance_checker_new() {
        let checker = ComplianceChecker::new();
        assert!(!checker.standards().is_empty());
    }

    #[test]
    fn test_compliance_checker_weak_crypto() {
        let mut checker = ComplianceChecker::new();
        let content = "hash = md5(password)";
        let findings = checker.check_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_scan_result_new() {
        let result = ScanResult::new();
        assert_eq!(result.total_findings(), 0);
        assert!(!result.has_critical());
    }

    #[test]
    fn test_scan_result_risk_score() {
        let mut result = ScanResult::new();
        result.findings.push(SecurityFinding::new(
            "Test",
            SecurityCategory::Injection,
            SecuritySeverity::Critical,
        ));
        assert!(result.risk_score() > 0.0);
    }

    #[test]
    fn test_security_scanner_new() {
        let scanner = SecurityScanner::new();
        let stats = scanner.get_stats();
        assert_eq!(stats.total_scans, 0);
    }

    #[test]
    fn test_security_scanner_scan() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("let x = 1;", None, "rust");
        assert_eq!(result.files_scanned, 1);
    }

    #[test]
    fn test_security_scanner_detect_secret() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("api_key = \"AKIAIOSFODNN7EXAMPLE\"", None, "rust");
        assert!(result.total_findings() > 0);
    }

    #[test]
    fn test_security_scanner_audit_dependency() {
        let scanner = SecurityScanner::new();
        let dep = scanner.audit_dependency("lodash", "4.17.0", "npm");
        assert!(dep.is_vulnerable());
    }

    #[test]
    fn test_security_scanner_report() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("unsafe { }", None, "rust");
        let report = scanner.generate_report(&result);
        assert!(report.contains("Security Scan Report"));
    }

    #[test]
    fn test_secret_pattern_new() {
        let pattern = SecretPattern::new("Test", r"\d+", SecuritySeverity::Low);
        assert_eq!(pattern.name, "Test");
    }

    #[test]
    fn test_vulnerability_pattern() {
        let pattern = VulnerabilityPattern {
            id: "V1".to_string(),
            name: "Test".to_string(),
            language: "rust".to_string(),
            pattern: r"test".to_string(),
            severity: SecuritySeverity::Low,
            cwe: "CWE-1".to_string(),
            description: "Test".to_string(),
            remediation: "Fix it".to_string(),
        };
        assert_eq!(pattern.id, "V1");
    }

    #[test]
    fn test_scan_result_has_critical() {
        let mut result = ScanResult::new();
        result.by_severity.insert(SecuritySeverity::Critical, 1);
        assert!(result.has_critical());
    }

    #[test]
    fn test_scan_result_has_high() {
        let mut result = ScanResult::new();
        result.by_severity.insert(SecuritySeverity::High, 2);
        assert!(result.has_high());
    }
}
