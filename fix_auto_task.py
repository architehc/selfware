import re

with open('src/cognitive/self_edit.rs', 'r') as f:
    content = f.read()

# Add a method to inspect past performance metrics and generate structural targets
introspection_method = """
    /// Introspect past performance to identify systemic weaknesses
    pub fn introspect_performance(&self) -> Vec<ImprovementTarget> {
        let mut targets = Vec::new();
        let snapshot = crate::cognitive::metrics::PerformanceSnapshot::take();
        
        // Example: If compilation fails often, we need better syntax checking or FIM usage
        if snapshot.compilation_errors_per_task > 1.5 {
            targets.push(ImprovementTarget {
                id: format!("perf-compilation-{}", chrono::Utc::now().timestamp()),
                source: ImprovementSource::ErrorPattern,
                category: ImprovementCategory::BugFix,
                description: "Agent is struggling with compilation errors. Consider building a pre-compilation AST validation tool or utilizing FIM more reliably.".to_string(),
                target_files: vec!["src/tools/mod.rs".to_string(), "src/tools/fim.rs".to_string()],
                priority: 0.9,
                confidence: 0.8,
            });
        }
        
        // Example: If tool calls are inefficient
        if snapshot.avg_tool_calls > 15.0 {
             targets.push(ImprovementTarget {
                id: format!("perf-tools-{}", chrono::Utc::now().timestamp()),
                source: ImprovementSource::Performance,
                category: ImprovementCategory::Refactor,
                description: "Agent is using too many tool calls per task. Consider consolidating file search and edit tools or increasing token extraction limits.".to_string(),
                target_files: vec!["src/tools/file.rs".to_string()],
                priority: 0.8,
                confidence: 0.7,
            });
        }
        
        // Example: Low verification rate indicates tests are not passing on the first try
        if snapshot.first_try_verification_rate < 0.4 {
             targets.push(ImprovementTarget {
                id: format!("perf-verify-{}", chrono::Utc::now().timestamp()),
                source: ImprovementSource::Testing,
                category: ImprovementCategory::NewFeature,
                description: "First-try test verification is too low. Implement a test-driven development (TDD) harness tool that forces tests to be written before implementation.".to_string(),
                target_files: vec!["src/agent/execution.rs".to_string()],
                priority: 0.85,
                confidence: 0.75,
            });
        }
        
        targets
    }
"""

content = content.replace('    pub fn analyze_self(&self) -> Vec<ImprovementTarget> {', introspection_method + '\n    pub fn analyze_self(&self) -> Vec<ImprovementTarget> {')

# Add call inside analyze_self
content = content.replace('        targets.extend(self.scan_code_quality());', '        targets.extend(self.scan_code_quality());\n        targets.extend(self.introspect_performance());')

with open('src/cognitive/self_edit.rs', 'w') as f:
    f.write(content)
