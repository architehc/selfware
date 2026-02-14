//! CI/CD Integration
//!
//! DevOps capabilities:
//! - GitHub Actions, GitLab CI, Jenkins support
//! - Pipeline generation
//! - Failure analysis
//! - Deployment triggers

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// CI/CD platform type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CiPlatform {
    GitHubActions,
    GitLabCi,
    Jenkins,
    CircleCi,
    TravisCi,
    AzureDevOps,
}

impl CiPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitHubActions => "github_actions",
            Self::GitLabCi => "gitlab_ci",
            Self::Jenkins => "jenkins",
            Self::CircleCi => "circleci",
            Self::TravisCi => "travis_ci",
            Self::AzureDevOps => "azure_devops",
        }
    }

    pub fn config_filename(&self) -> &'static str {
        match self {
            Self::GitHubActions => ".github/workflows/ci.yml",
            Self::GitLabCi => ".gitlab-ci.yml",
            Self::Jenkins => "Jenkinsfile",
            Self::CircleCi => ".circleci/config.yml",
            Self::TravisCi => ".travis.yml",
            Self::AzureDevOps => "azure-pipelines.yml",
        }
    }
}

/// Build status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildStatus {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
    Skipped,
}

impl BuildStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Success | Self::Failed | Self::Cancelled | Self::Skipped
        )
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled)
    }
}

/// Pipeline step/job
#[derive(Debug, Clone)]
pub struct PipelineStep {
    /// Step name
    pub name: String,
    /// Commands to run
    pub commands: Vec<String>,
    /// Step image/environment
    pub image: Option<String>,
    /// Dependencies (other step names)
    pub depends_on: Vec<String>,
    /// Condition for running
    pub condition: Option<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Timeout in minutes
    pub timeout_minutes: Option<u32>,
    /// Continue on error?
    pub continue_on_error: bool,
}

impl PipelineStep {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            commands: Vec::new(),
            image: None,
            depends_on: Vec::new(),
            condition: None,
            env: HashMap::new(),
            timeout_minutes: None,
            continue_on_error: false,
        }
    }

    pub fn with_command(mut self, cmd: &str) -> Self {
        self.commands.push(cmd.to_string());
        self
    }

    pub fn with_commands(mut self, cmds: Vec<&str>) -> Self {
        self.commands
            .extend(cmds.into_iter().map(|s| s.to_string()));
        self
    }

    pub fn with_image(mut self, image: &str) -> Self {
        self.image = Some(image.to_string());
        self
    }

    pub fn depends_on(mut self, step: &str) -> Self {
        self.depends_on.push(step.to_string());
        self
    }

    pub fn with_condition(mut self, condition: &str) -> Self {
        self.condition = Some(condition.to_string());
        self
    }

    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_timeout(mut self, minutes: u32) -> Self {
        self.timeout_minutes = Some(minutes);
        self
    }
}

/// Pipeline stage (group of parallel jobs)
#[derive(Debug, Clone)]
pub struct PipelineStage {
    /// Stage name
    pub name: String,
    /// Steps in this stage
    pub steps: Vec<PipelineStep>,
}

impl PipelineStage {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(mut self, step: PipelineStep) -> Self {
        self.steps.push(step);
        self
    }
}

/// Pipeline definition
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// Pipeline name
    pub name: String,
    /// Target platform
    pub platform: CiPlatform,
    /// Stages
    pub stages: Vec<PipelineStage>,
    /// Trigger events
    pub triggers: Vec<PipelineTrigger>,
    /// Global environment variables
    pub env: HashMap<String, String>,
    /// Matrix builds
    pub matrix: Option<BuildMatrix>,
}

impl Pipeline {
    pub fn new(name: &str, platform: CiPlatform) -> Self {
        Self {
            name: name.to_string(),
            platform,
            stages: Vec::new(),
            triggers: Vec::new(),
            env: HashMap::new(),
            matrix: None,
        }
    }

    pub fn add_stage(mut self, stage: PipelineStage) -> Self {
        self.stages.push(stage);
        self
    }

    pub fn add_trigger(mut self, trigger: PipelineTrigger) -> Self {
        self.triggers.push(trigger);
        self
    }

    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_matrix(mut self, matrix: BuildMatrix) -> Self {
        self.matrix = Some(matrix);
        self
    }
}

/// Pipeline trigger
#[derive(Debug, Clone)]
pub enum PipelineTrigger {
    /// Push to branches
    Push { branches: Vec<String> },
    /// Pull request
    PullRequest { branches: Vec<String> },
    /// Schedule (cron)
    Schedule { cron: String },
    /// Manual trigger
    Manual,
    /// Tag push
    Tag { pattern: String },
    /// Workflow dispatch (GitHub)
    WorkflowDispatch,
}

impl PipelineTrigger {
    pub fn push_main() -> Self {
        Self::Push {
            branches: vec!["main".to_string()],
        }
    }

    pub fn pull_request_all() -> Self {
        Self::PullRequest {
            branches: vec!["*".to_string()],
        }
    }

    pub fn nightly() -> Self {
        Self::Schedule {
            cron: "0 0 * * *".to_string(),
        }
    }
}

/// Build matrix for multiple configurations
#[derive(Debug, Clone)]
pub struct BuildMatrix {
    /// Matrix dimensions
    pub dimensions: HashMap<String, Vec<String>>,
    /// Exclude combinations
    pub exclude: Vec<HashMap<String, String>>,
    /// Include extra combinations
    pub include: Vec<HashMap<String, String>>,
}

impl BuildMatrix {
    pub fn new() -> Self {
        Self {
            dimensions: HashMap::new(),
            exclude: Vec::new(),
            include: Vec::new(),
        }
    }

    pub fn add_dimension(mut self, name: &str, values: Vec<&str>) -> Self {
        self.dimensions.insert(
            name.to_string(),
            values.into_iter().map(|s| s.to_string()).collect(),
        );
        self
    }

    /// Calculate total combinations
    pub fn total_combinations(&self) -> usize {
        if self.dimensions.is_empty() {
            return 1;
        }
        self.dimensions.values().map(|v| v.len()).product::<usize>() - self.exclude.len()
            + self.include.len()
    }
}

impl Default for BuildMatrix {
    fn default() -> Self {
        Self::new()
    }
}

/// Pipeline generator
pub struct PipelineGenerator {
    /// Default platform
    default_platform: CiPlatform,
}

impl PipelineGenerator {
    pub fn new(platform: CiPlatform) -> Self {
        Self {
            default_platform: platform,
        }
    }

    /// Generate a basic Rust pipeline
    pub fn rust_pipeline(&self, name: &str) -> Pipeline {
        let test_step = PipelineStep::new("test").with_commands(vec![
            "cargo fmt --check",
            "cargo clippy -- -D warnings",
            "cargo test",
        ]);

        let build_step = PipelineStep::new("build")
            .with_command("cargo build --release")
            .depends_on("test");

        Pipeline::new(name, self.default_platform)
            .add_trigger(PipelineTrigger::push_main())
            .add_trigger(PipelineTrigger::pull_request_all())
            .add_stage(PipelineStage::new("test").add_step(test_step))
            .add_stage(PipelineStage::new("build").add_step(build_step))
    }

    /// Generate a Node.js pipeline
    pub fn nodejs_pipeline(&self, name: &str) -> Pipeline {
        let test_step =
            PipelineStep::new("test").with_commands(vec!["npm ci", "npm run lint", "npm test"]);

        let build_step = PipelineStep::new("build")
            .with_command("npm run build")
            .depends_on("test");

        Pipeline::new(name, self.default_platform)
            .add_trigger(PipelineTrigger::push_main())
            .add_trigger(PipelineTrigger::pull_request_all())
            .add_stage(PipelineStage::new("test").add_step(test_step))
            .add_stage(PipelineStage::new("build").add_step(build_step))
    }

    /// Generate a Python pipeline
    pub fn python_pipeline(&self, name: &str) -> Pipeline {
        let test_step = PipelineStep::new("test").with_commands(vec![
            "pip install -r requirements.txt",
            "pip install pytest flake8",
            "flake8 .",
            "pytest",
        ]);

        Pipeline::new(name, self.default_platform)
            .add_trigger(PipelineTrigger::push_main())
            .add_trigger(PipelineTrigger::pull_request_all())
            .add_stage(PipelineStage::new("test").add_step(test_step))
    }

    /// Generate YAML for GitHub Actions
    pub fn to_github_actions(&self, pipeline: &Pipeline) -> String {
        let mut yaml = String::new();
        yaml.push_str(&format!("name: {}\n\n", pipeline.name));

        // Triggers
        yaml.push_str("on:\n");
        for trigger in &pipeline.triggers {
            match trigger {
                PipelineTrigger::Push { branches } => {
                    yaml.push_str("  push:\n");
                    yaml.push_str("    branches:\n");
                    for branch in branches {
                        yaml.push_str(&format!("      - {}\n", branch));
                    }
                }
                PipelineTrigger::PullRequest { branches } => {
                    yaml.push_str("  pull_request:\n");
                    yaml.push_str("    branches:\n");
                    for branch in branches {
                        yaml.push_str(&format!("      - {}\n", branch));
                    }
                }
                PipelineTrigger::Schedule { cron } => {
                    yaml.push_str("  schedule:\n");
                    yaml.push_str(&format!("    - cron: '{}'\n", cron));
                }
                PipelineTrigger::WorkflowDispatch => {
                    yaml.push_str("  workflow_dispatch:\n");
                }
                _ => {}
            }
        }

        // Environment variables
        if !pipeline.env.is_empty() {
            yaml.push_str("\nenv:\n");
            for (key, value) in &pipeline.env {
                yaml.push_str(&format!("  {}: {}\n", key, value));
            }
        }

        // Jobs
        yaml.push_str("\njobs:\n");
        for stage in &pipeline.stages {
            for step in &stage.steps {
                yaml.push_str(&format!("  {}:\n", step.name.replace(' ', "-")));
                yaml.push_str("    runs-on: ubuntu-latest\n");

                if !step.depends_on.is_empty() {
                    yaml.push_str("    needs:\n");
                    for dep in &step.depends_on {
                        yaml.push_str(&format!("      - {}\n", dep));
                    }
                }

                if let Some(timeout) = step.timeout_minutes {
                    yaml.push_str(&format!("    timeout-minutes: {}\n", timeout));
                }

                yaml.push_str("    steps:\n");
                yaml.push_str("      - uses: actions/checkout@v4\n");

                for cmd in &step.commands {
                    yaml.push_str(&format!("      - run: {}\n", cmd));
                }
            }
        }

        yaml
    }

    /// Generate YAML for GitLab CI
    pub fn to_gitlab_ci(&self, pipeline: &Pipeline) -> String {
        let mut yaml = String::new();

        // Stages
        yaml.push_str("stages:\n");
        for stage in &pipeline.stages {
            yaml.push_str(&format!("  - {}\n", stage.name));
        }
        yaml.push('\n');

        // Jobs
        for stage in &pipeline.stages {
            for step in &stage.steps {
                yaml.push_str(&format!("{}:\n", step.name.replace(' ', "_")));
                yaml.push_str(&format!("  stage: {}\n", stage.name));

                if let Some(image) = &step.image {
                    yaml.push_str(&format!("  image: {}\n", image));
                }

                if !step.commands.is_empty() {
                    yaml.push_str("  script:\n");
                    for cmd in &step.commands {
                        yaml.push_str(&format!("    - {}\n", cmd));
                    }
                }

                if !step.depends_on.is_empty() {
                    yaml.push_str("  needs:\n");
                    for dep in &step.depends_on {
                        yaml.push_str(&format!("    - {}\n", dep));
                    }
                }

                yaml.push('\n');
            }
        }

        yaml
    }

    /// Generate Jenkinsfile
    pub fn to_jenkinsfile(&self, pipeline: &Pipeline) -> String {
        let mut groovy = String::new();
        groovy.push_str("pipeline {\n");
        groovy.push_str("    agent any\n\n");

        // Environment
        if !pipeline.env.is_empty() {
            groovy.push_str("    environment {\n");
            for (key, value) in &pipeline.env {
                groovy.push_str(&format!("        {} = '{}'\n", key, value));
            }
            groovy.push_str("    }\n\n");
        }

        // Stages
        groovy.push_str("    stages {\n");
        for stage in &pipeline.stages {
            for step in &stage.steps {
                groovy.push_str(&format!("        stage('{}') {{\n", step.name));
                groovy.push_str("            steps {\n");
                for cmd in &step.commands {
                    groovy.push_str(&format!("                sh '{}'\n", cmd));
                }
                groovy.push_str("            }\n");
                groovy.push_str("        }\n");
            }
        }
        groovy.push_str("    }\n");

        groovy.push_str("}\n");
        groovy
    }

    /// Generate config for the pipeline's platform
    pub fn generate(&self, pipeline: &Pipeline) -> String {
        match pipeline.platform {
            CiPlatform::GitHubActions => self.to_github_actions(pipeline),
            CiPlatform::GitLabCi => self.to_gitlab_ci(pipeline),
            CiPlatform::Jenkins => self.to_jenkinsfile(pipeline),
            _ => self.to_github_actions(pipeline), // Default to GitHub Actions
        }
    }
}

impl Default for PipelineGenerator {
    fn default() -> Self {
        Self::new(CiPlatform::GitHubActions)
    }
}

/// Build failure record
#[derive(Debug, Clone)]
pub struct BuildFailure {
    /// Build ID
    pub build_id: String,
    /// Failed step
    pub step: String,
    /// Error message
    pub error_message: String,
    /// Error type
    pub error_type: FailureType,
    /// Log excerpt
    pub log_excerpt: Option<String>,
    /// Suggested fix
    pub suggested_fix: Option<String>,
    /// Timestamp
    pub timestamp: u64,
}

/// Type of build failure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureType {
    /// Compilation error
    Compilation,
    /// Test failure
    Test,
    /// Linting error
    Lint,
    /// Dependency issue
    Dependency,
    /// Configuration error
    Configuration,
    /// Timeout
    Timeout,
    /// Infrastructure issue
    Infrastructure,
    /// Unknown
    Unknown,
}

impl FailureType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Compilation => "compilation",
            Self::Test => "test",
            Self::Lint => "lint",
            Self::Dependency => "dependency",
            Self::Configuration => "configuration",
            Self::Timeout => "timeout",
            Self::Infrastructure => "infrastructure",
            Self::Unknown => "unknown",
        }
    }
}

/// Failure analyzer
pub struct FailureAnalyzer {
    /// Known patterns
    patterns: Vec<FailurePattern>,
    /// Analysis history
    history: Vec<BuildFailure>,
}

/// Pattern for detecting failure types
#[derive(Debug, Clone)]
pub struct FailurePattern {
    /// Pattern name
    pub name: String,
    /// Error type
    pub error_type: FailureType,
    /// Regex pattern
    pub pattern: String,
    /// Suggested fix
    pub suggested_fix: String,
}

impl FailureAnalyzer {
    pub fn new() -> Self {
        Self {
            patterns: Self::default_patterns(),
            history: Vec::new(),
        }
    }

    fn default_patterns() -> Vec<FailurePattern> {
        vec![
            FailurePattern {
                name: "Rust compilation error".to_string(),
                error_type: FailureType::Compilation,
                pattern: r"error\[E\d+\]:".to_string(),
                suggested_fix: "Check the error code and fix the compilation issue".to_string(),
            },
            FailurePattern {
                name: "Rust test failure".to_string(),
                error_type: FailureType::Test,
                pattern: r"test .+ \.\.\. FAILED".to_string(),
                suggested_fix: "Review the failing test and fix the assertion".to_string(),
            },
            FailurePattern {
                name: "Clippy warning".to_string(),
                error_type: FailureType::Lint,
                pattern: r"warning: .+ clippy::".to_string(),
                suggested_fix: "Run 'cargo clippy --fix' to auto-fix or address manually"
                    .to_string(),
            },
            FailurePattern {
                name: "npm install failure".to_string(),
                error_type: FailureType::Dependency,
                pattern: r"npm ERR!|ERESOLVE".to_string(),
                suggested_fix: "Check package.json for conflicts, try 'npm cache clean --force'"
                    .to_string(),
            },
            FailurePattern {
                name: "pip install failure".to_string(),
                error_type: FailureType::Dependency,
                pattern: r"pip: error:|Could not find a version".to_string(),
                suggested_fix: "Check requirements.txt for version conflicts".to_string(),
            },
            FailurePattern {
                name: "Docker build failure".to_string(),
                error_type: FailureType::Configuration,
                pattern: r"failed to build|docker: error".to_string(),
                suggested_fix: "Check Dockerfile syntax and base image availability".to_string(),
            },
            FailurePattern {
                name: "Timeout".to_string(),
                error_type: FailureType::Timeout,
                pattern: r"timed out|timeout exceeded".to_string(),
                suggested_fix: "Increase timeout or optimize the slow step".to_string(),
            },
        ]
    }

    /// Analyze build log for failures
    pub fn analyze(&mut self, build_id: &str, step: &str, log: &str) -> Option<BuildFailure> {
        for pattern in &self.patterns {
            if let Ok(re) = regex::Regex::new(&pattern.pattern) {
                if re.is_match(log) {
                    // Extract relevant log excerpt
                    let excerpt = Self::extract_excerpt(log, &re);

                    let failure = BuildFailure {
                        build_id: build_id.to_string(),
                        step: step.to_string(),
                        error_message: pattern.name.clone(),
                        error_type: pattern.error_type,
                        log_excerpt: excerpt,
                        suggested_fix: Some(pattern.suggested_fix.clone()),
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    };

                    self.history.push(failure.clone());
                    return Some(failure);
                }
            }
        }

        // Unknown failure
        let failure = BuildFailure {
            build_id: build_id.to_string(),
            step: step.to_string(),
            error_message: "Unknown failure".to_string(),
            error_type: FailureType::Unknown,
            log_excerpt: Some(log.lines().rev().take(10).collect::<Vec<_>>().join("\n")),
            suggested_fix: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        self.history.push(failure.clone());
        Some(failure)
    }

    fn extract_excerpt(log: &str, pattern: &regex::Regex) -> Option<String> {
        if let Some(mat) = pattern.find(log) {
            let start = mat.start().saturating_sub(200);
            let end = (mat.end() + 200).min(log.len());
            Some(log[start..end].to_string())
        } else {
            None
        }
    }

    /// Get common failures
    pub fn common_failures(&self) -> HashMap<FailureType, usize> {
        let mut counts = HashMap::new();
        for failure in &self.history {
            *counts.entry(failure.error_type).or_insert(0) += 1;
        }
        counts
    }

    /// Get history
    pub fn history(&self) -> &[BuildFailure] {
        &self.history
    }
}

impl Default for FailureAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Deployment environment
#[derive(Debug, Clone)]
pub struct DeploymentEnv {
    /// Environment name
    pub name: String,
    /// Environment type
    pub env_type: EnvType,
    /// URL
    pub url: Option<String>,
    /// Required approvers
    pub approvers: Vec<String>,
    /// Auto-deploy enabled
    pub auto_deploy: bool,
    /// From branch
    pub from_branch: Option<String>,
}

/// Environment type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvType {
    Development,
    Staging,
    Production,
}

impl DeploymentEnv {
    pub fn development(name: &str) -> Self {
        Self {
            name: name.to_string(),
            env_type: EnvType::Development,
            url: None,
            approvers: Vec::new(),
            auto_deploy: true,
            from_branch: Some("main".to_string()),
        }
    }

    pub fn staging(name: &str) -> Self {
        Self {
            name: name.to_string(),
            env_type: EnvType::Staging,
            url: None,
            approvers: Vec::new(),
            auto_deploy: false,
            from_branch: Some("main".to_string()),
        }
    }

    pub fn production(name: &str) -> Self {
        Self {
            name: name.to_string(),
            env_type: EnvType::Production,
            url: None,
            approvers: Vec::new(),
            auto_deploy: false,
            from_branch: Some("main".to_string()),
        }
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self
    }

    pub fn with_approver(mut self, approver: &str) -> Self {
        self.approvers.push(approver.to_string());
        self
    }
}

/// CI/CD integration manager
pub struct CiCdManager {
    /// Pipeline generator
    generator: PipelineGenerator,
    /// Failure analyzer
    failure_analyzer: RwLock<FailureAnalyzer>,
    /// Pipelines
    pipelines: RwLock<HashMap<String, Pipeline>>,
    /// Environments
    environments: RwLock<HashMap<String, DeploymentEnv>>,
    /// Build history
    build_history: RwLock<Vec<BuildRecord>>,
}

/// Record of a build
#[derive(Debug, Clone)]
pub struct BuildRecord {
    /// Build ID
    pub id: String,
    /// Pipeline name
    pub pipeline: String,
    /// Status
    pub status: BuildStatus,
    /// Started at
    pub started_at: u64,
    /// Finished at
    pub finished_at: Option<u64>,
    /// Commit SHA
    pub commit: Option<String>,
    /// Branch
    pub branch: Option<String>,
}

impl BuildRecord {
    pub fn new(id: &str, pipeline: &str) -> Self {
        Self {
            id: id.to_string(),
            pipeline: pipeline.to_string(),
            status: BuildStatus::Pending,
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            finished_at: None,
            commit: None,
            branch: None,
        }
    }

    pub fn with_commit(mut self, commit: &str) -> Self {
        self.commit = Some(commit.to_string());
        self
    }

    pub fn with_branch(mut self, branch: &str) -> Self {
        self.branch = Some(branch.to_string());
        self
    }

    pub fn duration_secs(&self) -> Option<u64> {
        self.finished_at.map(|f| f.saturating_sub(self.started_at))
    }
}

impl CiCdManager {
    pub fn new(platform: CiPlatform) -> Self {
        Self {
            generator: PipelineGenerator::new(platform),
            failure_analyzer: RwLock::new(FailureAnalyzer::new()),
            pipelines: RwLock::new(HashMap::new()),
            environments: RwLock::new(HashMap::new()),
            build_history: RwLock::new(Vec::new()),
        }
    }

    /// Add a pipeline
    pub fn add_pipeline(&self, pipeline: Pipeline) {
        if let Ok(mut pipelines) = self.pipelines.write() {
            pipelines.insert(pipeline.name.clone(), pipeline);
        }
    }

    /// Generate pipeline config
    pub fn generate_config(&self, name: &str) -> Option<String> {
        if let Ok(pipelines) = self.pipelines.read() {
            pipelines.get(name).map(|p| self.generator.generate(p))
        } else {
            None
        }
    }

    /// Add environment
    pub fn add_environment(&self, env: DeploymentEnv) {
        if let Ok(mut envs) = self.environments.write() {
            envs.insert(env.name.clone(), env);
        }
    }

    /// Record build
    pub fn record_build(&self, record: BuildRecord) {
        if let Ok(mut history) = self.build_history.write() {
            history.push(record);
            if history.len() > 1000 {
                history.drain(0..500);
            }
        }
    }

    /// Analyze failure
    pub fn analyze_failure(&self, build_id: &str, step: &str, log: &str) -> Option<BuildFailure> {
        if let Ok(mut analyzer) = self.failure_analyzer.write() {
            analyzer.analyze(build_id, step, log)
        } else {
            None
        }
    }

    /// Get stats
    pub fn get_stats(&self) -> CiCdStats {
        let history = self
            .build_history
            .read()
            .map(|h| h.clone())
            .unwrap_or_default();
        let total = history.len();
        let successful = history
            .iter()
            .filter(|b| b.status == BuildStatus::Success)
            .count();
        let failed = history
            .iter()
            .filter(|b| b.status == BuildStatus::Failed)
            .count();

        let avg_duration: f64 = {
            let durations: Vec<u64> = history.iter().filter_map(|b| b.duration_secs()).collect();
            if durations.is_empty() {
                0.0
            } else {
                durations.iter().sum::<u64>() as f64 / durations.len() as f64
            }
        };

        CiCdStats {
            total_builds: total,
            successful_builds: successful,
            failed_builds: failed,
            avg_duration_secs: avg_duration,
            pipelines_count: self.pipelines.read().map(|p| p.len()).unwrap_or(0),
            environments_count: self.environments.read().map(|e| e.len()).unwrap_or(0),
        }
    }
}

impl Default for CiCdManager {
    fn default() -> Self {
        Self::new(CiPlatform::GitHubActions)
    }
}

/// CI/CD statistics
#[derive(Debug, Clone)]
pub struct CiCdStats {
    pub total_builds: usize,
    pub successful_builds: usize,
    pub failed_builds: usize,
    pub avg_duration_secs: f64,
    pub pipelines_count: usize,
    pub environments_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ci_platform_as_str() {
        assert_eq!(CiPlatform::GitHubActions.as_str(), "github_actions");
        assert_eq!(CiPlatform::GitLabCi.as_str(), "gitlab_ci");
    }

    #[test]
    fn test_ci_platform_config_filename() {
        assert_eq!(
            CiPlatform::GitHubActions.config_filename(),
            ".github/workflows/ci.yml"
        );
        assert_eq!(CiPlatform::Jenkins.config_filename(), "Jenkinsfile");
    }

    #[test]
    fn test_build_status() {
        assert!(BuildStatus::Success.is_terminal());
        assert!(BuildStatus::Failed.is_failure());
        assert!(!BuildStatus::Running.is_terminal());
    }

    #[test]
    fn test_pipeline_step_new() {
        let step = PipelineStep::new("test").with_command("cargo test");
        assert_eq!(step.name, "test");
        assert_eq!(step.commands.len(), 1);
    }

    #[test]
    fn test_pipeline_step_builder() {
        let step = PipelineStep::new("build")
            .with_image("rust:latest")
            .with_env("RUST_BACKTRACE", "1")
            .with_timeout(30)
            .depends_on("test");

        assert_eq!(step.image, Some("rust:latest".to_string()));
        assert!(step.env.contains_key("RUST_BACKTRACE"));
    }

    #[test]
    fn test_pipeline_stage() {
        let stage = PipelineStage::new("test")
            .add_step(PipelineStep::new("unit"))
            .add_step(PipelineStep::new("integration"));
        assert_eq!(stage.steps.len(), 2);
    }

    #[test]
    fn test_pipeline_new() {
        let pipeline = Pipeline::new("CI", CiPlatform::GitHubActions)
            .add_trigger(PipelineTrigger::push_main());
        assert_eq!(pipeline.name, "CI");
        assert_eq!(pipeline.triggers.len(), 1);
    }

    #[test]
    fn test_build_matrix() {
        let matrix = BuildMatrix::new()
            .add_dimension("os", vec!["ubuntu", "macos", "windows"])
            .add_dimension("rust", vec!["stable", "nightly"]);
        assert_eq!(matrix.total_combinations(), 6);
    }

    #[test]
    fn test_pipeline_generator_rust() {
        let gen = PipelineGenerator::new(CiPlatform::GitHubActions);
        let pipeline = gen.rust_pipeline("Rust CI");
        assert!(!pipeline.stages.is_empty());
    }

    #[test]
    fn test_pipeline_generator_to_github_actions() {
        let gen = PipelineGenerator::new(CiPlatform::GitHubActions);
        let pipeline = gen.rust_pipeline("CI");
        let yaml = gen.to_github_actions(&pipeline);
        assert!(yaml.contains("name: CI"));
        assert!(yaml.contains("runs-on:"));
    }

    #[test]
    fn test_pipeline_generator_to_gitlab_ci() {
        let gen = PipelineGenerator::new(CiPlatform::GitLabCi);
        let pipeline = gen.rust_pipeline("CI");
        let yaml = gen.to_gitlab_ci(&pipeline);
        assert!(yaml.contains("stages:"));
    }

    #[test]
    fn test_pipeline_generator_to_jenkinsfile() {
        let gen = PipelineGenerator::new(CiPlatform::Jenkins);
        let pipeline = gen.rust_pipeline("CI");
        let groovy = gen.to_jenkinsfile(&pipeline);
        assert!(groovy.contains("pipeline {"));
        assert!(groovy.contains("stages {"));
    }

    #[test]
    fn test_failure_type() {
        assert_eq!(FailureType::Compilation.as_str(), "compilation");
        assert_eq!(FailureType::Test.as_str(), "test");
    }

    #[test]
    fn test_failure_analyzer_rust_error() {
        let mut analyzer = FailureAnalyzer::new();
        let log = "error[E0425]: cannot find value `x` in this scope";
        let failure = analyzer.analyze("build-1", "compile", log);
        assert!(failure.is_some());
        assert_eq!(failure.unwrap().error_type, FailureType::Compilation);
    }

    #[test]
    fn test_failure_analyzer_test_failure() {
        let mut analyzer = FailureAnalyzer::new();
        let log = "test tests::my_test ... FAILED";
        let failure = analyzer.analyze("build-1", "test", log);
        assert!(failure.is_some());
        assert_eq!(failure.unwrap().error_type, FailureType::Test);
    }

    #[test]
    fn test_failure_analyzer_common_failures() {
        let mut analyzer = FailureAnalyzer::new();
        analyzer.analyze("b1", "test", "error[E0425]: x");
        analyzer.analyze("b2", "test", "error[E0404]: y");
        let common = analyzer.common_failures();
        assert_eq!(common.get(&FailureType::Compilation), Some(&2));
    }

    #[test]
    fn test_deployment_env() {
        let env = DeploymentEnv::production("prod")
            .with_url("https://app.example.com")
            .with_approver("admin");
        assert_eq!(env.env_type, EnvType::Production);
        assert!(!env.auto_deploy);
    }

    #[test]
    fn test_build_record_new() {
        let record = BuildRecord::new("build-123", "CI")
            .with_commit("abc123")
            .with_branch("main");
        assert_eq!(record.status, BuildStatus::Pending);
        assert!(record.commit.is_some());
    }

    #[test]
    fn test_cicd_manager_new() {
        let manager = CiCdManager::new(CiPlatform::GitHubActions);
        let stats = manager.get_stats();
        assert_eq!(stats.total_builds, 0);
    }

    #[test]
    fn test_cicd_manager_add_pipeline() {
        let manager = CiCdManager::new(CiPlatform::GitHubActions);
        let pipeline = Pipeline::new("CI", CiPlatform::GitHubActions);
        manager.add_pipeline(pipeline);

        let config = manager.generate_config("CI");
        assert!(config.is_some());
    }

    #[test]
    fn test_cicd_manager_analyze_failure() {
        let manager = CiCdManager::new(CiPlatform::GitHubActions);
        let failure = manager.analyze_failure("b1", "test", "error[E0001]: test");
        assert!(failure.is_some());
    }

    #[test]
    fn test_cicd_stats() {
        let stats = CiCdStats {
            total_builds: 100,
            successful_builds: 90,
            failed_builds: 10,
            avg_duration_secs: 120.0,
            pipelines_count: 3,
            environments_count: 2,
        };
        assert_eq!(
            stats.total_builds,
            stats.successful_builds + stats.failed_builds
        );
    }

    #[test]
    fn test_pipeline_trigger_variants() {
        let push = PipelineTrigger::push_main();
        let pr = PipelineTrigger::pull_request_all();
        let nightly = PipelineTrigger::nightly();

        assert!(matches!(push, PipelineTrigger::Push { .. }));
        assert!(matches!(pr, PipelineTrigger::PullRequest { .. }));
        assert!(matches!(nightly, PipelineTrigger::Schedule { .. }));
    }

    #[test]
    fn test_ci_platform_all_variants() {
        assert_eq!(CiPlatform::CircleCi.as_str(), "circleci");
        assert_eq!(CiPlatform::TravisCi.as_str(), "travis_ci");
        assert_eq!(CiPlatform::AzureDevOps.as_str(), "azure_devops");
    }

    #[test]
    fn test_ci_platform_config_filenames() {
        assert_eq!(
            CiPlatform::CircleCi.config_filename(),
            ".circleci/config.yml"
        );
        assert_eq!(CiPlatform::TravisCi.config_filename(), ".travis.yml");
        assert_eq!(
            CiPlatform::AzureDevOps.config_filename(),
            "azure-pipelines.yml"
        );
        assert_eq!(CiPlatform::GitLabCi.config_filename(), ".gitlab-ci.yml");
    }

    #[test]
    fn test_build_status_as_str() {
        assert_eq!(BuildStatus::Pending.as_str(), "pending");
        assert_eq!(BuildStatus::Running.as_str(), "running");
        assert_eq!(BuildStatus::Success.as_str(), "success");
        assert_eq!(BuildStatus::Failed.as_str(), "failed");
        assert_eq!(BuildStatus::Cancelled.as_str(), "cancelled");
        assert_eq!(BuildStatus::Skipped.as_str(), "skipped");
    }

    #[test]
    fn test_build_status_is_terminal() {
        assert!(!BuildStatus::Pending.is_terminal());
        assert!(!BuildStatus::Running.is_terminal());
        assert!(BuildStatus::Success.is_terminal());
        assert!(BuildStatus::Failed.is_terminal());
        assert!(BuildStatus::Cancelled.is_terminal());
        assert!(BuildStatus::Skipped.is_terminal());
    }

    #[test]
    fn test_build_status_is_failure() {
        assert!(!BuildStatus::Pending.is_failure());
        assert!(!BuildStatus::Success.is_failure());
        assert!(BuildStatus::Failed.is_failure());
        assert!(BuildStatus::Cancelled.is_failure());
    }

    #[test]
    fn test_pipeline_step_with_condition() {
        let step = PipelineStep::new("deploy").with_condition("github.ref == 'refs/heads/main'");

        assert!(step.condition.is_some());
    }

    #[test]
    fn test_build_matrix_dimensions() {
        let matrix = BuildMatrix::new()
            .add_dimension("os", vec!["ubuntu", "windows"])
            .add_dimension("rust", vec!["stable", "nightly"]);

        // Total would be 4 combinations
        assert_eq!(matrix.total_combinations(), 4);
    }

    #[test]
    fn test_failure_type_all_variants() {
        assert_eq!(FailureType::Compilation.as_str(), "compilation");
        assert_eq!(FailureType::Test.as_str(), "test");
        assert_eq!(FailureType::Lint.as_str(), "lint");
        assert_eq!(FailureType::Dependency.as_str(), "dependency");
        assert_eq!(FailureType::Configuration.as_str(), "configuration");
        assert_eq!(FailureType::Timeout.as_str(), "timeout");
        assert_eq!(FailureType::Infrastructure.as_str(), "infrastructure");
        assert_eq!(FailureType::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_deployment_env_staging() {
        let env = DeploymentEnv::staging("staging-1").with_url("https://staging.example.com");

        assert_eq!(env.env_type, EnvType::Staging);
    }

    #[test]
    fn test_deployment_env_development() {
        let env = DeploymentEnv::development("dev-1");
        assert_eq!(env.env_type, EnvType::Development);
        assert!(env.auto_deploy);
    }

    #[test]
    fn test_build_record_status() {
        let record = BuildRecord::new("build-456", "CI")
            .with_commit("def456")
            .with_branch("feature/test");

        assert_eq!(record.status, BuildStatus::Pending);
        assert!(record.commit.is_some());
        assert!(record.branch.is_some());
    }

    #[test]
    fn test_pipeline_with_stages() {
        let pipeline = Pipeline::new("Full CI", CiPlatform::GitHubActions)
            .add_trigger(PipelineTrigger::push_main())
            .add_stage(PipelineStage::new("build").add_step(PipelineStep::new("compile")))
            .add_stage(PipelineStage::new("test").add_step(PipelineStep::new("unit-tests")));

        assert_eq!(pipeline.stages.len(), 2);
    }

    #[test]
    fn test_pipeline_stage_steps() {
        let stage = PipelineStage::new("parallel-tests")
            .add_step(PipelineStep::new("test-1"))
            .add_step(PipelineStep::new("test-2"));

        assert_eq!(stage.steps.len(), 2);
    }

    #[test]
    fn test_env_type_debug() {
        let dev = EnvType::Development;
        let staging = EnvType::Staging;
        let prod = EnvType::Production;

        assert_eq!(format!("{:?}", dev), "Development");
        assert_eq!(format!("{:?}", staging), "Staging");
        assert_eq!(format!("{:?}", prod), "Production");
    }

    #[test]
    fn test_failure_analyzer_unknown() {
        let mut analyzer = FailureAnalyzer::new();
        let log = "some random log output";
        let failure = analyzer.analyze("build-1", "unknown", log);
        // May or may not find a match
        let _ = failure;
    }

    #[test]
    fn test_pipeline_step_multiple_commands() {
        let step = PipelineStep::new("setup")
            .with_command("npm install")
            .with_command("npm run build")
            .with_command("npm test");

        assert_eq!(step.commands.len(), 3);
    }

    #[test]
    fn test_pipeline_step_with_commands() {
        let step = PipelineStep::new("multi").with_commands(vec!["cmd1", "cmd2", "cmd3"]);

        assert_eq!(step.commands.len(), 3);
    }

    #[test]
    fn test_pipeline_step_depends_on_multiple() {
        let step = PipelineStep::new("deploy")
            .depends_on("build")
            .depends_on("test");

        assert_eq!(step.depends_on.len(), 2);
    }
}
