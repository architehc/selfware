# Selfware Recursive Self-Modification System Design

## Executive Summary

This document designs a comprehensive self-modification system for Selfware that enables true recursive self-improvement. The system allows the agent to:
1. Modify its own Rust source code safely
2. Compile and test modified code
3. Hot-reload changes without losing state
4. Rollback failed modifications
5. Improve its own improvement mechanisms

## Current State Analysis

### Existing Self-Modification Capabilities

From examining `self_edit.rs` and `self_improvement.rs`:

**self_edit.rs:**
- `ImprovementTarget`: Identifies improvement opportunities
- `SelfEditOrchestrator`: Manages the self-improvement workflow
- `ImprovementCategory`: Categories like PromptTemplate, ToolPipeline, ErrorHandling
- `DENY_LIST`: Safety mechanism protecting critical files
- Basic analysis for TODO/FIXME markers and code quality issues

**self_improvement.rs:**
- `SelfImprovementEngine`: Tracks prompt/tool effectiveness
- `PromptRecord`/`ToolRecord`: Records effectiveness data
- Learning from outcomes for optimization
- Error pattern detection and avoidance

### Current Limitations

1. **No True Source Code Self-Modification**: Only modifies prompts and tool selection, not actual Rust code
2. **No Self-Compilation**: Cannot compile modified code
3. **No Hot-Reloading**: Changes require manual restart
4. **Limited to Parameter Level**: Cannot modify architecture or add new capabilities
5. **No Type-Safe Modification**: No AST-based code transformation
6. **No Differential Testing**: Cannot compare behavior before/after changes

## Target Architecture

### 1. Self-Modification Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Selfware Recursive Self-Modification System           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                   │
│  │   Analysis   │───▶│  Planning    │───▶│  Generation  │                   │
│  │   Engine     │    │   Engine     │    │   Engine     │                   │
│  └──────────────┘    └──────────────┘    └──────────────┘                   │
│         │                   │                   │                           │
│         ▼                   ▼                   ▼                           │
│  ┌─────────────────────────────────────────────────────┐                    │
│  │              AST-Based Code Transformer              │                    │
│  │         (syn, quote, proc-macro2 crates)            │                    │
│  └─────────────────────────────────────────────────────┘                    │
│                            │                                                │
│                            ▼                                                │
│  ┌─────────────────────────────────────────────────────┐                    │
│  │              Compilation & Verification              │                    │
│  │         (cargo check, cargo test, clippy)           │                    │
│  └─────────────────────────────────────────────────────┘                    │
│                            │                                                │
│                            ▼                                                │
│  ┌─────────────────────────────────────────────────────┐                    │
│  │              Hot-Reload & State Migration            │                    │
│  │         (libloading, shared library updates)        │                    │
│  └─────────────────────────────────────────────────────┘                    │
│                            │                                                │
│                            ▼                                                │
│  ┌─────────────────────────────────────────────────────┐                    │
│  │              Rollback & Differential Testing         │                    │
│  │         (git-based versioning, behavior comparison)  │                    │
│  └─────────────────────────────────────────────────────┘                    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2. Core Components

#### 2.1 Code Analysis Engine (`cognitive/code_analysis.rs`)

```rust
//! Code Analysis Engine for Self-Modification
//! 
//! Analyzes the codebase to identify improvement opportunities
//! using AST-based analysis.

use syn::{File, Item, visit::Visit};
use quote::quote;
use proc_macro2::TokenStream;

/// Analysis result for a single file
#[derive(Debug, Clone)]
pub struct FileAnalysis {
    pub path: PathBuf,
    pub ast: File,
    pub complexity: CyclomaticComplexity,
    pub dependencies: Vec<String>,
    pub test_coverage: Option<f64>,
    pub issues: Vec<CodeIssue>,
}

/// Types of code issues that can be improved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CodeIssue {
    /// Function is too complex
    HighComplexity { function: String, score: u32 },
    /// Function is too long
    LongFunction { function: String, lines: usize },
    /// Missing documentation
    MissingDocs { item: String, item_type: ItemType },
    /// Clone-heavy code that could use references
    UnnecessaryCloning { location: String, suggestion: String },
    /// Could use better error handling
    ErrorHandlingOpportunity { location: String, pattern: ErrorPattern },
    /// Performance optimization opportunity
    PerformanceOpportunity { location: String, optimization: String },
    /// Could benefit from parallelization
    ParallelizationOpportunity { location: String, reason: String },
    /// Type safety improvement
    TypeSafetyIssue { location: String, issue: String },
}

/// AST visitor for code analysis
pub struct CodeAnalyzer {
    issues: Vec<CodeIssue>,
    current_function: Option<String>,
    complexity_stack: Vec<u32>,
}

impl<'ast> Visit<'ast> for CodeAnalyzer {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        self.current_function = Some(name.clone());
        
        // Calculate cyclomatic complexity
        let complexity = calculate_complexity(&node.block);
        if complexity > 10 {
            self.issues.push(CodeIssue::HighComplexity {
                function: name.clone(),
                score: complexity,
            });
        }
        
        // Check function length
        let lines = count_lines(&node.block);
        if lines > 50 {
            self.issues.push(CodeIssue::LongFunction {
                function: name.clone(),
                lines,
            });
        }
        
        // Check for missing docs
        if node.attrs.iter().none(|a| a.path().is_ident("doc")) {
            self.issues.push(CodeIssue::MissingDocs {
                item: name,
                item_type: ItemType::Function,
            });
        }
        
        syn::visit::visit_item_fn(self, node);
        self.current_function = None;
    }
    
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        // Detect .clone() calls that might be unnecessary
        if node.method == "clone" {
            self.issues.push(CodeIssue::UnnecessaryCloning {
                location: format!("{:?}", node),
                suggestion: "Consider using references instead".to_string(),
            });
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

/// Analyze a Rust source file
pub fn analyze_file(path: &Path) -> Result<FileAnalysis> {
    let content = fs::read_to_string(path)?;
    let ast = syn::parse_file(&content)?;
    
    let mut analyzer = CodeAnalyzer::new();
    analyzer.visit_file(&ast);
    
    Ok(FileAnalysis {
        path: path.to_path_buf(),
        ast,
        complexity: analyzer.total_complexity(),
        dependencies: extract_dependencies(&ast),
        test_coverage: None, // Would integrate with coverage tools
        issues: analyzer.issues,
    })
}
```

#### 2.2 AST-Based Code Transformer (`cognitive/code_transformer.rs`)

```rust
//! AST-Based Code Transformer
//! 
//! Provides type-safe code transformations using syn/quote.

use syn::{
    parse_quote, File, Item, ItemFn, ItemStruct, ItemEnum,
    visit_mut::{self, VisitMut},
    visit::{self, Visit},
};
use quote::{quote, ToTokens};

/// A code transformation that can be applied
#[derive(Debug, Clone)]
pub struct CodeTransformation {
    pub id: String,
    pub target_file: PathBuf,
    pub transformation_type: TransformationType,
    pub description: String,
    pub safety_level: SafetyLevel,
}

#[derive(Debug, Clone)]
pub enum TransformationType {
    /// Add a new function
    AddFunction { item: ItemFn },
    /// Modify an existing function
    ModifyFunction { name: String, new_body: syn::Block },
    /// Add a method to a struct/impl block
    AddMethod { impl_for: String, method: ItemFn },
    /// Refactor: extract function
    ExtractFunction { 
        source_function: String,
        extracted_code: syn::Block,
        new_function_name: String,
    },
    /// Add error handling
    AddErrorHandling { function: String, error_type: String },
    /// Optimize: replace pattern with better implementation
    PatternReplace { 
        pattern: CodePattern,
        replacement: TokenStream,
    },
    /// Add documentation
    AddDocumentation { item: String, docs: String },
    /// Add test
    AddTest { test_function: ItemFn },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyLevel {
    /// Safe: only adds new code, doesn't modify existing
    Additive,
    /// Medium: modifies existing code but preserves behavior
    BehaviorPreserving,
    /// Risky: may change behavior
    BehaviorChanging,
    /// Critical: modifies core infrastructure
    Critical,
}

/// Applies transformations to AST
pub struct AstTransformer;

impl AstTransformer {
    /// Parse file and apply transformation
    pub fn transform(
        source: &str,
        transformation: &CodeTransformation,
    ) -> Result<String> {
        let mut file = syn::parse_file(source)?;
        
        match &transformation.transformation_type {
            TransformationType::AddFunction { item } => {
                file.items.push(Item::Fn(item.clone()));
            }
            TransformationType::ModifyFunction { name, new_body } => {
                Self::modify_function(&mut file, name, new_body)?;
            }
            TransformationType::ExtractFunction { 
                source_function,
                extracted_code,
                new_function_name,
            } => {
                Self::extract_function(
                    &mut file,
                    source_function,
                    extracted_code,
                    new_function_name,
                )?;
            }
            _ => unimplemented!(),
        }
        
        // Format the output
        let formatted = prettyplease::unparse(&file);
        Ok(formatted)
    }
    
    fn modify_function(
        file: &mut File,
        name: &str,
        new_body: &syn::Block,
    ) -> Result<()> {
        struct FunctionModifier<'a> {
            target: &'a str,
            new_body: &'a syn::Block,
            modified: bool,
        }
        
        impl<'a> VisitMut for FunctionModifier<'a> {
            fn visit_item_fn_mut(&mut self, node: &mut ItemFn) {
                if node.sig.ident == self.target {
                    node.block = Box::new(self.new_body.clone());
                    self.modified = true;
                }
                visit_mut::visit_item_fn_mut(self, node);
            }
        }
        
        let mut modifier = FunctionModifier {
            target: name,
            new_body,
            modified: false,
        };
        
        for item in &mut file.items {
            modifier.visit_item_mut(item);
        }
        
        if !modifier.modified {
            return Err(anyhow!("Function '{}' not found", name));
        }
        
        Ok(())
    }
    
    /// Extract code into a new function
    fn extract_function(
        file: &mut File,
        source_function: &str,
        extracted_code: &syn::Block,
        new_function_name: &str,
    ) -> Result<()> {
        // 1. Create new function from extracted code
        let new_fn: ItemFn = parse_quote! {
            fn #new_function_name() -> Result<()> {
                #extracted_code
            }
        };
        
        // 2. Replace extracted code with call to new function
        let replacement: syn::Stmt = parse_quote! {
            #new_function_name()?;
        };
        
        // 3. Add new function to file
        file.items.push(Item::Fn(new_fn));
        
        // 4. Modify source function to use new function
        // (implementation details...)
        
        Ok(())
    }
}

/// Verify transformation preserves types
pub fn verify_type_safety(
    original: &str,
    transformed: &str,
) -> Result<TypeSafetyReport> {
    // Use rustc or rust-analyzer to check type compatibility
    let original_ast = syn::parse_file(original)?;
    let transformed_ast = syn::parse_file(transformed)?;
    
    // Compare public API signatures
    let original_api = extract_public_api(&original_ast);
    let transformed_api = extract_public_api(&transformed_ast);
    
    Ok(TypeSafetyReport {
        compatible: original_api == transformed_api,
        changes: diff_apis(&original_api, &transformed_api),
    })
}
```

#### 2.3 Compilation Manager (`cognitive/compilation_manager.rs`)

```rust
//! Compilation Manager
//! 
//! Manages compilation of modified code and verification.

use std::process::{Command, Output};
use std::path::Path;

/// Compilation configuration
#[derive(Debug, Clone)]
pub struct CompileConfig {
    pub target_dir: PathBuf,
    pub profile: CompileProfile,
    pub features: Vec<String>,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum CompileProfile {
    Debug,
    Release,
    Test,
}

/// Compilation result
#[derive(Debug, Clone)]
pub struct CompileResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub warnings: Vec<CompilerWarning>,
    pub errors: Vec<CompilerError>,
    pub artifacts: Vec<PathBuf>,
}

/// Manages compilation of self-modified code
pub struct CompilationManager {
    config: CompileConfig,
    original_dir: PathBuf,
    work_dir: PathBuf,
}

impl CompilationManager {
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let work_dir = project_root.join(".selfware").join("build");
        fs::create_dir_all(&work_dir)?;
        
        Ok(Self {
            config: CompileConfig::default(),
            original_dir: project_root,
            work_dir,
        })
    }
    
    /// Check if code compiles without errors
    pub fn check(&self) -> Result<CompileResult> {
        let output = Command::new("cargo")
            .arg("check")
            .arg("--message-format=short")
            .current_dir(&self.original_dir)
            .output()?;
        
        self.parse_output(output)
    }
    
    /// Run clippy for additional linting
    pub fn lint(&self) -> Result<CompileResult> {
        let output = Command::new("cargo")
            .arg("clippy")
            .arg("--")
            .arg("-D")
            .arg("warnings")
            .current_dir(&self.original_dir)
            .output()?;
        
        self.parse_output(output)
    }
    
    /// Build the project
    pub fn build(&self, profile: CompileProfile) -> Result<CompileResult> {
        let mut cmd = Command::new("cargo");
        cmd.arg("build");
        
        match profile {
            CompileProfile::Release => { cmd.arg("--release"); }
            CompileProfile::Test => { cmd.arg("--profile").arg("test"); }
            _ => {}
        }
        
        let output = cmd
            .current_dir(&self.original_dir)
            .output()?;
        
        self.parse_output(output)
    }
    
    /// Run tests
    pub fn test(&self, filter: Option<&str>) -> Result<TestResult> {
        let mut cmd = Command::new("cargo");
        cmd.arg("test");
        
        if let Some(f) = filter {
            cmd.arg(f);
        }
        
        cmd.arg("--");
        cmd.arg("--nocapture");
        
        let output = cmd
            .current_dir(&self.original_dir)
            .output()?;
        
        self.parse_test_output(output)
    }
    
    /// Full verification pipeline
    pub fn verify(&self) -> Result<VerificationReport> {
        let mut report = VerificationReport::default();
        
        // Step 1: Check compilation
        report.check_result = self.check()?;
        if !report.check_result.success {
            return Ok(report);
        }
        
        // Step 2: Run clippy
        report.lint_result = self.lint()?;
        
        // Step 3: Run tests
        report.test_result = self.test(None)?;
        
        // Step 4: Check formatting
        report.format_check = self.check_formatting()?;
        
        Ok(report)
    }
    
    fn parse_output(&self, output: Output) -> Result<CompileResult> {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        
        // Parse compiler messages
        let (errors, warnings) = self.parse_compiler_messages(&stderr);
        
        Ok(CompileResult {
            success: output.status.success(),
            stdout,
            stderr,
            warnings,
            errors,
            artifacts: vec![],
        })
    }
    
    fn parse_compiler_messages(&self, stderr: &str) -> (Vec<CompilerError>, Vec<CompilerWarning>) {
        // Parse rustc/cargo JSON output
        // Implementation using cargo_metadata or regex
        todo!()
    }
}
```

#### 2.4 Hot-Reload System (`cognitive/hot_reload.rs`)

```rust
//! Hot-Reload System
//! 
//! Enables dynamic loading of modified code without restart.

use libloading::{Library, Symbol};
use std::sync::Arc;
use parking_lot::RwLock;

/// Trait for hot-reloadable components
pub trait HotReloadable: Send + Sync {
    /// Called when component is loaded/reloaded
    fn on_load(&mut self) -> Result<()>;
    
    /// Called before component is unloaded
    fn on_unload(&mut self) -> Result<()>;
    
    /// Get component version
    fn version(&self) -> ComponentVersion;
}

/// Version info for hot-reloadable components
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub git_hash: Option<String>,
}

/// Handle to a hot-reloaded library
pub struct HotReloadHandle<T: HotReloadable> {
    library: Library,
    component: Arc<RwLock<T>>,
    version: ComponentVersion,
    library_path: PathBuf,
}

/// Manages hot-reloadable components
pub struct HotReloadManager {
    components: HashMap<String, Box<dyn Any + Send + Sync>>,
    state_migrator: StateMigrator,
    watch_handles: Vec<RecommendedWatcher>,
}

impl HotReloadManager {
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            state_migrator: StateMigrator::new(),
            watch_handles: vec![],
        }
    }
    
    /// Register a component for hot-reloading
    pub fn register<T: HotReloadable + 'static>(
        &mut self,
        name: &str,
        library_path: PathBuf,
    ) -> Result<Arc<RwLock<T>>> {
        let handle = self.load_component::<T>(&library_path)?;
        let component = handle.component.clone();
        
        self.components.insert(name.to_string(), Box::new(handle));
        
        // Set up file watcher
        self.watch_for_changes(name, library_path)?;
        
        Ok(component)
    }
    
    /// Load a component from a shared library
    fn load_component<T: HotReloadable>(&self, path: &Path) -> Result<HotReloadHandle<T>> {
        unsafe {
            let library = Library::new(path)?;
            
            // Get the component constructor
            let constructor: Symbol<unsafe fn() -> *mut T> = 
                library.get(b"_create_component")?;
            
            let component_ptr = constructor();
            let component = Arc::new(RwLock::new(*Box::from_raw(component_ptr)));
            
            // Initialize component
            component.write().on_load()?;
            
            let version = component.read().version();
            
            Ok(HotReloadHandle {
                library,
                component,
                version,
                library_path: path.to_path_buf(),
            })
        }
    }
    
    /// Hot-reload a component
    pub fn reload<T: HotReloadable + 'static>(
        &mut self,
        name: &str,
    ) -> Result<()> {
        let old_handle = self.components
            .get_mut(name)
            .ok_or_else(|| anyhow!("Component '{}' not found", name))?
            .downcast_mut::<HotReloadHandle<T>>()
            .ok_or_else(|| anyhow!("Type mismatch for component '{}'", name))?;
        
        // Extract state from old component
        let old_state = self.state_migrator.extract_state(&*old_handle.component.read());
        
        // Unload old component
        old_handle.component.write().on_unload()?;
        
        // Load new version
        let new_handle = self.load_component::<T>(&old_handle.library_path)?;
        
        // Migrate state to new component
        self.state_migrator.apply_state(
            &mut *new_handle.component.write(),
            old_state,
        )?;
        
        // Replace handle
        *old_handle = new_handle;
        
        info!("Hot-reloaded component: {}", name);
        Ok(())
    }
    
    /// Watch for file changes
    fn watch_for_changes(&mut self, name: &str, path: PathBuf) -> Result<()> {
        let (tx, rx) = channel();
        
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;
        
        watcher.watch(&path, RecursiveMode::NonRecursive)?;
        
        // Spawn watcher thread
        let name = name.to_string();
        thread::spawn(move || {
            for event in rx {
                if matches!(event.kind, EventKind::Modify(_)) {
                    info!("Detected change in {}, triggering reload", name);
                    // Trigger reload via message to main thread
                }
            }
        });
        
        self.watch_handles.push(watcher);
        Ok(())
    }
}

/// State migration between component versions
pub struct StateMigrator;

impl StateMigrator {
    pub fn new() -> Self {
        Self
    }
    
    /// Extract serializable state from a component
    pub fn extract_state<T: HotReloadable>(&self, component: &T) -> serde_json::Value {
        // Use reflection or derive macro to extract state
        // This would be customized per component type
        serde_json::json!({})
    }
    
    /// Apply state to a component
    pub fn apply_state<T: HotReloadable>(
        &self,
        component: &mut T,
        state: serde_json::Value,
    ) -> Result<()> {
        // Apply migrated state
        Ok(())
    }
}
```

#### 2.5 Safety & Rollback System (`cognitive/safety_rollback.rs`)

```rust
//! Safety & Rollback System
//! 
//! Ensures safe self-modification with comprehensive rollback capabilities.

use git2::{Repository, Signature, Oid};

/// Safety policy for self-modification
#[derive(Debug, Clone)]
pub struct SafetyPolicy {
    /// Maximum number of self-modifications per session
    pub max_modifications_per_session: u32,
    /// Require human approval for critical changes
    pub require_approval_for_critical: bool,
    /// Auto-rollback on test failure
    pub auto_rollback_on_failure: bool,
    /// Protected files that cannot be modified
    pub protected_files: Vec<String>,
    /// Maximum allowed complexity increase
    pub max_complexity_increase: f64,
    /// Required test coverage threshold
    pub min_test_coverage: f64,
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        Self {
            max_modifications_per_session: 10,
            require_approval_for_critical: true,
            auto_rollback_on_failure: true,
            protected_files: vec![
                "safety/".to_string(),
                "cognitive/safety_rollback.rs".to_string(),
                "Cargo.lock".to_string(),
            ],
            max_complexity_increase: 0.2,
            min_test_coverage: 0.7,
        }
    }
}

/// A checkpoint for rollback
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub id: String,
    pub git_commit: Oid,
    pub timestamp: u64,
    pub description: String,
    pub state_snapshot: StateSnapshot,
}

/// Captures system state for rollback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub modified_files: Vec<String>,
    pub cognitive_state: serde_json::Value,
    pub session_state: serde_json::Value,
    pub metrics: PerformanceSnapshot,
}

/// Manages safety and rollback
pub struct SafetyRollbackManager {
    repo: Repository,
    policy: SafetyPolicy,
    checkpoints: Vec<Checkpoint>,
    current_session_mods: u32,
}

impl SafetyRollbackManager {
    pub fn new(project_root: PathBuf) -> Result<Self> {
        let repo = Repository::open(&project_root)?;
        
        Ok(Self {
            repo,
            policy: SafetyPolicy::default(),
            checkpoints: vec![],
            current_session_mods: 0,
        })
    }
    
    /// Create a checkpoint before modification
    pub fn create_checkpoint(&mut self, description: &str) -> Result<Checkpoint> {
        // Check session limits
        if self.current_session_mods >= self.policy.max_modifications_per_session {
            return Err(anyhow!("Session modification limit reached"));
        }
        
        // Stage all changes
        let mut index = self.repo.index()?;
        index.add_all(["*.rs"], git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        
        // Create commit
        let signature = Signature::now("Selfware", "self@ware.ai")?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        
        let parent = self.repo.head()?.peel_to_commit()?;
        
        let commit_id = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &format!("[SELF-MOD] {}", description),
            &tree,
            &[&parent],
        )?;
        
        // Capture state snapshot
        let snapshot = self.capture_state()?;
        
        let checkpoint = Checkpoint {
            id: format!("chk-{}", commit_id),
            git_commit: commit_id,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_secs(),
            description: description.to_string(),
            state_snapshot: snapshot,
        };
        
        self.checkpoints.push(checkpoint.clone());
        self.current_session_mods += 1;
        
        info!("Created checkpoint: {}", checkpoint.id);
        Ok(checkpoint)
    }
    
    /// Rollback to a specific checkpoint
    pub fn rollback(&mut self, checkpoint_id: &str) -> Result<()> {
        let checkpoint = self.checkpoints
            .iter()
            .find(|c| c.id == checkpoint_id)
            .ok_or_else(|| anyhow!("Checkpoint not found: {}", checkpoint_id))?;
        
        // Git reset to checkpoint commit
        let commit = self.repo.find_commit(checkpoint.git_commit)?;
        self.repo.reset(
            commit.as_object(),
            git2::ResetType::Hard,
            None,
        )?;
        
        // Restore cognitive state
        self.restore_state(&checkpoint.state_snapshot)?;
        
        info!("Rolled back to checkpoint: {}", checkpoint_id);
        Ok(())
    }
    
    /// Rollback to last known good state
    pub fn rollback_last(&mut self) -> Result<()> {
        if let Some(checkpoint) = self.checkpoints.last() {
            self.rollback(&checkpoint.id)?;
        }
        Ok(())
    }
    
    /// Validate proposed modification against safety policy
    pub fn validate_modification(&self, modification: &ProposedModification) -> Result<()> {
        // Check protected files
        for file in &modification.affected_files {
            for protected in &self.policy.protected_files {
                if file.contains(protected) {
                    return Err(anyhow!(
                        "Cannot modify protected file: {}",
                        file
                    ));
                }
            }
        }
        
        // Check safety level
        if modification.safety_level == SafetyLevel::Critical 
            && self.policy.require_approval_for_critical {
            return Err(anyhow!(
                "Critical modification requires human approval"
            ));
        }
        
        Ok(())
    }
    
    /// Capture current system state
    fn capture_state(&self) -> Result<StateSnapshot> {
        // Get list of modified files
        let statuses = self.repo.statuses(None)?;
        let modified_files: Vec<String> = statuses
            .iter()
            .map(|s| s.path().unwrap_or("").to_string())
            .collect();
        
        Ok(StateSnapshot {
            modified_files,
            cognitive_state: serde_json::json!({}),
            session_state: serde_json::json!({}),
            metrics: PerformanceSnapshot::default(),
        })
    }
    
    /// Restore system state
    fn restore_state(&self, snapshot: &StateSnapshot) -> Result<()> {
        // Restore cognitive and session state
        Ok(())
    }
}
```

#### 2.6 Differential Testing (`cognitive/differential_testing.rs`)

```rust
//! Differential Testing for Self-Modified Code
//! 
//! Compares behavior of original vs modified code.

/// Test case for differential testing
#[derive(Debug, Clone)]
pub struct DifferentialTestCase {
    pub name: String,
    pub input: TestInput,
    pub expected_behavior: BehaviorExpectation,
}

#[derive(Debug, Clone)]
pub enum TestInput {
    /// Raw text input
    Text(String),
    /// Structured data
    Json(serde_json::Value),
    /// Binary data
    Binary(Vec<u8>),
    /// Function call with args
    FunctionCall { name: String, args: Vec<serde_json::Value> },
}

#[derive(Debug, Clone)]
pub enum BehaviorExpectation {
    /// Output should be exactly equal
    ExactMatch,
    /// Output should be semantically equivalent
    SemanticEquivalence,
    /// Performance should not regress
    PerformanceBound { max_regression: f64 },
    /// Should not panic
    NoPanic,
    /// Custom validation
    Custom(Box<dyn Fn(&TestOutput) -> bool>),
}

/// Result of a differential test
#[derive(Debug, Clone)]
pub struct DifferentialTestResult {
    pub test_name: String,
    pub passed: bool,
    pub original_output: TestOutput,
    pub modified_output: TestOutput,
    pub differences: Vec<OutputDifference>,
}

#[derive(Debug, Clone)]
pub struct TestOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub memory_usage: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct OutputDifference {
    pub field: String,
    pub original: String,
    pub modified: String,
    pub severity: DifferenceSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifferenceSeverity {
    Info,
    Warning,
    Critical,
}

/// Differential testing engine
pub struct DifferentialTester {
    original_binary: PathBuf,
    modified_binary: PathBuf,
    test_cases: Vec<DifferentialTestCase>,
}

impl DifferentialTester {
    pub fn new(original: PathBuf, modified: PathBuf) -> Self {
        Self {
            original_binary: original,
            modified_binary: modified,
            test_cases: vec![],
        }
    }
    
    /// Add a test case
    pub fn add_test_case(&mut self, case: DifferentialTestCase) {
        self.test_cases.push(case);
    }
    
    /// Run all differential tests
    pub fn run_tests(&self) -> Result<Vec<DifferentialTestResult>> {
        let mut results = vec![];
        
        for case in &self.test_cases {
            let result = self.run_single_test(case)?;
            results.push(result);
        }
        
        Ok(results)
    }
    
    /// Run a single differential test
    fn run_single_test(&self, case: &DifferentialTestCase) -> Result<DifferentialTestResult> {
        // Run original version
        let original_output = self.execute(&self.original_binary, &case.input)?;
        
        // Run modified version
        let modified_output = self.execute(&self.modified_binary, &case.input)?;
        
        // Compare outputs
        let differences = self.compare_outputs(&original_output, &modified_output);
        
        // Check against expectations
        let passed = self.check_expectations(case, &differences)?;
        
        Ok(DifferentialTestResult {
            test_name: case.name.clone(),
            passed,
            original_output,
            modified_output,
            differences,
        })
    }
    
    /// Execute binary with input
    fn execute(&self, binary: &Path, input: &TestInput) -> Result<TestOutput> {
        let start = Instant::now();
        
        let mut cmd = Command::new(binary);
        
        match input {
            TestInput::Text(text) => {
                cmd.stdin(Stdio::piped());
                let mut child = cmd.spawn()?;
                if let Some(stdin) = child.stdin.take() {
                    stdin.write_all(text.as_bytes())?;
                }
                let output = child.wait_with_output()?;
                
                Ok(TestOutput {
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    exit_code: output.status.code(),
                    duration: start.elapsed(),
                    memory_usage: None,
                })
            }
            _ => unimplemented!(),
        }
    }
    
    /// Compare two outputs
    fn compare_outputs(&self, original: &TestOutput, modified: &TestOutput) -> Vec<OutputDifference> {
        let mut differences = vec![];
        
        // Compare stdout
        if original.stdout != modified.stdout {
            differences.push(OutputDifference {
                field: "stdout".to_string(),
                original: original.stdout.clone(),
                modified: modified.stdout.clone(),
                severity: DifferenceSeverity::Critical,
            });
        }
        
        // Compare stderr
        if original.stderr != modified.stderr {
            differences.push(OutputDifference {
                field: "stderr".to_string(),
                original: original.stderr.clone(),
                modified: modified.stderr.clone(),
                severity: DifferenceSeverity::Warning,
            });
        }
        
        // Compare exit codes
        if original.exit_code != modified.exit_code {
            differences.push(OutputDifference {
                field: "exit_code".to_string(),
                original: format!("{:?}", original.exit_code),
                modified: format!("{:?}", modified.exit_code),
                severity: DifferenceSeverity::Critical,
            });
        }
        
        // Compare performance
        let perf_diff = (modified.duration.as_secs_f64() - original.duration.as_secs_f64())
            / original.duration.as_secs_f64();
        if perf_diff > 0.1 { // 10% regression
            differences.push(OutputDifference {
                field: "performance".to_string(),
                original: format!("{:?}", original.duration),
                modified: format!("{:?}", modified.duration),
                severity: DifferenceSeverity::Warning,
            });
        }
        
        differences
    }
    
    /// Check if differences meet expectations
    fn check_expectations(
        &self,
        case: &DifferentialTestCase,
        differences: &[OutputDifference],
    ) -> Result<bool> {
        match &case.expected_behavior {
            BehaviorExpectation::ExactMatch => {
                Ok(differences.is_empty())
            }
            BehaviorExpectation::NoPanic => {
                let has_panic = differences.iter().any(|d| {
                    d.field == "exit_code" && d.modified == "Some(101)"
                });
                Ok(!has_panic)
            }
            _ => Ok(true),
        }
    }
}
```

### 3. Integration with Existing System

#### 3.1 Enhanced SelfEditOrchestrator

```rust
//! Enhanced Self-Edit Orchestrator with Full Self-Modification Support

use crate::cognitive::{
    code_analysis::CodeAnalyzer,
    code_transformer::{AstTransformer, CodeTransformation},
    compilation_manager::CompilationManager,
    hot_reload::HotReloadManager,
    safety_rollback::SafetyRollbackManager,
    differential_testing::DifferentialTester,
};

/// Enhanced orchestrator with full self-modification capabilities
pub struct RecursiveSelfImprovementEngine {
    /// Original self-edit orchestrator
    base: SelfEditOrchestrator,
    /// Code analysis engine
    analyzer: CodeAnalyzer,
    /// AST transformer
    transformer: AstTransformer,
    /// Compilation manager
    compiler: CompilationManager,
    /// Hot-reload manager
    hot_reload: HotReloadManager,
    /// Safety and rollback
    safety: SafetyRollbackManager,
    /// Differential tester
    differential: DifferentialTester,
}

impl RecursiveSelfImprovementEngine {
    pub fn new(project_root: PathBuf) -> Result<Self> {
        Ok(Self {
            base: SelfEditOrchestrator::new(project_root.clone()),
            analyzer: CodeAnalyzer::new(),
            transformer: AstTransformer::new(),
            compiler: CompilationManager::new(project_root.clone())?,
            hot_reload: HotReloadManager::new(),
            safety: SafetyRollbackManager::new(project_root.clone())?,
            differential: DifferentialTester::new(
                project_root.join("target/release/selfware"),
                project_root.join(".selfware/build/selfware"),
            ),
        })
    }
    
    /// Main self-improvement loop
    pub async fn run_improvement_cycle(&mut self) -> Result<ImprovementCycleResult> {
        info!("Starting recursive self-improvement cycle");
        
        // Step 1: Analyze codebase
        let analysis = self.analyze_codebase().await?;
        info!("Analysis complete: {} issues found", analysis.issues.len());
        
        // Step 2: Prioritize improvements
        let targets = self.prioritize_improvements(analysis).await?;
        
        // Step 3: Select best target
        let target = self.select_target(&targets)
            .ok_or_else(|| anyhow!("No improvement targets found"))?;
        info!("Selected target: {:?}", target);
        
        // Step 4: Create checkpoint
        let checkpoint = self.safety.create_checkpoint(&format!(
            "Before improvement: {}",
            target.description
        ))?;
        
        // Step 5: Generate transformation
        let transformation = self.generate_transformation(target).await?;
        
        // Step 6: Validate against safety policy
        self.safety.validate_modification(&transformation.into())?;
        
        // Step 7: Apply transformation
        self.apply_transformation(&transformation).await?;
        
        // Step 8: Compile and verify
        let verification = self.compiler.verify()?;
        if !verification.all_passed() {
            if self.safety.policy().auto_rollback_on_failure {
                warn!("Verification failed, rolling back");
                self.safety.rollback(&checkpoint.id)?;
                return Ok(ImprovementCycleResult::RolledBack);
            }
            return Err(anyhow!("Verification failed"));
        }
        
        // Step 9: Differential testing
        let diff_results = self.run_differential_tests().await?;
        if !diff_results.iter().all(|r| r.passed) {
            warn!("Differential tests failed, rolling back");
            self.safety.rollback(&checkpoint.id)?;
            return Ok(ImprovementCycleResult::RolledBack);
        }
        
        // Step 10: Hot-reload if possible
        if self.can_hot_reload(&transformation) {
            self.hot_reload_changes(&transformation).await?;
        }
        
        // Step 11: Record success
        self.record_success(target, &checkpoint, &verification).await?;
        
        info!("Improvement cycle completed successfully");
        Ok(ImprovementCycleResult::Success)
    }
    
    /// Analyze the entire codebase
    async fn analyze_codebase(&self) -> Result<CodebaseAnalysis> {
        let src_dir = self.base.project_root.join("src");
        let files = glob_rs_files(&src_dir)?;
        
        let mut all_issues = vec![];
        let mut file_analyses = vec![];
        
        for file in files {
            let analysis = self.analyzer.analyze_file(&file).await?;
            all_issues.extend(analysis.issues.clone());
            file_analyses.push(analysis);
        }
        
        Ok(CodebaseAnalysis {
            files: file_analyses,
            issues: all_issues,
        })
    }
    
    /// Generate transformation for a target
    async fn generate_transformation(
        &self,
        target: &ImprovementTarget,
    ) -> Result<CodeTransformation> {
        // Use LLM to generate transformation
        let prompt = self.build_transformation_prompt(target);
        
        // Call Qwen3 Coder via local API
        let response = self.call_local_llm(&prompt).await?;
        
        // Parse response into transformation
        self.parse_transformation(&response, target)
    }
    
    /// Apply transformation to source code
    async fn apply_transformation(&self, transformation: &CodeTransformation) -> Result<()> {
        let file_path = &transformation.target_file;
        let content = fs::read_to_string(file_path)?;
        
        let transformed = self.transformer.transform(&content, transformation)?;
        
        fs::write(file_path, transformed)?;
        
        info!("Applied transformation to: {:?}", file_path);
        Ok(())
    }
    
    /// Build prompt for transformation generation
    fn build_transformation_prompt(&self, target: &ImprovementTarget) -> String {
        format!(r#"
You are an expert Rust programmer. Generate a code transformation to address the following improvement target:

**Target**: {:?}
**Category**: {:?}
**Description**: {}
**File**: {:?}

Please provide the transformation in the following format:

```rust
// TRANSFORMATION_START
// transformation_type: AddFunction | ModifyFunction | ExtractFunction | etc.
// target_file: path/to/file.rs
// safety_level: Additive | BehaviorPreserving | BehaviorChanging

// Your Rust code here

// TRANSFORMATION_END
```

Requirements:
1. Code must be syntactically valid Rust
2. Preserve existing behavior unless explicitly improving it
3. Follow Rust best practices
4. Include proper error handling
5. Add documentation for public items
"#,
            target.id,
            target.category,
            target.description,
            target.file,
        )
    }
    
    /// Call local Qwen3 Coder
    async fn call_local_llm(&self, prompt: &str) -> Result<String> {
        // Integration with local LLM (ollama, llama.cpp, etc.)
        let client = reqwest::Client::new();
        let response = client
            .post("http://localhost:11434/api/generate")
            .json(&json!({
                "model": "qwen3-coder:32b",
                "prompt": prompt,
                "stream": false,
                "options": {
                    "temperature": 0.2,
                    "num_ctx": 131072, // 128K context for full codebase
                }
            }))
            .send()
            .await?;
        
        let result: serde_json::Value = response.json().await?;
        Ok(result["response"].as_str().unwrap_or("").to_string())
    }
}
```

### 4. Cargo.toml Dependencies

```toml
[dependencies]
# Core dependencies
syn = { version = "2.0", features = ["full", "visit", "visit-mut", "parsing"] }
quote = "1.0"
proc-macro2 = "1.0"
prettyplease = "0.2"

# Hot-reloading
libloading = "0.8"
notify = "6.1"

# Git operations for rollback
git2 = "0.18"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Async runtime
tokio = { version = "1.35", features = ["full"] }

# HTTP client for LLM API
reqwest = { version = "0.11", features = ["json"] }

# Process management
duct = "0.13"

# File system operations
tempfile = "3.9"
walkdir = "2.4"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Testing
mockall = "0.12"

[dev-dependencies]
# Additional test utilities
assert_cmd = "2.0"
predicates = "3.0"
```

### 5. Workflow Summary

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Recursive Self-Improvement Workflow                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. ANALYZE                                                                  │
│     ├── Parse all Rust files using syn                                       │
│     ├── Calculate complexity metrics                                         │
│     ├── Identify code smells and issues                                      │
│     └── Generate improvement targets                                         │
│                                                                              │
│  2. PRIORITIZE                                                               │
│     ├── Score targets by impact × confidence                                 │
│     ├── Filter by safety policy                                              │
│     └── Select highest-priority target                                       │
│                                                                              │
│  3. CREATE CHECKPOINT                                                        │
│     ├── Git commit current state                                             │
│     ├── Capture cognitive state                                              │
│     └── Store checkpoint metadata                                            │
│                                                                              │
│  4. GENERATE TRANSFORMATION                                                  │
│     ├── Build prompt with target context                                     │
│     ├── Call Qwen3 Coder (local)                                             │
│     ├── Parse AST transformation                                             │
│     └── Validate transformation safety                                       │
│                                                                              │
│  5. APPLY TRANSFORMATION                                                     │
│     ├── Parse target file to AST                                             │
│     ├── Apply transformation using syn/quote                                 │
│     ├── Format output with prettyplease                                      │
│     └── Write modified file                                                  │
│                                                                              │
│  6. VERIFY                                                                   │
│     ├── Run cargo check                                                      │
│     ├── Run cargo clippy                                                     │
│     ├── Run cargo test                                                       │
│     └── Check code formatting                                                │
│                                                                              │
│  7. DIFFERENTIAL TEST                                                        │
│     ├── Build original and modified binaries                                 │
│     ├── Run test cases on both                                               │
│     ├── Compare outputs                                                      │
│     └── Check behavior expectations                                          │
│                                                                              │
│  8. HOT-RELOAD (if applicable)                                               │
│     ├── Extract state from current component                                 │
│     ├── Load new shared library                                              │
│     ├── Migrate state to new component                                       │
│     └── Verify functionality                                                 │
│                                                                              │
│  9. RECORD RESULT                                                            │
│     ├── Calculate effectiveness score                                        │
│     ├── Update improvement history                                           │
│     └── Learn from outcome                                                   │
│                                                                              │
│  10. ROLLBACK (if needed)                                                    │
│      ├── Git reset to checkpoint                                             │
│      ├── Restore cognitive state                                             │
│      └── Record failure for learning                                         │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 6. Safety Mechanisms

#### 6.1 Multi-Layer Safety

```rust
/// Safety layers for self-modification
pub struct SafetyLayers;

impl SafetyLayers {
    /// Layer 1: Static Analysis
    fn static_analysis(transformation: &CodeTransformation) -> Result<()> {
        // Check for forbidden patterns
        // Verify type safety
        // Check for potential panics
        Ok(())
    }
    
    /// Layer 2: Compilation Check
    fn compilation_check(compiler: &CompilationManager) -> Result<()> {
        let result = compiler.check()?;
        if !result.success {
            return Err(anyhow!("Compilation failed"));
        }
        Ok(())
    }
    
    /// Layer 3: Test Verification
    fn test_verification(compiler: &CompilationManager) -> Result<()> {
        let result = compiler.test(None)?;
        if !result.all_passed() {
            return Err(anyhow!("Tests failed"));
        }
        Ok(())
    }
    
    /// Layer 4: Differential Testing
    fn differential_testing(tester: &DifferentialTester) -> Result<()> {
        let results = tester.run_tests()?;
        if !results.iter().all(|r| r.passed) {
            return Err(anyhow!("Differential tests failed"));
        }
        Ok(())
    }
    
    /// Layer 5: Gradual Rollout
    fn gradual_rollout() -> Result<()> {
        // A/B testing
        // Canary deployment
        // Monitor metrics
        Ok(())
    }
}
```

#### 6.2 Protected Components

```rust
/// Components that cannot be self-modified
pub const PROTECTED_COMPONENTS: &[&str] = &[
    // Core safety
    "cognitive/safety_rollback.rs",
    "cognitive/safety_layers.rs",
    
    // State management
    "session/checkpointing.rs",
    "memory.rs",
    
    // Core infrastructure
    "main.rs",
    "lib.rs",
    
    // Build configuration
    "Cargo.toml",
    "Cargo.lock",
];
```

### 7. Integration Points

#### 7.1 With Existing self_improvement.rs

```rust
// Add to self_improvement.rs

impl SelfImprovementEngine {
    /// Enable recursive self-modification
    pub fn enable_recursive_mode(&mut self) -> Result<()> {
        self.recursive_engine = Some(RecursiveSelfImprovementEngine::new(
            self.project_root.clone(),
        )?);
        Ok(())
    }
    
    /// Run recursive improvement cycle
    pub async fn run_recursive_improvement(&mut self) -> Result<ImprovementResult> {
        if let Some(ref mut engine) = self.recursive_engine {
            engine.run_improvement_cycle().await
        } else {
            Err(anyhow!("Recursive mode not enabled"))
        }
    }
}
```

#### 7.2 With Existing self_edit.rs

```rust
// Extend ImprovementCategory in self_edit.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ImprovementCategory {
    // Existing categories
    PromptTemplate,
    ToolPipeline,
    ErrorHandling,
    VerificationLogic,
    ContextManagement,
    CodeQuality,
    NewCapability,
    
    // NEW: Source code modification categories
    /// Refactor existing code
    CodeRefactoring,
    /// Add new functionality
    FeatureAddition,
    /// Optimize performance
    PerformanceOptimization,
    /// Improve type safety
    TypeSafety,
    /// Add or improve tests
    TestImprovement,
    /// Documentation improvement
    Documentation,
    /// API design improvement
    ApiDesign,
}
```

## Conclusion

This design provides a comprehensive framework for enabling true recursive self-improvement in Selfware:

1. **AST-Based Transformation**: Uses syn/quote for type-safe code modification
2. **Compilation Integration**: Full cargo/rustc integration with verification
3. **Hot-Reloading**: Dynamic code updates without state loss
4. **Safety & Rollback**: Multi-layer safety with git-based versioning
5. **Differential Testing**: Behavior verification before/after changes
6. **Local LLM Integration**: Works with Qwen3 Coder for code generation

The system is designed to be:
- **Safe**: Multiple safety layers prevent catastrophic self-modification
- **Incremental**: Gradual improvements with verification at each step
- **Reversible**: Full rollback capability to any previous state
- **Observable**: Comprehensive logging and metrics
- **Extensible**: Plugin architecture for new transformation types
