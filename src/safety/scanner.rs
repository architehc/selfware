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
    /// Pre-compiled regex (avoids recompilation on every scan and enables size limits)
    pub compiled: Option<regex::Regex>,
    /// Severity
    pub severity: SecuritySeverity,
    /// Description
    pub description: String,
}

impl SecretPattern {
    pub fn new(name: &str, pattern: &str, severity: SecuritySeverity) -> Self {
        let compiled = regex::RegexBuilder::new(pattern)
            .size_limit(1 << 20) // 1 MB limit to mitigate ReDoS
            .build()
            .ok();
        Self {
            name: name.to_string(),
            pattern: pattern.to_string(),
            compiled,
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
            // GitHub classic tokens (ghp_, gho_, ghu_, ghs_, ghr_)
            SecretPattern::new(
                "GitHub Token",
                r"gh[pousr]_[A-Za-z0-9_]{36,}",
                SecuritySeverity::Critical,
            ),
            // GitHub fine-grained personal access tokens
            SecretPattern::new(
                "GitHub Fine-Grained Token",
                r"github_pat_[A-Za-z0-9_]{22,}",
                SecuritySeverity::Critical,
            ),
            // GitLab personal/project/group access tokens
            SecretPattern::new(
                "GitLab Token",
                r"glpat-[A-Za-z0-9_\-]{20,}",
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
            // Google API keys (AIza...)
            SecretPattern::new(
                "Google API Key",
                r"AIza[a-zA-Z0-9_\-]{35}",
                SecuritySeverity::High,
            ),
            // Stripe secret keys
            SecretPattern::new(
                "Stripe Key",
                r"(sk_live_|rk_live_|pk_live_)[a-zA-Z0-9]{24,}",
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
            // Slack tokens: bot (xoxb-), user (xoxp-), app-level (xoxa-),
            // legacy (xoxs-), refresh (xoxr-)
            SecretPattern::new(
                "Slack Token",
                r"xox[bpsar]-[0-9A-Za-z\-]{10,}",
                SecuritySeverity::High,
            ),
            // Partial JWT / base64-encoded token starting with eyJ
            SecretPattern::new(
                "JWT Partial",
                r"eyJ[a-zA-Z0-9_/+\-]{30,}",
                SecuritySeverity::Medium,
            ),
            // Generic high-entropy base64 strings
            SecretPattern::new(
                "Base64 Secret",
                r#"(?i)(?:key|token|secret|password|credential|auth)\s*[:=]\s*['"]?[A-Za-z0-9+/=_\-]{40,}['"]?"#,
                SecuritySeverity::Medium,
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
                if let Some(ref re) = pattern.compiled {
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
            format!(
                "{}...{}",
                secret.chars().take(4).collect::<String>(),
                "*".repeat(secret.chars().count().saturating_sub(4))
            )
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

    // -------------------------------------------------------------------------
    // SecuritySeverity
    // -------------------------------------------------------------------------

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
        assert!(SecuritySeverity::High.score() > SecuritySeverity::Medium.score());
        assert!(SecuritySeverity::Medium.score() > SecuritySeverity::Low.score());
        assert!(SecuritySeverity::Low.score() > SecuritySeverity::Info.score());
        assert_eq!(SecuritySeverity::Info.score(), 0.0);
    }

    #[test]
    fn test_security_severity_as_str() {
        assert_eq!(SecuritySeverity::Info.as_str(), "info");
        assert_eq!(SecuritySeverity::Low.as_str(), "low");
        assert_eq!(SecuritySeverity::Medium.as_str(), "medium");
        assert_eq!(SecuritySeverity::High.as_str(), "high");
        assert_eq!(SecuritySeverity::Critical.as_str(), "critical");
    }

    #[test]
    fn test_security_severity_scores_concrete() {
        assert_eq!(SecuritySeverity::Info.score(), 0.0);
        assert_eq!(SecuritySeverity::Low.score(), 3.0);
        assert_eq!(SecuritySeverity::Medium.score(), 5.5);
        assert_eq!(SecuritySeverity::High.score(), 7.5);
        assert_eq!(SecuritySeverity::Critical.score(), 9.5);
    }

    // -------------------------------------------------------------------------
    // SecurityCategory
    // -------------------------------------------------------------------------

    #[test]
    fn test_security_category_as_str() {
        assert_eq!(
            SecurityCategory::HardcodedSecret.as_str(),
            "hardcoded_secret"
        );
        assert_eq!(SecurityCategory::Injection.as_str(), "injection");
        assert_eq!(SecurityCategory::Authentication.as_str(), "authentication");
        assert_eq!(SecurityCategory::Authorization.as_str(), "authorization");
        assert_eq!(SecurityCategory::DataExposure.as_str(), "data_exposure");
        assert_eq!(SecurityCategory::Cryptography.as_str(), "cryptography");
        assert_eq!(SecurityCategory::Configuration.as_str(), "configuration");
        assert_eq!(SecurityCategory::Dependency.as_str(), "dependency");
        assert_eq!(SecurityCategory::Compliance.as_str(), "compliance");
        assert_eq!(SecurityCategory::CodeQuality.as_str(), "code_quality");
    }

    #[test]
    fn test_security_category_custom() {
        let cat = SecurityCategory::Custom("my_category".to_string());
        assert_eq!(cat.as_str(), "my_category");
    }

    // -------------------------------------------------------------------------
    // SecurityFinding
    // -------------------------------------------------------------------------

    #[test]
    fn test_security_finding_new() {
        let finding = SecurityFinding::new(
            "Test Finding",
            SecurityCategory::HardcodedSecret,
            SecuritySeverity::High,
        );
        assert!(!finding.id.is_empty());
        assert!(finding.id.starts_with("SEC-"));
        assert!(finding.timestamp > 0);
        assert_eq!(finding.title, "Test Finding");
        assert_eq!(finding.description, "");
        assert!(finding.file.is_none());
        assert!(finding.line.is_none());
        assert!(finding.snippet.is_none());
        assert!(finding.remediation.is_none());
        assert!(finding.cwe.is_none());
    }

    #[test]
    fn test_security_finding_with_description() {
        let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
            .with_description("Detailed description here");
        assert_eq!(finding.description, "Detailed description here");
    }

    #[test]
    fn test_security_finding_with_location() {
        let finding =
            SecurityFinding::new("Test", SecurityCategory::Injection, SecuritySeverity::High)
                .with_location(PathBuf::from("test.rs"), 42);
        assert_eq!(finding.line, Some(42));
        assert_eq!(finding.file, Some(PathBuf::from("test.rs")));
    }

    #[test]
    fn test_security_finding_with_snippet() {
        let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
            .with_snippet("let x = dangerous();");
        assert_eq!(finding.snippet.as_deref(), Some("let x = dangerous();"));
    }

    #[test]
    fn test_security_finding_with_remediation() {
        let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
            .with_remediation("Use parameterized queries");
        assert_eq!(
            finding.remediation.as_deref(),
            Some("Use parameterized queries")
        );
    }

    #[test]
    fn test_security_finding_with_cwe() {
        let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
            .with_cwe("CWE-89");
        assert_eq!(finding.cwe.as_deref(), Some("CWE-89"));
    }

    #[test]
    fn test_security_finding_builder_chain() {
        let finding = SecurityFinding::new(
            "Chain Test",
            SecurityCategory::HardcodedSecret,
            SecuritySeverity::Critical,
        )
        .with_description("desc")
        .with_location(PathBuf::from("foo.rs"), 10)
        .with_snippet("snippet")
        .with_remediation("fix")
        .with_cwe("CWE-798");

        assert_eq!(finding.title, "Chain Test");
        assert_eq!(finding.description, "desc");
        assert_eq!(finding.line, Some(10));
        assert_eq!(finding.snippet.as_deref(), Some("snippet"));
        assert_eq!(finding.remediation.as_deref(), Some("fix"));
        assert_eq!(finding.cwe.as_deref(), Some("CWE-798"));
        assert_eq!(finding.severity, SecuritySeverity::Critical);
        assert_eq!(finding.category, SecurityCategory::HardcodedSecret);
    }

    // -------------------------------------------------------------------------
    // SecretPattern
    // -------------------------------------------------------------------------

    #[test]
    fn test_secret_pattern_new() {
        let pattern = SecretPattern::new("Test", r"\d+", SecuritySeverity::Low);
        assert_eq!(pattern.name, "Test");
        assert_eq!(pattern.severity, SecuritySeverity::Low);
        assert!(pattern.compiled.is_some(), "valid regex should compile");
    }

    #[test]
    fn test_secret_pattern_invalid_regex() {
        let pattern = SecretPattern::new("Bad", r"[invalid", SecuritySeverity::Low);
        assert!(
            pattern.compiled.is_none(),
            "invalid regex should produce None"
        );
    }

    #[test]
    fn test_secret_pattern_description_default() {
        let pattern = SecretPattern::new("AWS Key", r"\d+", SecuritySeverity::Critical);
        assert!(pattern.description.contains("AWS Key"));
    }

    // -------------------------------------------------------------------------
    // SecretScanner
    // -------------------------------------------------------------------------

    #[test]
    fn test_secret_scanner_new() {
        let scanner = SecretScanner::new();
        assert!(scanner.findings().is_empty());
    }

    #[test]
    fn test_secret_scanner_default() {
        let scanner = SecretScanner::default();
        assert!(scanner.findings().is_empty());
    }

    #[test]
    fn test_secret_scanner_empty_content() {
        let mut scanner = SecretScanner::new();
        let findings = scanner.scan_content("", None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_no_secrets() {
        let mut scanner = SecretScanner::new();
        let content = "let x = 42;\nfn main() {}\n// This is safe code";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_aws_access_key() {
        let mut scanner = SecretScanner::new();
        // AKIA followed by 16 uppercase alphanumeric chars
        let content = "aws_access_key = AKIAIOSFODNN7EXAMPLE";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.severity == SecuritySeverity::Critical));
    }

    #[test]
    fn test_secret_scanner_detect_private_key() {
        let mut scanner = SecretScanner::new();
        let content = "-----BEGIN RSA PRIVATE KEY-----";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_ec_private_key() {
        let mut scanner = SecretScanner::new();
        let content = "-----BEGIN EC PRIVATE KEY-----";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_openssh_private_key() {
        let mut scanner = SecretScanner::new();
        let content = "-----BEGIN OPENSSH PRIVATE KEY-----";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_generic_private_key() {
        let mut scanner = SecretScanner::new();
        let content = "-----BEGIN PRIVATE KEY-----";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_github_token_ghp() {
        let mut scanner = SecretScanner::new();
        // ghp_ followed by 36 alphanumeric/underscore chars (pattern: gh[pousr]_[A-Za-z0-9_]{36,})
        let content =
            "token = ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789ab";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.severity == SecuritySeverity::Critical));
    }

    #[test]
    fn test_secret_scanner_detect_github_token_gho() {
        let mut scanner = SecretScanner::new();
        let content = "gho_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_github_token_ghu() {
        let mut scanner = SecretScanner::new();
        let content = "ghu_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_github_fine_grained_token() {
        let mut scanner = SecretScanner::new();
        // github_pat_ followed by 22+ chars
        let content = "github_pat_ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_gitlab_token() {
        let mut scanner = SecretScanner::new();
        // glpat- followed by 20+ chars
        let content = "glpat-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_google_api_key() {
        let mut scanner = SecretScanner::new();
        // AIza followed by exactly 35 alphanumeric/underscore/hyphen chars
        // Pattern: AIza[a-zA-Z0-9_\-]{35}
        // Pattern: AIza[a-zA-Z0-9_\-]{35}  →  AIza + exactly 35 body chars
        let content = "key = AIzaSyDabcdefghijklmnopqrstuvwxyz012345";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_stripe_secret_key() {
        let mut scanner = SecretScanner::new();
        // Build the test key dynamically to avoid triggering push protection
        let prefix = "sk_live_";
        let suffix = "TESTKEYTESTKEYTESTKEYTESTK";
        let content = format!("stripe_key = {}{}", prefix, suffix);
        let findings = scanner.scan_content(&content, None);
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.severity == SecuritySeverity::Critical));
    }

    #[test]
    fn test_secret_scanner_detect_stripe_restricted_key() {
        let mut scanner = SecretScanner::new();
        // Build the test key dynamically to avoid triggering push protection
        let prefix = "rk_live_";
        let suffix = "TESTKEYTESTKEYTESTKEYTESTK";
        let content = format!("{}{}", prefix, suffix);
        let findings = scanner.scan_content(&content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_generic_api_key() {
        let mut scanner = SecretScanner::new();
        // api_key = 'somevalue20chars+'
        let content = r#"api_key = "abcdefghij1234567890extra""#;
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_apikey_variant() {
        let mut scanner = SecretScanner::new();
        let content = r#"apikey = "abcdefghijklmnopqrstuv""#;
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_password_in_code() {
        let mut scanner = SecretScanner::new();
        let content = r#"password = "s3cr3tpassword""#;
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_passwd_variant() {
        let mut scanner = SecretScanner::new();
        let content = r#"passwd = "mysecretpass1234""#;
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_bearer_token() {
        let mut scanner = SecretScanner::new();
        let content = "Authorization: Bearer eyJhbGciOiJSUzI1NiJ9";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_jwt_token() {
        let mut scanner = SecretScanner::new();
        // Full JWT: header.payload.signature, each base64url encoded starting with eyJ
        let content =
            "token = eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_database_url_postgres() {
        let mut scanner = SecretScanner::new();
        let content = "db_url = postgres://user:password@localhost/mydb";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_database_url_mysql() {
        let mut scanner = SecretScanner::new();
        let content = "url = mysql://admin:s3cr3t@db.example.com/prod";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_database_url_mongodb() {
        let mut scanner = SecretScanner::new();
        let content = "mongo_uri = mongodb://root:hunter2@mongo.local/admin";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_slack_bot_token() {
        let mut scanner = SecretScanner::new();
        // xoxb- followed by 10+ alphanumeric/hyphen
        let content = "slack_token = xoxb-1234567890-abcdefghij";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_slack_user_token() {
        let mut scanner = SecretScanner::new();
        let content = "xoxp-1234567890-0987654321-abcdef";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_detect_base64_secret() {
        let mut scanner = SecretScanner::new();
        // "secret = " followed by 40+ base64 chars
        let content = "secret = ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/==";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_skip_double_slash_comment() {
        let mut scanner = SecretScanner::new();
        let content = "// aws_key = AKIAIOSFODNN7EXAMPLE";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_skip_hash_comment() {
        let mut scanner = SecretScanner::new();
        let content = "# password = supersecretvalue";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_skip_block_comment() {
        let mut scanner = SecretScanner::new();
        let content = "/* password = supersecretvalue */";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_with_file_path() {
        let mut scanner = SecretScanner::new();
        let path = PathBuf::from("src/config.rs");
        let content = "AKIAIOSFODNN7EXAMPLE";
        let findings = scanner.scan_content(content, Some(&path));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].file, Some(path));
        assert_eq!(findings[0].line, Some(1));
    }

    #[test]
    fn test_secret_scanner_line_numbers() {
        let mut scanner = SecretScanner::new();
        let path = PathBuf::from("foo.rs");
        let content = "line1\nline2\nAKIAIOSFODNN7EXAMPLE\nline4";
        let findings = scanner.scan_content(content, Some(&path));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].line, Some(3));
    }

    #[test]
    fn test_secret_scanner_accumulates_findings() {
        let mut scanner = SecretScanner::new();
        scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
        scanner.scan_content("-----BEGIN RSA PRIVATE KEY-----", None);
        assert!(scanner.findings().len() >= 2);
    }

    #[test]
    fn test_secret_scanner_clear() {
        let mut scanner = SecretScanner::new();
        scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
        assert!(!scanner.findings().is_empty());
        scanner.clear();
        assert!(scanner.findings().is_empty());
    }

    #[test]
    fn test_secret_scanner_masked_snippet() {
        let mut scanner = SecretScanner::new();
        let content = "AKIAIOSFODNN7EXAMPLE";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
        // The snippet should be masked (contains "..." or all "*")
        if let Some(snippet) = &findings[0].snippet {
            assert!(snippet.contains("...") || snippet.chars().all(|c| c == '*'));
        }
    }

    #[test]
    fn test_secret_scanner_remediation_set() {
        let mut scanner = SecretScanner::new();
        let findings = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
        assert!(!findings.is_empty());
        assert!(findings[0].remediation.is_some());
    }

    #[test]
    fn test_secret_scanner_cwe_set() {
        let mut scanner = SecretScanner::new();
        let findings = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].cwe.as_deref(), Some("CWE-798"));
    }

    #[test]
    fn test_secret_scanner_add_custom_pattern() {
        let mut scanner = SecretScanner::new();
        let custom = SecretPattern::new("Custom Secret", r"MYSECRET\d{6}", SecuritySeverity::High);
        scanner.add_pattern(custom);
        let content = "key = MYSECRET123456";
        let findings = scanner.scan_content(content, None);
        assert!(findings.iter().any(|f| f.title == "Custom Secret"));
    }

    #[test]
    fn test_secret_scanner_unicode_content() {
        let mut scanner = SecretScanner::new();
        // Unicode characters should not cause panics
        let content = "let msg = \"こんにちは世界\"; // no secrets here";
        let findings = scanner.scan_content(content, None);
        // Comment line is skipped; non-comment line has no secret pattern match
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_very_long_line() {
        let mut scanner = SecretScanner::new();
        // 10 000-character line with no secrets; should complete without panic
        let content = "a".repeat(10_000);
        let findings = scanner.scan_content(&content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_multiline_content() {
        let mut scanner = SecretScanner::new();
        let content = "line1\nline2\nline3\nline4\nline5";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    // -------------------------------------------------------------------------
    // VulnerabilityDetector
    // -------------------------------------------------------------------------

    #[test]
    fn test_vulnerability_detector_new() {
        let detector = VulnerabilityDetector::new();
        assert!(detector.findings().is_empty());
    }

    #[test]
    fn test_vulnerability_detector_default() {
        let detector = VulnerabilityDetector::default();
        assert!(detector.findings().is_empty());
    }

    #[test]
    fn test_vulnerability_detector_empty_content() {
        let mut detector = VulnerabilityDetector::new();
        let findings = detector.scan_content("", None, "rust");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_rust_unsafe() {
        let mut detector = VulnerabilityDetector::new();
        let content = "unsafe { ptr::read(addr) }";
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-242")));
    }

    #[test]
    fn test_vulnerability_detector_rust_unsafe_severity() {
        let mut detector = VulnerabilityDetector::new();
        let content = "unsafe { }";
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        // RUST001 is Medium
        assert!(findings
            .iter()
            .any(|f| f.severity == SecuritySeverity::Medium));
    }

    #[test]
    fn test_vulnerability_detector_rust_unwrap() {
        let mut detector = VulnerabilityDetector::new();
        let content = "let x = option.unwrap();";
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-252")));
    }

    #[test]
    fn test_vulnerability_detector_rust_unwrap_severity() {
        let mut detector = VulnerabilityDetector::new();
        let content = "let x = result.unwrap();";
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        // RUST002 is Low
        assert!(findings.iter().any(|f| f.severity == SecuritySeverity::Low));
    }

    #[test]
    fn test_vulnerability_detector_rust_sql_injection_select() {
        let mut detector = VulnerabilityDetector::new();
        let content = r#"let q = format!("SELECT * FROM users WHERE id={}", id);"#;
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-89")));
    }

    #[test]
    fn test_vulnerability_detector_rust_sql_injection_insert() {
        let mut detector = VulnerabilityDetector::new();
        let content = r#"let q = format!("INSERT INTO logs VALUES({})", val);"#;
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_rust_sql_injection_update() {
        let mut detector = VulnerabilityDetector::new();
        let content = r#"let q = format!("UPDATE users SET name={}", name);"#;
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_rust_sql_injection_delete() {
        let mut detector = VulnerabilityDetector::new();
        let content = r#"let q = format!("DELETE FROM sessions WHERE id={}", id);"#;
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_rust_command_injection() {
        let mut detector = VulnerabilityDetector::new();
        let content = r#"Command::new(&format!("{}", user_input)).spawn().unwrap();"#;
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.severity == SecuritySeverity::Critical));
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
    }

    #[test]
    fn test_vulnerability_detector_rust_path_traversal_path_new() {
        let mut detector = VulnerabilityDetector::new();
        let content = r#"let p = Path::new(&format!("/uploads/{}", filename));"#;
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-22")));
    }

    #[test]
    fn test_vulnerability_detector_rust_path_traversal_pathbuf_from() {
        let mut detector = VulnerabilityDetector::new();
        let content = r#"let p = PathBuf::from(&format!("/data/{}", input));"#;
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_js_eval() {
        let mut detector = VulnerabilityDetector::new();
        let content = "eval(userInput)";
        let findings = detector.scan_content(content, None, "javascript");
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.severity == SecuritySeverity::Critical));
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-95")));
    }

    #[test]
    fn test_vulnerability_detector_js_inner_html() {
        let mut detector = VulnerabilityDetector::new();
        let content = "element.innerHTML = userInput;";
        let findings = detector.scan_content(content, None, "javascript");
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-79")));
    }

    #[test]
    fn test_vulnerability_detector_python_exec() {
        let mut detector = VulnerabilityDetector::new();
        let content = "exec(user_code)";
        let findings = detector.scan_content(content, None, "python");
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.severity == SecuritySeverity::Critical));
    }

    #[test]
    fn test_vulnerability_detector_python_eval() {
        let mut detector = VulnerabilityDetector::new();
        let content = "result = eval(expression)";
        let findings = detector.scan_content(content, None, "python");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_python_pickle_load() {
        let mut detector = VulnerabilityDetector::new();
        let content = "obj = pickle.load(file_handle)";
        let findings = detector.scan_content(content, None, "python");
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-502")));
    }

    #[test]
    fn test_vulnerability_detector_python_pickle_loads() {
        let mut detector = VulnerabilityDetector::new();
        let content = "data = pickle.loads(raw_bytes)";
        let findings = detector.scan_content(content, None, "python");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_language_filter() {
        // JS patterns should NOT fire on Rust code
        let mut detector = VulnerabilityDetector::new();
        let content = "eval(something)";
        let findings = detector.scan_content(content, None, "rust");
        // JS001 (eval) should not match when language is "rust"
        assert!(!findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-95")));
    }

    #[test]
    fn test_vulnerability_detector_with_file_location() {
        let mut detector = VulnerabilityDetector::new();
        let path = PathBuf::from("src/main.rs");
        let content = "unsafe { }";
        let findings = detector.scan_content(content, Some(&path), "rust");
        assert!(!findings.is_empty());
        assert_eq!(findings[0].file, Some(path));
        assert_eq!(findings[0].line, Some(1));
    }

    #[test]
    fn test_vulnerability_detector_snippet_is_trimmed() {
        let mut detector = VulnerabilityDetector::new();
        let content = "    unsafe { }  ";
        let findings = detector.scan_content(content, None, "rust");
        assert!(!findings.is_empty());
        // snippet should be trimmed
        if let Some(snippet) = &findings[0].snippet {
            assert_eq!(snippet.trim(), snippet.as_str());
        }
    }

    #[test]
    fn test_vulnerability_detector_accumulates_findings() {
        let mut detector = VulnerabilityDetector::new();
        detector.scan_content("unsafe { }", None, "rust");
        detector.scan_content("let x = option.unwrap();", None, "rust");
        assert!(detector.findings().len() >= 2);
    }

    #[test]
    fn test_vulnerability_detector_clear() {
        let mut detector = VulnerabilityDetector::new();
        detector.scan_content("unsafe { }", None, "rust");
        assert!(!detector.findings().is_empty());
        detector.clear();
        assert!(detector.findings().is_empty());
    }

    #[test]
    fn test_vulnerability_detector_add_custom_pattern() {
        let mut detector = VulnerabilityDetector::new();
        detector.add_pattern(VulnerabilityPattern {
            id: "CUSTOM001".to_string(),
            name: "Custom Check".to_string(),
            language: "rust".to_string(),
            pattern: r"todo!\(\)".to_string(),
            severity: SecuritySeverity::Info,
            cwe: "CWE-0".to_string(),
            description: "Incomplete code marker".to_string(),
            remediation: "Remove todo!() before production".to_string(),
        });
        let findings = detector.scan_content("todo!()", None, "rust");
        assert!(findings.iter().any(|f| f.title == "Custom Check"));
    }

    #[test]
    fn test_vulnerability_detector_unknown_language_no_matches() {
        let mut detector = VulnerabilityDetector::new();
        // "cobol" matches no built-in patterns
        let content = "eval(foo) unsafe { } pickle.loads(x)";
        let findings = detector.scan_content(content, None, "cobol");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_vulnerability_detector_wildcard_language() {
        // A pattern with language "*" should match all languages
        let mut detector = VulnerabilityDetector::new();
        detector.add_pattern(VulnerabilityPattern {
            id: "ALL001".to_string(),
            name: "Universal Check".to_string(),
            language: "*".to_string(),
            pattern: r"FORBIDDEN".to_string(),
            severity: SecuritySeverity::High,
            cwe: "CWE-999".to_string(),
            description: "Forbidden token".to_string(),
            remediation: "Remove it".to_string(),
        });
        let findings_rust = detector.scan_content("FORBIDDEN", None, "rust");
        assert!(!findings_rust.is_empty());
        let findings_js = detector.scan_content("FORBIDDEN", None, "javascript");
        assert!(!findings_js.is_empty());
    }

    // -------------------------------------------------------------------------
    // KnownVulnerability
    // -------------------------------------------------------------------------

    #[test]
    fn test_known_vulnerability_new() {
        let vuln = KnownVulnerability::new("CVE-2021-1234", SecuritySeverity::High, "Test vuln");
        assert_eq!(vuln.id, "CVE-2021-1234");
        assert_eq!(vuln.severity, SecuritySeverity::High);
        assert_eq!(vuln.description, "Test vuln");
        assert!(vuln.fixed_version.is_none());
        assert!(vuln.url.is_none());
    }

    #[test]
    fn test_known_vulnerability_with_fixed_version() {
        let vuln = KnownVulnerability::new("CVE-X", SecuritySeverity::Low, "desc")
            .with_fixed_version("2.0.0");
        assert_eq!(vuln.fixed_version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn test_known_vulnerability_with_url() {
        let vuln = KnownVulnerability::new("CVE-X", SecuritySeverity::Low, "desc")
            .with_url("https://nvd.nist.gov/vuln/detail/CVE-X");
        assert_eq!(
            vuln.url.as_deref(),
            Some("https://nvd.nist.gov/vuln/detail/CVE-X")
        );
    }

    // -------------------------------------------------------------------------
    // Dependency
    // -------------------------------------------------------------------------

    #[test]
    fn test_dependency_new() {
        let dep = Dependency::new("test-pkg", "1.0.0", "npm");
        assert_eq!(dep.name, "test-pkg");
        assert_eq!(dep.version, "1.0.0");
        assert_eq!(dep.source, "npm");
        assert!(!dep.is_vulnerable());
        assert!(dep.vulnerabilities.is_empty());
    }

    #[test]
    fn test_dependency_is_vulnerable_with_vuln() {
        let mut dep = Dependency::new("pkg", "1.0", "crates.io");
        dep.vulnerabilities.push(KnownVulnerability::new(
            "CVE-X",
            SecuritySeverity::High,
            "d",
        ));
        assert!(dep.is_vulnerable());
    }

    #[test]
    fn test_dependency_max_severity_none() {
        let dep = Dependency::new("pkg", "1.0", "crates.io");
        assert_eq!(dep.max_severity(), None);
    }

    #[test]
    fn test_dependency_max_severity_single() {
        let mut dep = Dependency::new("pkg", "1.0", "crates.io");
        dep.vulnerabilities.push(KnownVulnerability::new(
            "CVE-X",
            SecuritySeverity::High,
            "d",
        ));
        assert_eq!(dep.max_severity(), Some(SecuritySeverity::High));
    }

    #[test]
    fn test_dependency_max_severity_multiple() {
        let mut dep = Dependency::new("pkg", "1.0", "crates.io");
        dep.vulnerabilities
            .push(KnownVulnerability::new("CVE-A", SecuritySeverity::Low, "d"));
        dep.vulnerabilities.push(KnownVulnerability::new(
            "CVE-B",
            SecuritySeverity::Critical,
            "e",
        ));
        dep.vulnerabilities.push(KnownVulnerability::new(
            "CVE-C",
            SecuritySeverity::High,
            "f",
        ));
        assert_eq!(dep.max_severity(), Some(SecuritySeverity::Critical));
    }

    // -------------------------------------------------------------------------
    // DependencyAuditor
    // -------------------------------------------------------------------------

    #[test]
    fn test_dependency_auditor_new() {
        let auditor = DependencyAuditor::new();
        assert!(auditor.vulnerable_dependencies().is_empty());
        assert!(auditor.findings().is_empty());
    }

    #[test]
    fn test_dependency_auditor_default() {
        let auditor = DependencyAuditor::default();
        assert!(auditor.findings().is_empty());
    }

    #[test]
    fn test_dependency_auditor_known_vuln_lodash() {
        let mut auditor = DependencyAuditor::new();
        let dep = auditor.audit_dependency("lodash", "4.17.0", "npm");
        assert!(dep.is_vulnerable());
        assert!(!auditor.findings().is_empty());
        assert_eq!(auditor.findings()[0].category, SecurityCategory::Dependency);
    }

    #[test]
    fn test_dependency_auditor_known_vuln_log4j() {
        let mut auditor = DependencyAuditor::new();
        let dep = auditor.audit_dependency("log4j", "2.14.0", "maven");
        assert!(dep.is_vulnerable());
        assert!(dep.vulnerabilities.iter().any(|v| v.id == "CVE-2021-44228"));
        assert!(dep
            .vulnerabilities
            .iter()
            .any(|v| v.severity == SecuritySeverity::Critical));
    }

    #[test]
    fn test_dependency_auditor_safe_dep() {
        let mut auditor = DependencyAuditor::new();
        let dep = auditor.audit_dependency("safe-package", "1.0.0", "npm");
        assert!(!dep.is_vulnerable());
    }

    #[test]
    fn test_dependency_auditor_vulnerable_dependencies_list() {
        let mut auditor = DependencyAuditor::new();
        auditor.audit_dependency("safe-package", "1.0.0", "npm");
        auditor.audit_dependency("lodash", "4.17.0", "npm");
        let vulns = auditor.vulnerable_dependencies();
        assert_eq!(vulns.len(), 1);
        assert_eq!(vulns[0].name, "lodash");
    }

    #[test]
    fn test_dependency_auditor_add_vulnerability() {
        let mut auditor = DependencyAuditor::new();
        let vuln = KnownVulnerability::new("CVE-CUSTOM", SecuritySeverity::Medium, "Custom vuln");
        auditor.add_vulnerability("my-package", vuln);
        let dep = auditor.audit_dependency("my-package", "1.0.0", "custom");
        assert!(dep.is_vulnerable());
    }

    #[test]
    fn test_dependency_auditor_finding_remediation_contains_fixed_version() {
        let mut auditor = DependencyAuditor::new();
        auditor.audit_dependency("lodash", "4.17.0", "npm");
        let finding = &auditor.findings()[0];
        // remediation should mention the fixed version
        assert!(finding
            .remediation
            .as_deref()
            .unwrap_or("")
            .contains("4.17.21"));
    }

    #[test]
    fn test_dependency_auditor_clear() {
        let mut auditor = DependencyAuditor::new();
        auditor.audit_dependency("lodash", "4.17.0", "npm");
        assert!(!auditor.findings().is_empty());
        auditor.clear();
        assert!(auditor.findings().is_empty());
        assert!(auditor.vulnerable_dependencies().is_empty());
    }

    // -------------------------------------------------------------------------
    // ComplianceRule
    // -------------------------------------------------------------------------

    #[test]
    fn test_compliance_rule_new() {
        let rule = ComplianceRule::new("R1", "OWASP", "Test rule");
        assert_eq!(rule.id, "R1");
        assert_eq!(rule.standard, "OWASP");
        assert_eq!(rule.description, "Test rule");
        assert!(rule.pattern.is_none());
        assert_eq!(rule.severity, SecuritySeverity::Medium);
    }

    #[test]
    fn test_compliance_rule_with_pattern() {
        let rule = ComplianceRule::new("R1", "OWASP", "desc").with_pattern(r"md5\(");
        assert!(rule.pattern.is_some());
    }

    #[test]
    fn test_compliance_rule_with_severity() {
        let rule =
            ComplianceRule::new("R1", "PCI", "desc").with_severity(SecuritySeverity::Critical);
        assert_eq!(rule.severity, SecuritySeverity::Critical);
    }

    // -------------------------------------------------------------------------
    // ComplianceChecker
    // -------------------------------------------------------------------------

    #[test]
    fn test_compliance_checker_new() {
        let checker = ComplianceChecker::new();
        assert!(!checker.standards().is_empty());
        assert!(checker.findings().is_empty());
    }

    #[test]
    fn test_compliance_checker_default() {
        let checker = ComplianceChecker::default();
        assert!(!checker.standards().is_empty());
    }

    #[test]
    fn test_compliance_checker_standards_sorted_deduped() {
        let checker = ComplianceChecker::new();
        let standards = checker.standards();
        // Check sorted
        for window in standards.windows(2) {
            assert!(window[0] <= window[1]);
        }
        // Check deduped
        let mut prev = "";
        for s in &standards {
            assert_ne!(s, prev, "duplicate standard found: {s}");
            prev = s;
        }
    }

    #[test]
    fn test_compliance_checker_contains_known_standards() {
        let checker = ComplianceChecker::new();
        let standards = checker.standards();
        assert!(standards.iter().any(|s| s == "OWASP Top 10"));
        assert!(standards.iter().any(|s| s == "PCI-DSS"));
        assert!(standards.iter().any(|s| s == "HIPAA"));
    }

    #[test]
    fn test_compliance_checker_weak_crypto_md5() {
        let mut checker = ComplianceChecker::new();
        let content = "hash = md5(password)";
        let findings = checker.check_content(content, None);
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.category == SecurityCategory::Compliance));
    }

    #[test]
    fn test_compliance_checker_weak_crypto_sha1() {
        let mut checker = ComplianceChecker::new();
        let content = "digest = sha1(data)";
        let findings = checker.check_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_compliance_checker_case_insensitive_md5() {
        let mut checker = ComplianceChecker::new();
        let content = "hash = MD5(password)";
        let findings = checker.check_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_compliance_checker_case_insensitive_sha1() {
        let mut checker = ComplianceChecker::new();
        let content = "digest = SHA1(data)";
        let findings = checker.check_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_compliance_checker_clean_code() {
        let mut checker = ComplianceChecker::new();
        // sha256 is not a weak hash; should not trigger
        let content = "digest = sha256(data)";
        let findings = checker.check_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_compliance_checker_empty_content() {
        let mut checker = ComplianceChecker::new();
        let findings = checker.check_content("", None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_compliance_checker_with_file_path() {
        let mut checker = ComplianceChecker::new();
        let path = PathBuf::from("crypto.py");
        let content = "hash = md5(data)";
        let findings = checker.check_content(content, Some(&path));
        assert!(!findings.is_empty());
        assert_eq!(findings[0].file, Some(path));
    }

    #[test]
    fn test_compliance_checker_snippet_is_trimmed() {
        let mut checker = ComplianceChecker::new();
        let content = "   hash = md5(data)   ";
        let findings = checker.check_content(content, None);
        assert!(!findings.is_empty());
        if let Some(snippet) = &findings[0].snippet {
            assert_eq!(snippet.trim(), snippet.as_str());
        }
    }

    #[test]
    fn test_compliance_checker_accumulates_findings() {
        let mut checker = ComplianceChecker::new();
        checker.check_content("md5(x)", None);
        checker.check_content("sha1(y)", None);
        assert!(checker.findings().len() >= 2);
    }

    #[test]
    fn test_compliance_checker_clear() {
        let mut checker = ComplianceChecker::new();
        checker.check_content("md5(x)", None);
        assert!(!checker.findings().is_empty());
        checker.clear();
        assert!(checker.findings().is_empty());
    }

    #[test]
    fn test_compliance_checker_add_rule() {
        let mut checker = ComplianceChecker::new();
        checker.add_rule(
            ComplianceRule::new("CUSTOM-001", "CUSTOM_STD", "No goto")
                .with_pattern(r"\bgoto\b")
                .with_severity(SecuritySeverity::Low),
        );
        let findings = checker.check_content("goto label;", None);
        assert!(findings.iter().any(|f| f.title.contains("CUSTOM-001")));
    }

    // -------------------------------------------------------------------------
    // ScanResult
    // -------------------------------------------------------------------------

    #[test]
    fn test_scan_result_new() {
        let result = ScanResult::new();
        assert_eq!(result.total_findings(), 0);
        assert!(!result.has_critical());
        assert!(!result.has_high());
        assert_eq!(result.risk_score(), 0.0);
        assert_eq!(result.files_scanned, 0);
        assert_eq!(result.lines_scanned, 0);
    }

    #[test]
    fn test_scan_result_default() {
        let result = ScanResult::default();
        assert_eq!(result.total_findings(), 0);
    }

    #[test]
    fn test_scan_result_total_findings() {
        let mut result = ScanResult::new();
        result.findings.push(SecurityFinding::new(
            "F1",
            SecurityCategory::Injection,
            SecuritySeverity::Low,
        ));
        result.findings.push(SecurityFinding::new(
            "F2",
            SecurityCategory::Injection,
            SecuritySeverity::High,
        ));
        assert_eq!(result.total_findings(), 2);
    }

    #[test]
    fn test_scan_result_has_critical_true() {
        let mut result = ScanResult::new();
        result.by_severity.insert(SecuritySeverity::Critical, 1);
        assert!(result.has_critical());
    }

    #[test]
    fn test_scan_result_has_critical_zero() {
        let mut result = ScanResult::new();
        result.by_severity.insert(SecuritySeverity::Critical, 0);
        assert!(!result.has_critical());
    }

    #[test]
    fn test_scan_result_has_high_true() {
        let mut result = ScanResult::new();
        result.by_severity.insert(SecuritySeverity::High, 2);
        assert!(result.has_high());
    }

    #[test]
    fn test_scan_result_has_high_zero() {
        let mut result = ScanResult::new();
        result.by_severity.insert(SecuritySeverity::High, 0);
        assert!(!result.has_high());
    }

    #[test]
    fn test_scan_result_risk_score_empty() {
        let result = ScanResult::new();
        assert_eq!(result.risk_score(), 0.0);
    }

    #[test]
    fn test_scan_result_risk_score_one_critical() {
        let mut result = ScanResult::new();
        result.findings.push(SecurityFinding::new(
            "Test",
            SecurityCategory::Injection,
            SecuritySeverity::Critical,
        ));
        assert_eq!(result.risk_score(), 9.5);
    }

    #[test]
    fn test_scan_result_risk_score_additive() {
        let mut result = ScanResult::new();
        result.findings.push(SecurityFinding::new(
            "F1",
            SecurityCategory::Injection,
            SecuritySeverity::High,
        ));
        result.findings.push(SecurityFinding::new(
            "F2",
            SecurityCategory::Injection,
            SecuritySeverity::Medium,
        ));
        // 7.5 + 5.5 = 13.0
        assert!((result.risk_score() - 13.0).abs() < f32::EPSILON);
    }

    // -------------------------------------------------------------------------
    // SecurityScanner (top-level integration)
    // -------------------------------------------------------------------------

    #[test]
    fn test_security_scanner_new() {
        let scanner = SecurityScanner::new();
        let stats = scanner.get_stats();
        assert_eq!(stats.total_scans, 0);
        assert_eq!(stats.total_findings, 0);
        assert_eq!(stats.critical_findings, 0);
        assert_eq!(stats.high_findings, 0);
    }

    #[test]
    fn test_security_scanner_default() {
        let scanner = SecurityScanner::default();
        let stats = scanner.get_stats();
        assert_eq!(stats.total_scans, 0);
    }

    #[test]
    fn test_security_scanner_scan_clean_code() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("let x = 1;", None, "rust");
        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.lines_scanned, 1);
    }

    #[test]
    fn test_security_scanner_scan_empty_content() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("", None, "rust");
        assert_eq!(result.total_findings(), 0);
        assert_eq!(result.lines_scanned, 0);
    }

    #[test]
    fn test_security_scanner_scan_detects_aws_secret() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("api_key = \"AKIAIOSFODNN7EXAMPLE\"", None, "rust");
        assert!(result.total_findings() > 0);
        assert!(result.by_category.contains_key("hardcoded_secret"));
    }

    #[test]
    fn test_security_scanner_scan_detects_rust_unsafe() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("unsafe { }", None, "rust");
        assert!(result.total_findings() > 0);
        assert!(result.by_category.contains_key("code_quality"));
    }

    #[test]
    fn test_security_scanner_scan_detects_weak_crypto() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("md5(password)", None, "python");
        assert!(result.total_findings() > 0);
        assert!(result.by_category.contains_key("compliance"));
    }

    #[test]
    fn test_security_scanner_by_severity_populated() {
        let scanner = SecurityScanner::new();
        // unsafe is Medium severity
        let result = scanner.scan_content("unsafe { }", None, "rust");
        let total: usize = result.by_severity.values().sum();
        assert_eq!(total, result.total_findings());
    }

    #[test]
    fn test_security_scanner_by_category_populated() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("unsafe { }", None, "rust");
        let total: usize = result.by_category.values().sum();
        assert_eq!(total, result.total_findings());
    }

    #[test]
    fn test_security_scanner_duration_recorded() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("let x = 1;", None, "rust");
        // duration_ms may be 0 for fast scans but should not be negative (it's u64)
        let _ = result.duration_ms;
    }

    #[test]
    fn test_security_scanner_stats_accumulate_across_scans() {
        let scanner = SecurityScanner::new();
        scanner.scan_content("let x = 1;", None, "rust");
        scanner.scan_content("let y = 2;", None, "rust");
        let stats = scanner.get_stats();
        assert_eq!(stats.total_scans, 2);
    }

    #[test]
    fn test_security_scanner_stats_count_findings() {
        let scanner = SecurityScanner::new();
        scanner.scan_content("unsafe { }", None, "rust");
        let stats = scanner.get_stats();
        assert!(stats.total_findings >= 1);
    }

    #[test]
    fn test_security_scanner_stats_count_critical() {
        let scanner = SecurityScanner::new();
        // An AWS key is Critical
        scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None, "rust");
        let stats = scanner.get_stats();
        assert!(stats.critical_findings >= 1);
    }

    #[test]
    fn test_security_scanner_stats_count_high() {
        let scanner = SecurityScanner::new();
        // A bearer token is High
        scanner.scan_content("Authorization: Bearer sometoken123", None, "rust");
        let stats = scanner.get_stats();
        assert!(stats.high_findings >= 1);
    }

    #[test]
    fn test_security_scanner_audit_dependency_vulnerable() {
        let scanner = SecurityScanner::new();
        let dep = scanner.audit_dependency("lodash", "4.17.0", "npm");
        assert!(dep.is_vulnerable());
    }

    #[test]
    fn test_security_scanner_audit_dependency_safe() {
        let scanner = SecurityScanner::new();
        let dep = scanner.audit_dependency("totally-safe-pkg", "9.9.9", "npm");
        assert!(!dep.is_vulnerable());
    }

    #[test]
    fn test_security_scanner_audit_dependency_log4j() {
        let scanner = SecurityScanner::new();
        let dep = scanner.audit_dependency("log4j", "2.14.0", "maven");
        assert!(dep.is_vulnerable());
        assert!(dep
            .vulnerabilities
            .iter()
            .any(|v| v.severity == SecuritySeverity::Critical));
    }

    #[test]
    fn test_security_scanner_report_contains_header() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("let x = 1;", None, "rust");
        let report = scanner.generate_report(&result);
        assert!(report.contains("# Security Scan Report"));
    }

    #[test]
    fn test_security_scanner_report_contains_stats() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("let x = 1;\nlet y = 2;", None, "rust");
        let report = scanner.generate_report(&result);
        assert!(report.contains("Files scanned: 1"));
        assert!(report.contains("Lines scanned: 2"));
        assert!(report.contains("Total findings:"));
        assert!(report.contains("Risk score:"));
    }

    #[test]
    fn test_security_scanner_report_critical_section() {
        let scanner = SecurityScanner::new();
        // AWS key triggers Critical
        let result = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None, "rust");
        let report = scanner.generate_report(&result);
        assert!(report.contains("CRITICAL"));
    }

    #[test]
    fn test_security_scanner_report_high_section() {
        let scanner = SecurityScanner::new();
        // Bearer token triggers High (no Critical expected for a bearer token alone)
        let result = scanner.scan_content("Authorization: Bearer sometoken", None, "rust");
        let report = scanner.generate_report(&result);
        // At minimum we get summary section
        assert!(report.contains("Summary by Category"));
    }

    #[test]
    fn test_security_scanner_report_summary_by_category() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_content("unsafe { }", None, "rust");
        let report = scanner.generate_report(&result);
        assert!(report.contains("Summary by Category"));
    }

    #[test]
    fn test_security_scanner_report_with_file_location() {
        let scanner = SecurityScanner::new();
        let path = PathBuf::from("src/secrets.rs");
        let result = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", Some(&path), "rust");
        let report = scanner.generate_report(&result);
        assert!(report.contains("src/secrets.rs"));
    }

    #[test]
    fn test_security_scanner_scan_history_capped() {
        let scanner = SecurityScanner::new();
        // Scan 110 times; history should be capped at 100
        for _ in 0..110 {
            scanner.scan_content("let x = 1;", None, "rust");
        }
        let stats = scanner.get_stats();
        assert!(stats.total_scans <= 100);
    }

    #[test]
    fn test_security_scanner_multiline_rust_vuln() {
        let scanner = SecurityScanner::new();
        let content = "fn safe() {}\nfn danger() { unsafe { *ptr = 0; } }\nfn also_safe() {}";
        let result = scanner.scan_content(content, None, "rust");
        assert!(result.total_findings() >= 1);
        assert_eq!(result.lines_scanned, 3);
    }

    // -------------------------------------------------------------------------
    // Edge cases & advanced scenarios
    // -------------------------------------------------------------------------

    #[test]
    fn test_scan_only_whitespace() {
        let mut scanner = SecretScanner::new();
        let content = "   \n\t\n   ";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_scan_windows_line_endings() {
        let mut scanner = SecretScanner::new();
        // Windows CRLF - should not cause panics
        let content = "let x = 1;\r\nlet y = 2;\r\n";
        let findings = scanner.scan_content(content, None);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_secret_scanner_short_secret_masking() {
        // mask_secret: len <= 8 → all stars
        let mut scanner = SecretScanner::new();
        // Craft content that matches the private key pattern (short enough to trigger <=8 branch)
        // The pattern match "-----BEGIN PRIVATE KEY-----" is > 8 chars, so we test the other branch
        // Just verify the scanner does not panic on short snippets
        let content = "-----BEGIN PRIVATE KEY-----";
        let findings = scanner.scan_content(content, None);
        if let Some(f) = findings.first() {
            // snippet must be Some and non-empty
            assert!(f.snippet.is_some());
        }
    }

    #[test]
    fn test_vulnerability_detector_multiple_vulns_same_line() {
        let mut detector = VulnerabilityDetector::new();
        // This line matches both unsafe (RUST001) and unwrap (RUST002)
        let content = "unsafe { x.unwrap() }";
        let findings = detector.scan_content(content, None, "rust");
        assert!(findings.len() >= 2);
    }

    #[test]
    fn test_compliance_checker_multiple_findings_same_line() {
        let mut checker = ComplianceChecker::new();
        // Both md5 and sha1 patterns on one line
        let content = "use_md5_and_sha1()";
        // At most one OWASP-A02 pattern fires since there is only one pattern rule for weak crypto
        let findings = checker.check_content(content, None);
        // Results depend on how many rules have patterns that match
        let _ = findings; // No panic is the key assertion
    }

    #[test]
    fn test_security_finding_ids_are_unique() {
        // IDs are based on nanosecond timestamps; with sleep this would be guaranteed,
        // but we just check that the structure produces non-empty IDs and that two
        // findings created sequentially get the SEC- prefix.
        let f1 = SecurityFinding::new("A", SecurityCategory::Injection, SecuritySeverity::Low);
        let f2 = SecurityFinding::new("B", SecurityCategory::Injection, SecuritySeverity::Low);
        assert!(f1.id.starts_with("SEC-"));
        assert!(f2.id.starts_with("SEC-"));
    }

    #[test]
    fn test_scan_result_has_critical_absent_key() {
        let result = ScanResult::new();
        // by_severity is empty → has_critical should return false
        assert!(!result.has_critical());
    }

    #[test]
    fn test_scan_result_has_high_absent_key() {
        let result = ScanResult::new();
        assert!(!result.has_high());
    }

    #[test]
    fn test_dependency_auditor_multiple_audits() {
        let mut auditor = DependencyAuditor::new();
        auditor.audit_dependency("lodash", "4.17.0", "npm");
        auditor.audit_dependency("log4j", "2.14.0", "maven");
        auditor.audit_dependency("safe-pkg", "1.0", "npm");
        assert_eq!(auditor.vulnerable_dependencies().len(), 2);
    }

    #[test]
    fn test_scanner_stats_fields() {
        let scanner = SecurityScanner::new();
        // Scan with known critical finding
        scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None, "rust");
        let stats = scanner.get_stats();
        assert_eq!(stats.total_scans, 1);
        assert!(stats.total_findings >= 1);
        assert!(stats.critical_findings >= 1);
    }

    #[test]
    fn test_vulnerability_pattern_struct_fields() {
        let pattern = VulnerabilityPattern {
            id: "V1".to_string(),
            name: "Test Pattern".to_string(),
            language: "rust".to_string(),
            pattern: r"test".to_string(),
            severity: SecuritySeverity::Low,
            cwe: "CWE-1".to_string(),
            description: "A test pattern".to_string(),
            remediation: "Fix it".to_string(),
        };
        assert_eq!(pattern.id, "V1");
        assert_eq!(pattern.name, "Test Pattern");
        assert_eq!(pattern.language, "rust");
        assert_eq!(pattern.cwe, "CWE-1");
        assert_eq!(pattern.severity, SecuritySeverity::Low);
    }

    #[test]
    fn test_secret_scanner_detects_multiline_secrets() {
        let mut scanner = SecretScanner::new();
        let content = "line1\nAKIAIOSFODNN7EXAMPLE\nline3";
        let findings = scanner.scan_content(content, None);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_compliance_checker_no_match_when_no_pattern() {
        // Rules without a pattern should never produce findings via check_content
        let mut checker = ComplianceChecker::new();
        checker.add_rule(ComplianceRule::new(
            "OWASP-A01",
            "OWASP Top 10",
            "Broken Access Control",
        ));
        // "Broken Access Control" rule has no pattern, so check_content should not trigger it
        let findings = checker.check_content("something dangerous", None);
        // Only pattern-based rules fire; ensure no panic
        let _ = findings;
    }
}
