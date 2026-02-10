//! Monorepo Management Tools
//!
//! Provides affected target analysis, dependency graph visualization,
//! selective CI triggering, and cross-package refactoring for monorepos.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

// ============================================================================
// Package & Dependency Management
// ============================================================================

/// Monorepo build system type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildSystem {
    Bazel,
    Nx,
    Turborepo,
    Rush,
    Lerna,
    Pnpm,
    Cargo,
    Custom,
}

impl BuildSystem {
    pub fn config_file(&self) -> &str {
        match self {
            BuildSystem::Bazel => "BUILD.bazel",
            BuildSystem::Nx => "nx.json",
            BuildSystem::Turborepo => "turbo.json",
            BuildSystem::Rush => "rush.json",
            BuildSystem::Lerna => "lerna.json",
            BuildSystem::Pnpm => "pnpm-workspace.yaml",
            BuildSystem::Cargo => "Cargo.toml",
            BuildSystem::Custom => "",
        }
    }
}

/// Package type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PackageType {
    Library,
    Application,
    Service,
    Tool,
    Config,
    Docs,
    Test,
}

/// Package in the monorepo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// Package name
    pub name: String,
    /// Package path relative to root
    pub path: PathBuf,
    /// Package type
    pub package_type: PackageType,
    /// Package version
    pub version: String,
    /// Direct dependencies
    pub dependencies: Vec<String>,
    /// Dev dependencies
    pub dev_dependencies: Vec<String>,
    /// Build targets
    pub targets: Vec<String>,
    /// Owners/maintainers
    pub owners: Vec<String>,
    /// Tags for categorization
    pub tags: HashSet<String>,
}

impl Package {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            package_type: PackageType::Library,
            version: "0.1.0".to_string(),
            dependencies: Vec::new(),
            dev_dependencies: Vec::new(),
            targets: Vec::new(),
            owners: Vec::new(),
            tags: HashSet::new(),
        }
    }

    pub fn with_type(mut self, package_type: PackageType) -> Self {
        self.package_type = package_type;
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn add_dependency(&mut self, dep: impl Into<String>) {
        self.dependencies.push(dep.into());
    }

    pub fn add_dev_dependency(&mut self, dep: impl Into<String>) {
        self.dev_dependencies.push(dep.into());
    }

    pub fn add_tag(&mut self, tag: impl Into<String>) {
        self.tags.insert(tag.into());
    }

    pub fn all_dependencies(&self) -> Vec<&String> {
        self.dependencies
            .iter()
            .chain(self.dev_dependencies.iter())
            .collect()
    }
}

/// Dependency graph for the monorepo
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Build system
    pub build_system: BuildSystem,
    /// Packages
    pub packages: HashMap<String, Package>,
    /// Adjacency list (package -> dependents)
    dependents: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    pub fn new(build_system: BuildSystem) -> Self {
        Self {
            build_system,
            packages: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    pub fn add_package(&mut self, package: Package) {
        let name = package.name.clone();

        // Update dependents map
        for dep in &package.dependencies {
            self.dependents
                .entry(dep.clone())
                .or_default()
                .insert(name.clone());
        }

        self.packages.insert(name, package);
    }

    pub fn get_package(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    pub fn get_dependents(&self, name: &str) -> Vec<&String> {
        self.dependents
            .get(name)
            .map(|deps| deps.iter().collect())
            .unwrap_or_default()
    }

    pub fn get_dependencies(&self, name: &str) -> Vec<&String> {
        self.packages
            .get(name)
            .map(|p| p.dependencies.iter().collect())
            .unwrap_or_default()
    }

    pub fn topological_sort(&self) -> Result<Vec<String>, String> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut result = Vec::new();

        // Initialize in-degree
        for name in self.packages.keys() {
            in_degree.insert(name, 0);
        }

        // Calculate in-degree
        for package in self.packages.values() {
            for dep in &package.dependencies {
                if let Some(count) = in_degree.get_mut(dep.as_str()) {
                    *count += 1;
                }
            }
        }

        // Find all nodes with in-degree 0
        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();

        while let Some(name) = queue.pop_front() {
            result.push(name.to_string());

            if let Some(package) = self.packages.get(name) {
                for dep in &package.dependencies {
                    if let Some(count) = in_degree.get_mut(dep.as_str()) {
                        *count -= 1;
                        if *count == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        if result.len() != self.packages.len() {
            return Err("Cycle detected in dependency graph".to_string());
        }

        result.reverse();
        Ok(result)
    }

    pub fn packages_by_type(&self, package_type: PackageType) -> Vec<&Package> {
        self.packages
            .values()
            .filter(|p| p.package_type == package_type)
            .collect()
    }

    pub fn packages_by_tag(&self, tag: &str) -> Vec<&Package> {
        self.packages
            .values()
            .filter(|p| p.tags.contains(tag))
            .collect()
    }

    pub fn leaf_packages(&self) -> Vec<&Package> {
        self.packages
            .values()
            .filter(|p| p.dependencies.is_empty())
            .collect()
    }

    pub fn root_packages(&self) -> Vec<&Package> {
        self.packages
            .values()
            .filter(|p| {
                !self.dependents.contains_key(&p.name) || self.dependents[&p.name].is_empty()
            })
            .collect()
    }
}

// ============================================================================
// Affected Target Analysis
// ============================================================================

/// Change type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// File change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// File path
    pub path: PathBuf,
    /// Change type
    pub change_type: ChangeType,
    /// Lines added
    pub lines_added: u32,
    /// Lines deleted
    pub lines_deleted: u32,
}

impl FileChange {
    pub fn new(path: impl Into<PathBuf>, change_type: ChangeType) -> Self {
        Self {
            path: path.into(),
            change_type,
            lines_added: 0,
            lines_deleted: 0,
        }
    }

    pub fn with_stats(mut self, added: u32, deleted: u32) -> Self {
        self.lines_added = added;
        self.lines_deleted = deleted;
        self
    }
}

/// Affected analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedAnalysis {
    /// Base reference (commit, branch)
    pub base_ref: String,
    /// Head reference
    pub head_ref: String,
    /// Changed files
    pub changed_files: Vec<FileChange>,
    /// Directly affected packages
    pub directly_affected: HashSet<String>,
    /// Transitively affected packages
    pub transitively_affected: HashSet<String>,
    /// Affected targets
    pub affected_targets: Vec<String>,
}

impl AffectedAnalysis {
    pub fn new(base_ref: impl Into<String>, head_ref: impl Into<String>) -> Self {
        Self {
            base_ref: base_ref.into(),
            head_ref: head_ref.into(),
            changed_files: Vec::new(),
            directly_affected: HashSet::new(),
            transitively_affected: HashSet::new(),
            affected_targets: Vec::new(),
        }
    }

    pub fn total_affected(&self) -> usize {
        self.directly_affected
            .union(&self.transitively_affected)
            .count()
    }

    pub fn all_affected(&self) -> HashSet<&String> {
        self.directly_affected
            .iter()
            .chain(self.transitively_affected.iter())
            .collect()
    }
}

/// Affected target analyzer
#[derive(Debug, Clone)]
pub struct AffectedAnalyzer {
    /// Dependency graph
    graph: DependencyGraph,
    /// File to package mapping
    file_package_map: HashMap<PathBuf, String>,
}

impl AffectedAnalyzer {
    pub fn new(graph: DependencyGraph) -> Self {
        let mut file_package_map = HashMap::new();

        // Build file to package mapping
        for package in graph.packages.values() {
            // Map package path to package name
            file_package_map.insert(package.path.clone(), package.name.clone());
        }

        Self {
            graph,
            file_package_map,
        }
    }

    pub fn add_file_mapping(&mut self, file: impl Into<PathBuf>, package: impl Into<String>) {
        self.file_package_map.insert(file.into(), package.into());
    }

    pub fn find_package_for_file(&self, file: &PathBuf) -> Option<&String> {
        // Try exact match first
        if let Some(pkg) = self.file_package_map.get(file) {
            return Some(pkg);
        }

        // Try to find package by path prefix
        for (pkg_path, pkg_name) in &self.file_package_map {
            if file.starts_with(pkg_path) {
                return Some(pkg_name);
            }
        }

        None
    }

    pub fn analyze(&self, changes: &[FileChange]) -> AffectedAnalysis {
        let mut analysis = AffectedAnalysis::new("base", "head");
        analysis.changed_files = changes.to_vec();

        // Find directly affected packages
        for change in changes {
            if let Some(pkg) = self.find_package_for_file(&change.path) {
                analysis.directly_affected.insert(pkg.clone());
            }
        }

        // Find transitively affected packages
        for pkg in &analysis.directly_affected {
            self.find_affected_recursive(pkg, &mut analysis.transitively_affected);
        }

        // Remove directly affected from transitively affected
        for pkg in &analysis.directly_affected {
            analysis.transitively_affected.remove(pkg);
        }

        // Collect affected targets - clone the names first to avoid borrow conflict
        let all_affected: Vec<String> = analysis
            .directly_affected
            .iter()
            .chain(analysis.transitively_affected.iter())
            .cloned()
            .collect();

        for pkg_name in all_affected {
            if let Some(package) = self.graph.get_package(&pkg_name) {
                for target in &package.targets {
                    analysis.affected_targets.push(target.clone());
                }
            }
        }

        analysis
    }

    fn find_affected_recursive(&self, package: &str, affected: &mut HashSet<String>) {
        for dependent in self.graph.get_dependents(package) {
            if affected.insert(dependent.clone()) {
                self.find_affected_recursive(dependent, affected);
            }
        }
    }
}

// ============================================================================
// Selective CI Triggering
// ============================================================================

/// CI pipeline type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineType {
    Build,
    Test,
    Lint,
    Deploy,
    Release,
    Custom,
}

/// CI pipeline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    /// Pipeline name
    pub name: String,
    /// Pipeline type
    pub pipeline_type: PipelineType,
    /// Packages this pipeline covers
    pub packages: Vec<String>,
    /// Tags to match
    pub tags: Vec<String>,
    /// Exclude patterns
    pub exclude_patterns: Vec<String>,
    /// Commands to run
    pub commands: Vec<String>,
    /// Dependencies (other pipelines)
    pub depends_on: Vec<String>,
    /// Estimated duration in seconds
    pub estimated_duration_secs: u32,
}

impl Pipeline {
    pub fn new(name: impl Into<String>, pipeline_type: PipelineType) -> Self {
        Self {
            name: name.into(),
            pipeline_type,
            packages: Vec::new(),
            tags: Vec::new(),
            exclude_patterns: Vec::new(),
            commands: Vec::new(),
            depends_on: Vec::new(),
            estimated_duration_secs: 0,
        }
    }

    pub fn for_packages(mut self, packages: Vec<String>) -> Self {
        self.packages = packages;
        self
    }

    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.commands.push(command.into());
        self
    }

    pub fn depends_on(mut self, pipeline: impl Into<String>) -> Self {
        self.depends_on.push(pipeline.into());
        self
    }
}

/// CI trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiTrigger {
    /// Trigger name
    pub name: String,
    /// Path patterns that trigger this
    pub path_patterns: Vec<String>,
    /// Pipelines to run
    pub pipelines: Vec<String>,
    /// Skip condition
    pub skip_if: Option<String>,
}

impl CiTrigger {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path_patterns: Vec::new(),
            pipelines: Vec::new(),
            skip_if: None,
        }
    }

    pub fn on_path(mut self, pattern: impl Into<String>) -> Self {
        self.path_patterns.push(pattern.into());
        self
    }

    pub fn run_pipeline(mut self, pipeline: impl Into<String>) -> Self {
        self.pipelines.push(pipeline.into());
        self
    }
}

/// CI configuration generator
#[derive(Debug, Clone)]
pub struct CiConfigGenerator {
    /// Pipelines
    pub pipelines: HashMap<String, Pipeline>,
    /// Triggers
    pub triggers: Vec<CiTrigger>,
    /// Global configuration
    pub global_config: HashMap<String, String>,
}

impl CiConfigGenerator {
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            triggers: Vec::new(),
            global_config: HashMap::new(),
        }
    }

    pub fn add_pipeline(&mut self, pipeline: Pipeline) {
        self.pipelines.insert(pipeline.name.clone(), pipeline);
    }

    pub fn add_trigger(&mut self, trigger: CiTrigger) {
        self.triggers.push(trigger);
    }

    pub fn set_config(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.global_config.insert(key.into(), value.into());
    }

    pub fn pipelines_for_changes(&self, changes: &[FileChange]) -> Vec<&Pipeline> {
        let mut triggered = HashSet::new();

        for trigger in &self.triggers {
            for change in changes {
                let path_str = change.path.to_string_lossy();
                for pattern in &trigger.path_patterns {
                    if path_str.contains(pattern) || Self::matches_glob(pattern, &path_str) {
                        for pipeline in &trigger.pipelines {
                            triggered.insert(pipeline.clone());
                        }
                    }
                }
            }
        }

        triggered
            .iter()
            .filter_map(|name| self.pipelines.get(name))
            .collect()
    }

    fn matches_glob(pattern: &str, path: &str) -> bool {
        // Simple glob matching
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                return path.starts_with(parts[0]) && path.ends_with(parts[1]);
            }
        }
        path.contains(pattern)
    }

    pub fn estimate_total_duration(&self, pipelines: &[&Pipeline]) -> u32 {
        // Simple estimation - could be improved with parallel execution analysis
        pipelines.iter().map(|p| p.estimated_duration_secs).sum()
    }

    pub fn generate_github_actions(&self, pipelines: &[&Pipeline]) -> String {
        let mut yaml = String::new();
        yaml.push_str("name: CI\n\n");
        yaml.push_str("on:\n");
        yaml.push_str("  push:\n");
        yaml.push_str("    branches: [main]\n");
        yaml.push_str("  pull_request:\n\n");
        yaml.push_str("jobs:\n");

        for pipeline in pipelines {
            yaml.push_str(&format!("  {}:\n", pipeline.name.replace(' ', "-")));
            yaml.push_str("    runs-on: ubuntu-latest\n");

            if !pipeline.depends_on.is_empty() {
                yaml.push_str("    needs:\n");
                for dep in &pipeline.depends_on {
                    yaml.push_str(&format!("      - {}\n", dep.replace(' ', "-")));
                }
            }

            yaml.push_str("    steps:\n");
            yaml.push_str("      - uses: actions/checkout@v4\n");

            for command in &pipeline.commands {
                yaml.push_str(&format!("      - run: {}\n", command));
            }

            yaml.push('\n');
        }

        yaml
    }
}

impl Default for CiConfigGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Cross-Package Refactoring
// ============================================================================

/// Refactoring operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefactoringType {
    RenameSymbol,
    MoveFile,
    MovePackage,
    ExtractPackage,
    MergePackages,
    UpdateImports,
    UpdateVersions,
}

/// Refactoring operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringOperation {
    /// Operation type
    pub operation_type: RefactoringType,
    /// Source (old name/path)
    pub source: String,
    /// Target (new name/path)
    pub target: String,
    /// Affected packages
    pub affected_packages: Vec<String>,
    /// File changes to make
    pub file_changes: Vec<RefactoringChange>,
}

impl RefactoringOperation {
    pub fn rename_symbol(old_name: impl Into<String>, new_name: impl Into<String>) -> Self {
        Self {
            operation_type: RefactoringType::RenameSymbol,
            source: old_name.into(),
            target: new_name.into(),
            affected_packages: Vec::new(),
            file_changes: Vec::new(),
        }
    }

    pub fn move_file(old_path: impl Into<String>, new_path: impl Into<String>) -> Self {
        Self {
            operation_type: RefactoringType::MoveFile,
            source: old_path.into(),
            target: new_path.into(),
            affected_packages: Vec::new(),
            file_changes: Vec::new(),
        }
    }

    pub fn move_package(old_name: impl Into<String>, new_name: impl Into<String>) -> Self {
        Self {
            operation_type: RefactoringType::MovePackage,
            source: old_name.into(),
            target: new_name.into(),
            affected_packages: Vec::new(),
            file_changes: Vec::new(),
        }
    }
}

/// Individual file change in a refactoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringChange {
    /// File path
    pub file: PathBuf,
    /// Line number
    pub line: u32,
    /// Column number
    pub column: u32,
    /// Original text
    pub original: String,
    /// Replacement text
    pub replacement: String,
}

impl RefactoringChange {
    pub fn new(
        file: impl Into<PathBuf>,
        line: u32,
        column: u32,
        original: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            original: original.into(),
            replacement: replacement.into(),
        }
    }
}

/// Cross-package refactoring planner
#[derive(Debug, Clone)]
pub struct RefactoringPlanner {
    /// Dependency graph
    graph: DependencyGraph,
    /// Planned operations
    operations: Vec<RefactoringOperation>,
}

impl RefactoringPlanner {
    pub fn new(graph: DependencyGraph) -> Self {
        Self {
            graph,
            operations: Vec::new(),
        }
    }

    pub fn plan_rename_symbol(
        &mut self,
        old_name: &str,
        new_name: &str,
        source_package: &str,
    ) -> RefactoringOperation {
        let mut operation = RefactoringOperation::rename_symbol(old_name, new_name);

        // Find all packages that depend on the source package
        operation.affected_packages.push(source_package.to_string());

        let mut to_check = vec![source_package.to_string()];
        let mut checked = HashSet::new();

        while let Some(pkg) = to_check.pop() {
            if !checked.insert(pkg.clone()) {
                continue;
            }

            for dependent in self.graph.get_dependents(&pkg) {
                operation.affected_packages.push(dependent.clone());
                to_check.push(dependent.clone());
            }
        }

        self.operations.push(operation.clone());
        operation
    }

    pub fn plan_move_package(&mut self, old_name: &str, new_name: &str) -> RefactoringOperation {
        let mut operation = RefactoringOperation::move_package(old_name, new_name);

        // Find all packages that depend on the package being moved
        operation.affected_packages.push(old_name.to_string());

        let mut to_check = vec![old_name.to_string()];
        let mut checked = HashSet::new();

        while let Some(pkg) = to_check.pop() {
            if !checked.insert(pkg.clone()) {
                continue;
            }

            for dependent in self.graph.get_dependents(&pkg) {
                operation.affected_packages.push(dependent.clone());
                to_check.push(dependent.clone());
            }
        }

        self.operations.push(operation.clone());
        operation
    }

    pub fn plan_extract_package(
        &mut self,
        source_package: &str,
        new_package: &str,
        files: Vec<String>,
    ) -> RefactoringOperation {
        let mut operation = RefactoringOperation {
            operation_type: RefactoringType::ExtractPackage,
            source: source_package.to_string(),
            target: new_package.to_string(),
            affected_packages: Vec::new(),
            file_changes: Vec::new(),
        };

        // The source package is affected
        operation.affected_packages.push(source_package.to_string());

        // All dependents of source package may need updates
        for dependent in self.graph.get_dependents(source_package) {
            operation.affected_packages.push(dependent.clone());
        }

        // Create file changes for the extracted files
        for file in files {
            operation.file_changes.push(RefactoringChange::new(
                &file,
                0,
                0,
                format!("from {}", source_package),
                format!("from {}", new_package),
            ));
        }

        self.operations.push(operation.clone());
        operation
    }

    pub fn operations(&self) -> &[RefactoringOperation] {
        &self.operations
    }

    pub fn clear(&mut self) {
        self.operations.clear();
    }

    pub fn estimate_impact(&self) -> (usize, usize) {
        let packages: HashSet<_> = self
            .operations
            .iter()
            .flat_map(|op| op.affected_packages.iter())
            .collect();

        let changes: usize = self.operations.iter().map(|op| op.file_changes.len()).sum();

        (packages.len(), changes)
    }
}

// ============================================================================
// Graph Visualization
// ============================================================================

/// Graph visualization format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    Dot,
    Mermaid,
    D3Json,
}

/// Graph visualizer
#[derive(Debug, Clone)]
pub struct GraphVisualizer {
    graph: DependencyGraph,
}

impl GraphVisualizer {
    pub fn new(graph: DependencyGraph) -> Self {
        Self { graph }
    }

    pub fn to_dot(&self) -> String {
        let mut dot = String::new();
        dot.push_str("digraph dependencies {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  node [shape=box];\n\n");

        // Add nodes with styling based on type
        for package in self.graph.packages.values() {
            let color = match package.package_type {
                PackageType::Application => "lightblue",
                PackageType::Service => "lightgreen",
                PackageType::Library => "lightyellow",
                PackageType::Tool => "lightgray",
                _ => "white",
            };
            dot.push_str(&format!(
                "  \"{}\" [fillcolor=\"{}\", style=filled];\n",
                package.name, color
            ));
        }

        dot.push('\n');

        // Add edges
        for package in self.graph.packages.values() {
            for dep in &package.dependencies {
                dot.push_str(&format!("  \"{}\" -> \"{}\";\n", package.name, dep));
            }
        }

        dot.push_str("}\n");
        dot
    }

    pub fn to_mermaid(&self) -> String {
        let mut mermaid = String::new();
        mermaid.push_str("graph LR\n");

        // Add nodes
        for package in self.graph.packages.values() {
            let shape = match package.package_type {
                PackageType::Application => format!("{}([{}])", package.name, package.name),
                PackageType::Service => format!("{}{{{{ {} }}}}", package.name, package.name),
                PackageType::Library => format!("{}[{}]", package.name, package.name),
                _ => format!("{}[{}]", package.name, package.name),
            };
            mermaid.push_str(&format!("    {}\n", shape));
        }

        // Add edges
        for package in self.graph.packages.values() {
            for dep in &package.dependencies {
                mermaid.push_str(&format!("    {} --> {}\n", package.name, dep));
            }
        }

        mermaid
    }

    pub fn to_d3_json(&self) -> String {
        let mut nodes = Vec::new();
        let mut links = Vec::new();

        for package in self.graph.packages.values() {
            nodes.push(format!(
                r#"{{"id": "{}", "group": {:?}}}"#,
                package.name, package.package_type as u8
            ));

            for dep in &package.dependencies {
                links.push(format!(
                    r#"{{"source": "{}", "target": "{}"}}"#,
                    package.name, dep
                ));
            }
        }

        format!(
            r#"{{"nodes": [{}], "links": [{}]}}"#,
            nodes.join(", "),
            links.join(", ")
        )
    }

    pub fn generate(&self, format: GraphFormat) -> String {
        match format {
            GraphFormat::Dot => self.to_dot(),
            GraphFormat::Mermaid => self.to_mermaid(),
            GraphFormat::D3Json => self.to_d3_json(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_graph() -> DependencyGraph {
        let mut graph = DependencyGraph::new(BuildSystem::Nx);

        let core = Package::new("core", "packages/core").with_type(PackageType::Library);
        graph.add_package(core.clone());

        let mut utils = Package::new("utils", "packages/utils").with_type(PackageType::Library);
        utils.add_dependency("core");
        graph.add_package(utils);

        let mut app = Package::new("app", "apps/app").with_type(PackageType::Application);
        app.add_dependency("core");
        app.add_dependency("utils");
        graph.add_package(app);

        graph
    }

    #[test]
    fn test_build_system_config() {
        assert_eq!(BuildSystem::Bazel.config_file(), "BUILD.bazel");
        assert_eq!(BuildSystem::Nx.config_file(), "nx.json");
        assert_eq!(BuildSystem::Turborepo.config_file(), "turbo.json");
    }

    #[test]
    fn test_package_creation() {
        let mut pkg = Package::new("my-lib", "packages/my-lib")
            .with_type(PackageType::Library)
            .with_version("1.0.0");

        pkg.add_dependency("core");
        pkg.add_dev_dependency("test-utils");
        pkg.add_tag("frontend");

        assert_eq!(pkg.name, "my-lib");
        assert_eq!(pkg.dependencies.len(), 1);
        assert_eq!(pkg.dev_dependencies.len(), 1);
        assert!(pkg.tags.contains("frontend"));
    }

    #[test]
    fn test_dependency_graph() {
        let graph = create_test_graph();

        assert_eq!(graph.packages.len(), 3);
        assert!(graph.get_package("core").is_some());
    }

    #[test]
    fn test_graph_dependents() {
        let graph = create_test_graph();

        let core_dependents = graph.get_dependents("core");
        assert_eq!(core_dependents.len(), 2); // utils and app
    }

    #[test]
    fn test_topological_sort() {
        let graph = create_test_graph();

        let order = graph.topological_sort().unwrap();
        // core should come before utils and app
        let core_idx = order.iter().position(|x| x == "core").unwrap();
        let utils_idx = order.iter().position(|x| x == "utils").unwrap();
        let app_idx = order.iter().position(|x| x == "app").unwrap();

        assert!(core_idx < utils_idx);
        assert!(core_idx < app_idx);
    }

    #[test]
    fn test_leaf_and_root_packages() {
        let graph = create_test_graph();

        let leaves = graph.leaf_packages();
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].name, "core");

        let roots = graph.root_packages();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].name, "app");
    }

    #[test]
    fn test_file_change() {
        let change =
            FileChange::new("packages/core/src/lib.rs", ChangeType::Modified).with_stats(10, 5);

        assert_eq!(change.lines_added, 10);
        assert_eq!(change.lines_deleted, 5);
    }

    #[test]
    fn test_affected_analysis() {
        let graph = create_test_graph();
        let analyzer = AffectedAnalyzer::new(graph);

        let changes = vec![FileChange::new(
            "packages/core/src/lib.rs",
            ChangeType::Modified,
        )];

        let analysis = analyzer.analyze(&changes);

        // core is directly affected
        assert!(analysis.directly_affected.contains("core"));
        // utils and app are transitively affected
        assert!(analysis.transitively_affected.contains("utils"));
        assert!(analysis.transitively_affected.contains("app"));
    }

    #[test]
    fn test_pipeline() {
        let pipeline = Pipeline::new("test", PipelineType::Test)
            .for_packages(vec!["core".to_string()])
            .with_command("npm test")
            .depends_on("build");

        assert_eq!(pipeline.name, "test");
        assert_eq!(pipeline.pipeline_type, PipelineType::Test);
        assert_eq!(pipeline.commands.len(), 1);
        assert_eq!(pipeline.depends_on.len(), 1);
    }

    #[test]
    fn test_ci_trigger() {
        let trigger = CiTrigger::new("core-changes")
            .on_path("packages/core/*")
            .run_pipeline("test-core");

        assert_eq!(trigger.path_patterns.len(), 1);
        assert_eq!(trigger.pipelines.len(), 1);
    }

    #[test]
    fn test_ci_config_generator() {
        let mut generator = CiConfigGenerator::new();

        generator.add_pipeline(
            Pipeline::new("build", PipelineType::Build).with_command("npm run build"),
        );

        generator.add_pipeline(
            Pipeline::new("test", PipelineType::Test)
                .with_command("npm test")
                .depends_on("build"),
        );

        generator.add_trigger(
            CiTrigger::new("all")
                .on_path("packages/")
                .run_pipeline("build")
                .run_pipeline("test"),
        );

        let changes = vec![FileChange::new(
            "packages/core/index.ts",
            ChangeType::Modified,
        )];
        let pipelines = generator.pipelines_for_changes(&changes);

        assert_eq!(pipelines.len(), 2);
    }

    #[test]
    fn test_github_actions_generation() {
        let mut generator = CiConfigGenerator::new();

        generator.add_pipeline(
            Pipeline::new("build", PipelineType::Build).with_command("npm run build"),
        );

        let pipelines: Vec<_> = generator.pipelines.values().collect();
        let yaml = generator.generate_github_actions(&pipelines);

        assert!(yaml.contains("name: CI"));
        assert!(yaml.contains("build:"));
        assert!(yaml.contains("npm run build"));
    }

    #[test]
    fn test_refactoring_operation() {
        let op = RefactoringOperation::rename_symbol("oldFunc", "newFunc");

        assert_eq!(op.operation_type, RefactoringType::RenameSymbol);
        assert_eq!(op.source, "oldFunc");
        assert_eq!(op.target, "newFunc");
    }

    #[test]
    fn test_refactoring_planner() {
        let graph = create_test_graph();
        let mut planner = RefactoringPlanner::new(graph);

        let op = planner.plan_rename_symbol("MyClass", "NewClass", "core");

        assert!(op.affected_packages.contains(&"core".to_string()));
        assert!(op.affected_packages.contains(&"utils".to_string()));
        assert!(op.affected_packages.contains(&"app".to_string()));
    }

    #[test]
    fn test_refactoring_impact() {
        let graph = create_test_graph();
        let mut planner = RefactoringPlanner::new(graph);

        planner.plan_rename_symbol("Foo", "Bar", "core");

        let (packages, _changes) = planner.estimate_impact();
        assert!(packages > 0);
    }

    #[test]
    fn test_graph_to_dot() {
        let graph = create_test_graph();
        let visualizer = GraphVisualizer::new(graph);

        let dot = visualizer.to_dot();
        assert!(dot.contains("digraph dependencies"));
        assert!(dot.contains("core"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_graph_to_mermaid() {
        let graph = create_test_graph();
        let visualizer = GraphVisualizer::new(graph);

        let mermaid = visualizer.to_mermaid();
        assert!(mermaid.contains("graph LR"));
        assert!(mermaid.contains("-->"));
    }

    #[test]
    fn test_graph_to_d3() {
        let graph = create_test_graph();
        let visualizer = GraphVisualizer::new(graph);

        let json = visualizer.to_d3_json();
        assert!(json.contains("nodes"));
        assert!(json.contains("links"));
    }

    #[test]
    fn test_packages_by_type() {
        let graph = create_test_graph();

        let libs = graph.packages_by_type(PackageType::Library);
        assert_eq!(libs.len(), 2); // core and utils

        let apps = graph.packages_by_type(PackageType::Application);
        assert_eq!(apps.len(), 1); // app
    }

    #[test]
    fn test_packages_by_tag() {
        let mut graph = DependencyGraph::new(BuildSystem::Pnpm);

        let mut pkg1 = Package::new("frontend", "packages/frontend");
        pkg1.add_tag("ui");
        graph.add_package(pkg1);

        let mut pkg2 = Package::new("backend", "packages/backend");
        pkg2.add_tag("api");
        graph.add_package(pkg2);

        let ui_packages = graph.packages_by_tag("ui");
        assert_eq!(ui_packages.len(), 1);
        assert_eq!(ui_packages[0].name, "frontend");
    }

    #[test]
    fn test_extract_package_refactoring() {
        let graph = create_test_graph();
        let mut planner = RefactoringPlanner::new(graph);

        let op = planner.plan_extract_package("core", "core-utils", vec!["utils.ts".to_string()]);

        assert_eq!(op.operation_type, RefactoringType::ExtractPackage);
        assert_eq!(op.file_changes.len(), 1);
    }

    #[test]
    fn test_build_system_all_variants() {
        let variants = [
            BuildSystem::Bazel,
            BuildSystem::Nx,
            BuildSystem::Turborepo,
            BuildSystem::Rush,
            BuildSystem::Lerna,
            BuildSystem::Pnpm,
            BuildSystem::Cargo,
            BuildSystem::Custom,
        ];

        for variant in variants {
            let _ = variant.config_file();
            let _ = serde_json::to_string(&variant).unwrap();
        }
    }

    #[test]
    fn test_build_system_serde_roundtrip() {
        let system = BuildSystem::Cargo;
        let json = serde_json::to_string(&system).unwrap();
        let parsed: BuildSystem = serde_json::from_str(&json).unwrap();
        assert_eq!(system, parsed);
    }

    #[test]
    fn test_build_system_clone() {
        let system = BuildSystem::Nx;
        let cloned = system;
        assert_eq!(system, cloned);
    }

    #[test]
    fn test_build_system_debug() {
        let system = BuildSystem::Turborepo;
        let debug_str = format!("{:?}", system);
        assert!(debug_str.contains("Turborepo"));
    }

    #[test]
    fn test_build_system_config_files() {
        assert_eq!(BuildSystem::Rush.config_file(), "rush.json");
        assert_eq!(BuildSystem::Lerna.config_file(), "lerna.json");
        assert_eq!(BuildSystem::Custom.config_file(), "");
    }

    #[test]
    fn test_package_type_all_variants() {
        let variants = [
            PackageType::Library,
            PackageType::Application,
            PackageType::Service,
            PackageType::Tool,
            PackageType::Config,
            PackageType::Docs,
            PackageType::Test,
        ];

        for variant in variants {
            let _ = serde_json::to_string(&variant).unwrap();
        }
    }

    #[test]
    fn test_package_type_serde_roundtrip() {
        let pkg_type = PackageType::Service;
        let json = serde_json::to_string(&pkg_type).unwrap();
        let parsed: PackageType = serde_json::from_str(&json).unwrap();
        assert_eq!(pkg_type, parsed);
    }

    #[test]
    fn test_package_all_dependencies() {
        let mut pkg = Package::new("test", "packages/test");
        pkg.add_dependency("core");
        pkg.add_dependency("utils");
        pkg.add_dev_dependency("test-utils");

        let all = pkg.all_dependencies();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_package_clone() {
        let mut pkg = Package::new("test", "packages/test");
        pkg.add_dependency("core");

        let cloned = pkg.clone();
        assert_eq!(pkg.name, cloned.name);
        assert_eq!(pkg.dependencies, cloned.dependencies);
    }

    #[test]
    fn test_package_serde_roundtrip() {
        let mut pkg = Package::new("test-pkg", "packages/test");
        pkg.add_dependency("core");
        pkg.add_tag("frontend");

        let json = serde_json::to_string(&pkg).unwrap();
        let parsed: Package = serde_json::from_str(&json).unwrap();

        assert_eq!(pkg.name, parsed.name);
        assert_eq!(pkg.dependencies, parsed.dependencies);
    }

    #[test]
    fn test_package_with_version() {
        let pkg = Package::new("test", "packages/test").with_version("2.0.0");

        assert_eq!(pkg.version, "2.0.0");
    }

    #[test]
    fn test_package_default_type() {
        let pkg = Package::new("test", "packages/test");
        assert_eq!(pkg.package_type, PackageType::Library);
    }

    #[test]
    fn test_package_debug() {
        let pkg = Package::new("debug-test", "packages/debug");
        let debug_str = format!("{:?}", pkg);
        assert!(debug_str.contains("Package"));
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_dependency_graph_clone() {
        let graph = create_test_graph();
        let cloned = graph.clone();

        assert_eq!(graph.packages.len(), cloned.packages.len());
        assert_eq!(graph.build_system, cloned.build_system);
    }

    #[test]
    fn test_dependency_graph_debug() {
        let graph = create_test_graph();
        let debug_str = format!("{:?}", graph);
        assert!(debug_str.contains("DependencyGraph"));
    }

    #[test]
    fn test_dependency_graph_get_dependencies() {
        let graph = create_test_graph();

        let deps = graph.get_dependencies("app");
        assert_eq!(deps.len(), 2);
    }

    #[test]
    fn test_dependency_graph_get_dependencies_not_found() {
        let graph = create_test_graph();

        let deps = graph.get_dependencies("nonexistent");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_dependency_graph_get_dependents_not_found() {
        let graph = create_test_graph();

        let deps = graph.get_dependents("nonexistent");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_change_type_all_variants() {
        let variants = [
            ChangeType::Added,
            ChangeType::Modified,
            ChangeType::Deleted,
            ChangeType::Renamed,
        ];

        for variant in variants {
            let _ = serde_json::to_string(&variant).unwrap();
        }
    }

    #[test]
    fn test_change_type_serde_roundtrip() {
        let change_type = ChangeType::Renamed;
        let json = serde_json::to_string(&change_type).unwrap();
        let parsed: ChangeType = serde_json::from_str(&json).unwrap();
        assert_eq!(change_type, parsed);
    }

    #[test]
    fn test_file_change_clone() {
        let change = FileChange::new("test.rs", ChangeType::Modified).with_stats(10, 5);

        let cloned = change.clone();
        assert_eq!(change.lines_added, cloned.lines_added);
    }

    #[test]
    fn test_file_change_serde_roundtrip() {
        let change = FileChange::new("src/lib.rs", ChangeType::Added).with_stats(100, 0);

        let json = serde_json::to_string(&change).unwrap();
        let parsed: FileChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change.lines_added, parsed.lines_added);
        assert_eq!(change.change_type, parsed.change_type);
    }

    #[test]
    fn test_affected_analysis_total_affected() {
        let mut analysis = AffectedAnalysis::new("main", "feature");
        analysis.directly_affected.insert("core".to_string());
        analysis.directly_affected.insert("utils".to_string());
        analysis.transitively_affected.insert("app".to_string());

        assert_eq!(analysis.total_affected(), 3);
    }

    #[test]
    fn test_affected_analysis_all_affected() {
        let mut analysis = AffectedAnalysis::new("main", "feature");
        analysis.directly_affected.insert("core".to_string());
        analysis.transitively_affected.insert("app".to_string());

        let all = analysis.all_affected();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_affected_analysis_serde() {
        let analysis = AffectedAnalysis::new("base", "head");

        let json = serde_json::to_string(&analysis).unwrap();
        let parsed: AffectedAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(analysis.base_ref, parsed.base_ref);
    }

    #[test]
    fn test_affected_analyzer_add_file_mapping() {
        let graph = create_test_graph();
        let mut analyzer = AffectedAnalyzer::new(graph);

        analyzer.add_file_mapping("src/special.rs", "special-pkg");

        let pkg = analyzer.find_package_for_file(&PathBuf::from("src/special.rs"));
        assert_eq!(pkg, Some(&"special-pkg".to_string()));
    }

    #[test]
    fn test_affected_analyzer_find_package_by_prefix() {
        let graph = create_test_graph();
        let analyzer = AffectedAnalyzer::new(graph);

        // Looking for file under packages/core should find core package
        let pkg = analyzer.find_package_for_file(&PathBuf::from("packages/core/src/lib.rs"));
        assert_eq!(pkg, Some(&"core".to_string()));
    }

    #[test]
    fn test_pipeline_type_all_variants() {
        let variants = [
            PipelineType::Build,
            PipelineType::Test,
            PipelineType::Lint,
            PipelineType::Deploy,
            PipelineType::Release,
            PipelineType::Custom,
        ];

        for variant in variants {
            let _ = serde_json::to_string(&variant).unwrap();
        }
    }

    #[test]
    fn test_pipeline_clone() {
        let pipeline = Pipeline::new("test", PipelineType::Test).with_command("npm test");

        let cloned = pipeline.clone();
        assert_eq!(pipeline.name, cloned.name);
    }

    #[test]
    fn test_pipeline_serde_roundtrip() {
        let pipeline = Pipeline::new("build", PipelineType::Build)
            .with_command("npm run build")
            .for_packages(vec!["core".to_string()]);

        let json = serde_json::to_string(&pipeline).unwrap();
        let parsed: Pipeline = serde_json::from_str(&json).unwrap();

        assert_eq!(pipeline.name, parsed.name);
        assert_eq!(pipeline.packages, parsed.packages);
    }

    #[test]
    fn test_ci_trigger_clone() {
        let trigger = CiTrigger::new("test").on_path("*.rs").run_pipeline("test");

        let cloned = trigger.clone();
        assert_eq!(trigger.name, cloned.name);
    }

    #[test]
    fn test_ci_trigger_serde_roundtrip() {
        let trigger = CiTrigger::new("all").on_path("packages/*");

        let json = serde_json::to_string(&trigger).unwrap();
        let parsed: CiTrigger = serde_json::from_str(&json).unwrap();

        assert_eq!(trigger.name, parsed.name);
    }

    #[test]
    fn test_ci_trigger_skip_if() {
        let mut trigger = CiTrigger::new("skip-test");
        trigger.skip_if = Some("[skip ci]".to_string());

        assert!(trigger.skip_if.is_some());
    }

    #[test]
    fn test_ci_config_generator_default() {
        let generator = CiConfigGenerator::default();
        assert!(generator.pipelines.is_empty());
        assert!(generator.triggers.is_empty());
    }

    #[test]
    fn test_ci_config_generator_set_config() {
        let mut generator = CiConfigGenerator::new();
        generator.set_config("runner", "ubuntu-latest");

        assert_eq!(
            generator.global_config.get("runner"),
            Some(&"ubuntu-latest".to_string())
        );
    }

    #[test]
    fn test_ci_config_generator_estimate_duration() {
        let mut generator = CiConfigGenerator::new();

        let mut p1 = Pipeline::new("p1", PipelineType::Build);
        p1.estimated_duration_secs = 60;

        let mut p2 = Pipeline::new("p2", PipelineType::Test);
        p2.estimated_duration_secs = 120;

        generator.add_pipeline(p1);
        generator.add_pipeline(p2);

        let pipelines: Vec<_> = generator.pipelines.values().collect();
        let total = generator.estimate_total_duration(&pipelines);
        assert_eq!(total, 180);
    }

    #[test]
    fn test_ci_config_generator_glob_matching() {
        let mut generator = CiConfigGenerator::new();

        generator.add_pipeline(Pipeline::new("test", PipelineType::Test));
        generator.add_trigger(
            CiTrigger::new("test-trigger")
                .on_path("*.ts")
                .run_pipeline("test"),
        );

        let changes = vec![FileChange::new("src/index.ts", ChangeType::Modified)];
        let pipelines = generator.pipelines_for_changes(&changes);

        assert_eq!(pipelines.len(), 1);
    }

    #[test]
    fn test_refactoring_type_all_variants() {
        let variants = [
            RefactoringType::RenameSymbol,
            RefactoringType::MoveFile,
            RefactoringType::MovePackage,
            RefactoringType::ExtractPackage,
            RefactoringType::MergePackages,
            RefactoringType::UpdateImports,
            RefactoringType::UpdateVersions,
        ];

        for variant in variants {
            let _ = serde_json::to_string(&variant).unwrap();
        }
    }

    #[test]
    fn test_refactoring_operation_move_file() {
        let op = RefactoringOperation::move_file("old/path.ts", "new/path.ts");

        assert_eq!(op.operation_type, RefactoringType::MoveFile);
        assert_eq!(op.source, "old/path.ts");
        assert_eq!(op.target, "new/path.ts");
    }

    #[test]
    fn test_refactoring_operation_move_package() {
        let op = RefactoringOperation::move_package("old-pkg", "new-pkg");

        assert_eq!(op.operation_type, RefactoringType::MovePackage);
    }

    #[test]
    fn test_refactoring_operation_serde() {
        let op = RefactoringOperation::rename_symbol("foo", "bar");

        let json = serde_json::to_string(&op).unwrap();
        let parsed: RefactoringOperation = serde_json::from_str(&json).unwrap();

        assert_eq!(op.source, parsed.source);
    }

    #[test]
    fn test_refactoring_change_new() {
        let change = RefactoringChange::new("src/lib.rs", 10, 5, "old_name", "new_name");

        assert_eq!(change.line, 10);
        assert_eq!(change.column, 5);
        assert_eq!(change.original, "old_name");
    }

    #[test]
    fn test_refactoring_change_serde() {
        let change = RefactoringChange::new("file.rs", 1, 1, "a", "b");

        let json = serde_json::to_string(&change).unwrap();
        let parsed: RefactoringChange = serde_json::from_str(&json).unwrap();

        assert_eq!(change.original, parsed.original);
    }

    #[test]
    fn test_refactoring_planner_operations() {
        let graph = create_test_graph();
        let mut planner = RefactoringPlanner::new(graph);

        planner.plan_rename_symbol("a", "b", "core");
        planner.plan_move_package("core", "core-v2");

        assert_eq!(planner.operations().len(), 2);
    }

    #[test]
    fn test_refactoring_planner_clear() {
        let graph = create_test_graph();
        let mut planner = RefactoringPlanner::new(graph);

        planner.plan_rename_symbol("a", "b", "core");
        planner.clear();

        assert!(planner.operations().is_empty());
    }

    #[test]
    fn test_refactoring_planner_plan_move_package() {
        let graph = create_test_graph();
        let mut planner = RefactoringPlanner::new(graph);

        let op = planner.plan_move_package("core", "core-new");

        assert!(op.affected_packages.contains(&"core".to_string()));
        // Dependents should also be affected
        assert!(op.affected_packages.len() > 1);
    }

    #[test]
    fn test_graph_format_variants() {
        let formats = [GraphFormat::Dot, GraphFormat::Mermaid, GraphFormat::D3Json];

        for format in formats {
            assert!(format == format); // Test PartialEq
        }
    }

    #[test]
    fn test_graph_visualizer_generate() {
        let graph = create_test_graph();
        let visualizer = GraphVisualizer::new(graph);

        let dot = visualizer.generate(GraphFormat::Dot);
        assert!(dot.contains("digraph"));

        let mermaid = visualizer.generate(GraphFormat::Mermaid);
        assert!(mermaid.contains("graph LR"));

        let d3 = visualizer.generate(GraphFormat::D3Json);
        assert!(d3.contains("nodes"));
    }

    #[test]
    fn test_graph_visualizer_clone() {
        let graph = create_test_graph();
        let visualizer = GraphVisualizer::new(graph);
        let cloned = visualizer.clone();

        assert_eq!(visualizer.to_dot(), cloned.to_dot());
    }

    #[test]
    fn test_affected_analyzer_clone() {
        let graph = create_test_graph();
        let analyzer = AffectedAnalyzer::new(graph);
        let cloned = analyzer.clone();

        let changes = vec![FileChange::new(
            "packages/core/lib.rs",
            ChangeType::Modified,
        )];
        let analysis1 = analyzer.analyze(&changes);
        let analysis2 = cloned.analyze(&changes);

        assert_eq!(
            analysis1.directly_affected.len(),
            analysis2.directly_affected.len()
        );
    }

    #[test]
    fn test_topological_sort_empty_graph() {
        let graph = DependencyGraph::new(BuildSystem::Cargo);
        let result = graph.topological_sort();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_dependency_graph_packages_by_tag_not_found() {
        let graph = create_test_graph();
        let packages = graph.packages_by_tag("nonexistent");
        assert!(packages.is_empty());
    }

    #[test]
    fn test_affected_analysis_empty() {
        let analysis = AffectedAnalysis::new("main", "feature");

        assert_eq!(analysis.total_affected(), 0);
        assert!(analysis.all_affected().is_empty());
        assert!(analysis.changed_files.is_empty());
    }

    #[test]
    fn test_github_actions_with_dependencies() {
        let mut generator = CiConfigGenerator::new();

        generator.add_pipeline(
            Pipeline::new("build", PipelineType::Build).with_command("npm run build"),
        );

        generator.add_pipeline(
            Pipeline::new("test", PipelineType::Test)
                .with_command("npm test")
                .depends_on("build"),
        );

        let pipelines: Vec<_> = generator.pipelines.values().collect();
        let yaml = generator.generate_github_actions(&pipelines);

        assert!(yaml.contains("needs:"));
    }

    #[test]
    fn test_package_multiple_tags() {
        let mut pkg = Package::new("multi-tag", "packages/multi");
        pkg.add_tag("frontend");
        pkg.add_tag("ui");
        pkg.add_tag("react");

        assert_eq!(pkg.tags.len(), 3);
        assert!(pkg.tags.contains("frontend"));
        assert!(pkg.tags.contains("ui"));
        assert!(pkg.tags.contains("react"));
    }

    #[test]
    fn test_package_owners() {
        let mut pkg = Package::new("owned", "packages/owned");
        pkg.owners.push("@team-frontend".to_string());
        pkg.owners.push("@john".to_string());

        assert_eq!(pkg.owners.len(), 2);
    }

    #[test]
    fn test_package_targets() {
        let mut pkg = Package::new("with-targets", "packages/targets");
        pkg.targets.push("build".to_string());
        pkg.targets.push("test".to_string());
        pkg.targets.push("lint".to_string());

        assert_eq!(pkg.targets.len(), 3);
    }

    #[test]
    fn test_graph_visualizer_with_different_types() {
        let mut graph = DependencyGraph::new(BuildSystem::Nx);

        graph.add_package(Package::new("lib", "packages/lib").with_type(PackageType::Library));
        graph.add_package(Package::new("app", "apps/app").with_type(PackageType::Application));
        graph.add_package(Package::new("svc", "services/svc").with_type(PackageType::Service));
        graph.add_package(Package::new("tool", "tools/tool").with_type(PackageType::Tool));

        let visualizer = GraphVisualizer::new(graph);
        let dot = visualizer.to_dot();

        // Each package type should have different colors
        assert!(dot.contains("lightyellow")); // Library
        assert!(dot.contains("lightblue")); // Application
        assert!(dot.contains("lightgreen")); // Service
        assert!(dot.contains("lightgray")); // Tool
    }

    #[test]
    fn test_file_change_debug() {
        let change = FileChange::new("test.rs", ChangeType::Modified);
        let debug_str = format!("{:?}", change);
        assert!(debug_str.contains("FileChange"));
    }

    #[test]
    fn test_affected_analysis_debug() {
        let analysis = AffectedAnalysis::new("main", "feature");
        let debug_str = format!("{:?}", analysis);
        assert!(debug_str.contains("AffectedAnalysis"));
    }

    #[test]
    fn test_pipeline_debug() {
        let pipeline = Pipeline::new("test", PipelineType::Test);
        let debug_str = format!("{:?}", pipeline);
        assert!(debug_str.contains("Pipeline"));
    }
}
