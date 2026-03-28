# SWL Implementation Roadmap
## Integration with Selfware Codebase

### Executive Summary

This document outlines the implementation plan for the **Selfware Workflow Language (SWL)** - a declarative-imperative hybrid language for agent orchestration with first-class observability.

### Where SWL Fits in Selfware

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         SELFWARE ARCHITECTURE                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │   Config    │  │    SWL      │  │   Agent     │  │   Command       │ │
│  │   System    │◄─┤  Compiler   │◄─┤   Runtime   │◄─┤   Center        │ │
│  │   (TOML)    │  │   (New)     │  │   (Existing)│  │   (New)         │ │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────────┘ │
│         ▲                ▲                ▲                  ▲          │
│         │                │                │                  │          │
│         └────────────────┴────────────────┴──────────────────┘          │
│                           Orchestration Layer                            │
│                                                                         │
├─────────────────────────────────────────────────────────────────────────┤
│  Existing: api/ agent/ tools/ safety/ cognitive/ orchestration/         │
│  New:      swl/ (compiler, parser, codegen)                             │
│  New:      observability/ (telemetry, dashboard)                        │
└─────────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Foundation (Week 1-2)

#### 1.1 Module Structure

Create new modules:

```
src/
├── swl/                          # NEW: SWL compiler
│   ├── mod.rs                    # Public API
│   ├── parser/                   # YAML + embedded code parsing
│   │   ├── mod.rs
│   │   ├── lexer.rs              # Extend existing workflow_dsl lexer
│   │   ├── ast.rs                # SWL AST definitions
│   │   └── validator.rs          # Semantic validation
│   ├── codegen/                  # Code generation
│   │   ├── mod.rs
│   │   ├── rust_gen.rs           # Generate Rust from SWL
│   │   └── runtime_gen.rs        # Generate runtime configs
│   └── types/                    # Type system
│       ├── mod.rs
│       ├── schema.rs             # State schemas
│       └── checker.rs            # Type checker
│
├── observability/                # NEW: Telemetry & dashboard
│   ├── mod.rs
│   ├── telemetry.rs              # Metrics collection
│   ├── dashboard/                # Command center
│   │   ├── mod.rs
│   │   ├── server.rs             # WebSocket server
│   │   ├── tui.rs                # ratatui dashboard
│   │   └── web.rs                # Web UI (optional)
│   └── storage/                  # Metrics storage
│       ├── mod.rs
│       └── ring_buffer.rs        # Time-series storage
│
└── orchestration/
    └── workflow_dsl/
        └── mod.rs                # MODIFY: Re-export SWL types
```

#### 1.2 Leverage Existing Code

**Reuse opportunities identified by agents:**

| Existing Component | How SWL Uses It | Changes Needed |
|-------------------|-----------------|----------------|
| `src/config/` | SWL compiles to Config structs | Add SWL→Config conversion |
| `src/orchestration/workflow_dsl/` | Extend with SWL features | Gradual migration path |
| `src/ui/tui/` | Command center TUI base | Add telemetry panels |
| `src/agent/` | SWL workflows drive agents | Add instrumentation hooks |
| `src/errors.rs` | Typed errors for SWL | Add SWLError variant |

### Phase 2: Parser Implementation (Week 3-4)

#### 2.1 Parser Architecture

**Decision: Two-stage parsing**

```rust
// Stage 1: Parse declarative YAML layer
let yaml_doc = serde_yaml::from_str::<SwlDocument>(source)?;

// Stage 2: Extract and parse embedded code blocks
for code_block in yaml_doc.extract_code_blocks() {
    let ast = match code_block.language {
        "rust" => parse_rust(&code_block.content)?,
        "python" => parse_python(&code_block.content)?,
        _ => bail!("Unsupported language: {}", code_block.language),
    };
}

// Stage 3: Type checking across boundaries
type_check(&yaml_doc, &code_blocks)?;
```

**Why this approach:**
- Reuses `serde_yaml` (already in Cargo.toml)
- Embedded code is parsed by language-specific parsers (rustc API, rustpython)
- Type checking ensures declarative and imperative parts agree

#### 2.2 AST Design

```rust
// src/swl/parser/ast.rs

pub struct WorkflowDocument {
    pub version: String,
    pub name: String,
    pub metadata: Metadata,
    pub agents: Vec<AgentDefinition>,
    pub workflows: Vec<WorkflowDefinition>,
    pub state: StateSchema,
    pub telemetry: TelemetryConfig,
    pub guardrails: Vec<Guardrail>,
}

pub struct AgentDefinition {
    pub name: String,
    pub model: ModelSpec,
    pub role: String,
    pub instruction: String,
    pub tools: Vec<ToolRef>,
    pub sub_agents: Vec<String>,
    pub output_key: Option<String>,
    pub callbacks: CallbackConfig,
}

pub struct WorkflowDefinition {
    pub name: String,
    pub workflow_type: WorkflowType,
    pub steps: Vec<WorkflowStep>,
}

pub enum WorkflowType {
    Sequential,
    Parallel,
    MapReduce,
    Conditional,
}

pub struct Guardrail {
    pub name: String,
    pub guardrail_type: GuardrailType,
    pub condition: CodeBlock,  // Embedded imperative code
    pub on_violation: ViolationAction,
}

pub struct CodeBlock {
    pub language: String,
    pub content: String,
    pub compiled: Option<CompiledCode>,
}
```

### Phase 3: Code Generation (Week 5-6)

#### 3.1 Compilation Target

SWL compiles to **Rust code** that integrates with existing Selfware:

```rust
// Input: workflow.swl
// Output: workflow_generated.rs

pub struct GeneratedWorkflow {
    // Agent definitions become struct fields
    architect: Agent,
    security_auditor: Agent,
    
    // State schema becomes typed struct
    state: WorkflowState,
    
    // Telemetry collector
    telemetry: Arc<TelemetryCollector>,
}

impl GeneratedWorkflow {
    pub async fn parallel_review(&self, input: Input) -> Result<Report> {
        // Map phase: spawn all agents
        let futures = vec![
            self.architect.run(input.clone()),
            self.security_auditor.run(input.clone()),
        ];
        
        // Collect results with telemetry
        let results = join_all(futures).await;
        
        // Reduce phase: custom merge logic from SWL
        self.merge_results(results).await
    }
}
```

**Benefits:**
- Zero runtime parsing overhead
- Type safety at compile time
- Optimized by Rust compiler
- Full IDE support (goto def, autocomplete)

### Phase 4: Observability (Week 7-8)

#### 4.1 Telemetry Integration Points

Instrument existing code at these points:

```rust
// src/agent/execution.rs

async fn execute_step(&mut self) -> Result<()> {
    // BEFORE: Start timing
    let span = self.telemetry.start_inference(&self.agent_name);
    
    // LLM call
    let response = self.llm_client.complete(request).await?;
    
    // AFTER: Record metrics
    span.finish(InferenceMetrics {
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        latency: span.elapsed(),
        cost: calculate_cost(&response),
    });
    
    Ok(response)
}

// src/tools/mod.rs

async fn invoke(&self, args: Value) -> Result<Value> {
    let span = self.telemetry.start_tool_invocation(&self.name);
    
    let result = self.implementation.call(args).await;
    
    span.finish(ToolMetrics {
        duration: span.elapsed(),
        status: result.is_ok(),
    });
    
    result
}
```

#### 4.2 Command Center Architecture

```rust
// src/observability/dashboard/tui.rs

pub struct CommandCenter {
    // Data models
    metrics: Arc<MetricsRingBuffer>,
    topology: TopologyGraph,
    logs: LogBuffer,
    
    // UI components
    latency_chart: SparklineWidget,
    context_gauges: Vec<GaugeWidget>,
    topology_canvas: CanvasWidget,
    log_table: TableWidget,
}

impl CommandCenter {
    pub fn draw(&mut self, frame: &mut Frame) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // Header
                Constraint::Ratio(1, 2), // Top panels
                Constraint::Ratio(1, 2), // Bottom panels
                Constraint::Length(3),   // Footer
            ])
            .split(frame.size());
            
        self.draw_header(frame, layout[0]);
        
        // Split main area into grid
        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(layout[1]);
            
        self.latency_chart.draw(frame, main[0]);
        self.context_gauges.draw(frame, main[1]);
        self.topology_canvas.draw(frame, main[2]);
        
        self.log_table.draw(frame, layout[2]);
        self.draw_footer(frame, layout[3]);
    }
}
```

### Phase 5: Integration (Week 9-10)

#### 5.1 CLI Integration

```rust
// src/cli.rs - Add SWL commands

#[derive(Subcommand)]
enum Commands {
    // ... existing commands
    
    /// Compile and run SWL workflows
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    
    /// Start command center dashboard
    Dashboard {
        /// Port for web interface
        #[arg(short, long, default_value = "8080")]
        port: u16,
        
        /// Show TUI instead of web
        #[arg(short, long)]
        tui: bool,
    },
}

#[derive(Subcommand)]
enum WorkflowAction {
    /// Compile SWL file
    Compile {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    
    /// Run SWL workflow
    Run {
        workflow: PathBuf,
        #[arg(short, long)]
        watch: bool,  // Live reload
    },
    
    /// Validate SWL syntax
    Check {
        workflow: PathBuf,
    },
}
```

#### 5.2 Migration Strategy from Existing Workflow DSL

**Phase A: Coexistence (Weeks 9-10)**
- SWL runs alongside existing workflow_dsl
- New features only in SWL
- Existing workflows continue working

**Phase B: Deprecation (Weeks 11-12)**
- Add deprecation warnings to workflow_dsl
- Provide auto-migration tool: `selfware workflow migrate old.yaml new.swl`

**Phase C: Removal (Week 13+)**
- Remove workflow_dsl module
- SWL becomes the only workflow system

### Implementation Timeline

```
Week 1-2:  Foundation
  - Create swl/ and observability/ modules
  - Set up parser crate dependencies
  - Define AST types
  
Week 3-4:  Parser
  - Implement YAML parsing with serde
  - Add embedded code block extraction
  - Build type checker
  
Week 5-6:  Code Generation
  - Generate Rust code from SWL AST
  - Integrate with existing Agent types
  - Build compilation pipeline
  
Week 7-8:  Observability
  - Add telemetry collection hooks
  - Build TUI dashboard
  - Implement WebSocket server
  
Week 9-10: Integration
  - CLI commands for SWL
  - Command center integration
  - Testing and documentation
  
Week 11+:  Migration & Polish
  - Migration tool from workflow_dsl
  - Performance optimization
  - Advanced features (plugins, remote agents)
```

### Key Design Decisions

#### 1. Why Compile to Rust Instead of Interpret?

| Aspect | Compilation | Interpretation |
|--------|------------|----------------|
| Performance | Zero runtime overhead | Parsing overhead on each run |
| Type Safety | Compile-time checks | Runtime errors |
| IDE Support | Full autocomplete, goto def | Limited |
| Debugging | Standard Rust debugging | Custom debugger needed |
| Deployment | Single binary | Requires SWL runtime |

#### 2. Why Separate observability/ Module?

- Telemetry is **cross-cutting** - needed by agent, tools, safety
- Keeps existing modules clean
- Allows dashboard to be optional feature
- Enables standalone monitoring tools

#### 3. How Does SWL Relate to Config?

```
SWL (High-level intent)
    ↓ compiles to
Config (Runtime configuration)
    ↓ drives
Agent Runtime (Execution)
```

SWL is **authoring format**, Config is **runtime format**.

### Success Metrics

- [ ] SWL parser handles all example workflows
- [ ] Compiled workflows run at same speed as hand-written
- [ ] Command center shows real-time metrics with <100ms latency
- [ ] Migration tool converts 90%+ of existing workflows automatically
- [ ] Test coverage >80% for swl/ and observability/ modules

### Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Parser complexity | Start with serde_yaml, add custom lexer later if needed |
| Performance overhead | Compile-time code generation, zero-cost abstractions |
| Breaking changes | Deprecation cycle, migration tool, backward compat layer |
| Scope creep | MVP first: declarative agents + telemetry only |

### Next Steps

1. **Approve roadmap** - Review with team
2. **Create milestone** - Set up tracking for 10-week plan
3. **Start Phase 1** - Begin module structure and foundation
4. **Parallel work** - Observability can start independently

---

**Document Version:** 1.0  
**Last Updated:** March 28, 2026  
**Status:** Draft for Review
