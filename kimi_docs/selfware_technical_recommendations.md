# Selfware Technical Recommendations: Detailed Implementation Guide

## 1. MCP Client Implementation

### 1.1 Protocol Overview

The Model Context Protocol (MCP) is a JSON-RPC based protocol for connecting AI applications to external tools. It supports three transport modes:

- **stdio**: Local servers via standard input/output
- **SSE**: Server-Sent Events for remote servers
- **HTTP**: Streamable HTTP for cloud APIs

### 1.2 Recommended Implementation

```rust
// Core MCP client structure
pub struct McpClient {
    transport: Box<dyn McpTransport>,
    server_info: ServerInfo,
    tools: Vec<Tool>,
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn initialize(&mut self) -> Result<ServerInfo>;
    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolResult>;
    async fn list_tools(&self) -> Result<Vec<Tool>>;
}

// stdio transport implementation
pub struct StdioTransport {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: AtomicU64,
}
```

### 1.3 Integration Points

1. **Tool Registry Extension**
   - Add MCP tools to existing 54 built-in tools
   - Dynamic tool discovery at runtime
   - Tool name collision handling

2. **Configuration**
   ```toml
   [mcp.servers]
   github = { command = "npx", args = ["-y", "@modelcontextprotocol/server-github"] }
   filesystem = { command = "npx", args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user"] }
   ```

3. **Permission Model**
   - Extend existing safety framework
   - Per-server permission grants
   - Tool-level access control

### 1.4 Priority MCP Servers to Support

| Server | Use Case | Priority |
|--------|----------|----------|
| GitHub | Issues, PRs, repos | P0 |
| Filesystem | File operations | P0 |
| Playwright | Browser automation | P1 |
| PostgreSQL | Database queries | P1 |
| Brave Search | Web search | P2 |

---

## 2. Hooks System Design

### 2.1 Event Types (Prioritized)

**Phase 1: Core Events**
- `PreToolUse`: Block/validate before execution
- `PostToolUse`: Run after successful execution
- `Stop`: Verify work before completion

**Phase 2: Extended Events**
- `SessionStart`: Initialize environment
- `SessionEnd`: Cleanup, reporting
- `SubagentStart/Stop`: Multi-agent coordination

### 2.2 Handler Types

```rust
pub enum HookHandler {
    /// Shell command execution
    Command { command: String },
    
    /// Single-turn LLM evaluation
    Prompt { prompt: String, model: Option<String> },
    
    /// Multi-turn agent verification
    Agent { prompt: String, timeout: u64 },
}

pub struct Hook {
    pub event: HookEvent,
    pub matcher: Option<String>, // regex pattern
    pub handler: HookHandler,
}
```

### 2.3 Configuration Format

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Write|Edit",
        "handler": {
          "type": "command",
          "command": "./scripts/validate-file.sh"
        }
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Write",
        "handler": {
          "type": "command", 
          "command": "cargo fmt"
        }
      }
    ],
    "Stop": [
      {
        "handler": {
          "type": "prompt",
          "prompt": "Verify all tests pass. $ARGUMENTS"
        }
      }
    ]
  }
}
```

### 2.4 Common Hook Patterns

**Auto-formatting**:
```json
{
  "event": "PostToolUse",
  "matcher": "Write|Edit",
  "handler": {
    "type": "command",
    "command": "prettier --write {{file_path}}"
  }
}
```

**Protected Files**:
```json
{
  "event": "PreToolUse",
  "matcher": "Write|Edit",
  "handler": {
    "type": "command",
    "command": "[[ '{{file_path}}' != *.env* ]] || exit 2"
  }
}
```

**Test Verification**:
```json
{
  "event": "Stop",
  "handler": {
    "type": "command",
    "command": "cargo test"
  }
}
```

---

## 3. LSP Client Integration

### 3.1 Recommended Crate

Use `tower-lsp` for LSP client implementation:

```toml
[dependencies]
tower-lsp = "0.20"
lsp-types = "0.95"
tokio = { version = "1", features = ["full"] }
```

### 3.2 Core Operations

```rust
pub struct LspClient {
    client: Client,
    server_process: Child,
    capabilities: ServerCapabilities,
}

impl LspClient {
    /// Navigate to symbol definition
    pub async fn goto_definition(
        &self,
        file_path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(file_path).unwrap(),
                },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        
        self.client.goto_definition(params).await
    }
    
    /// Find all references to a symbol
    pub async fn find_references(
        &self,
        file_path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        // Implementation
    }
    
    /// Get document symbols
    pub async fn document_symbol(
        &self,
        file_path: &Path,
    ) -> Result<Vec<DocumentSymbol>> {
        // Implementation
    }
}
```

### 3.3 Tool Integration

Add LSP as a tool in Selfware:

```rust
pub struct LspTool {
    clients: HashMap<String, LspClient>, // language -> client
}

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "LSP"
    }
    
    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let operation = args.get("operation").as_str()?;
        let file_path = args.get("file").as_str()?;
        
        match operation {
            "goToDefinition" => self.goto_definition(file_path, args).await,
            "findReferences" => self.find_references(file_path, args).await,
            "documentSymbol" => self.document_symbol(file_path).await,
            "hover" => self.hover(file_path, args).await,
            _ => Err(Error::UnknownOperation),
        }
    }
}
```

### 3.4 Language Server Configuration

```toml
[lsp]
rust = { command = "rust-analyzer" }
typescript = { command = "typescript-language-server", args = ["--stdio"] }
python = { command = "pylsp" }
go = { command = "gopls" }
```

---

## 4. VS Code Extension Architecture

### 4.1 Extension Structure

```
selfware-vscode/
├── package.json          # Extension manifest
├── src/
│   ├── extension.ts      # Entry point
│   ├── selfware.ts       # Selfware process management
│   ├── diffProvider.ts   # Inline diff viewing
│   ├── chatPanel.ts      # Chat UI
│   └── lspClient.ts      # LSP integration
├── webview/              # Chat UI HTML/CSS/JS
└── syntaxes/             # Syntax highlighting
```

### 4.2 Core Features to Implement

**1. Process Management**:
```typescript
export class SelfwareProcess {
    private process: ChildProcess;
    private rpc: JsonRpcConnection;
    
    async start(workspacePath: string): Promise<void> {
        this.process = spawn('selfware', ['--jsonrpc'], {
            cwd: workspacePath,
            env: { ...process.env, SELFWARE_MODE: 'ide' }
        });
        
        this.rpc = new JsonRpcConnection(this.process);
    }
    
    async sendRequest(method: string, params: any): Promise<any> {
        return this.rpc.request(method, params);
    }
}
```

**2. Diff Provider**:
```typescript
export class SelfwareDiffProvider {
    async showDiff(uri: Uri, original: string, modified: string): Promise<void> {
        const originalUri = Uri.parse(`selfware-original:${uri.path}`);
        const modifiedUri = Uri.parse(`selfware-modified:${uri.path}`);
        
        await workspace.fs.writeFile(originalUri, Buffer.from(original));
        await workspace.fs.writeFile(modifiedUri, Buffer.from(modified));
        
        await commands.executeCommand('vscode.diff', originalUri, modifiedUri);
    }
}
```

**3. Chat Panel**:
```typescript
export class ChatPanel {
    public static createOrShow(extensionUri: Uri): ChatPanel {
        const panel = window.createWebviewPanel(
            'selfwareChat',
            'Selfware',
            ViewColumn.Beside,
            { enableScripts: true }
        );
        
        panel.webview.html = this.getHtmlContent(extensionUri);
        return new ChatPanel(panel);
    }
}
```

### 4.3 Communication Protocol

Use JSON-RPC over stdio for communication between VS Code and Selfware:

```typescript
// VS Code -> Selfware
interface SelfwareRequest {
    jsonrpc: '2.0';
    id: number;
    method: string;
    params: any;
}

// Methods to implement
type SelfwareMethods = 
    | 'initialize'
    | 'chat/message'
    | 'tools/execute'
    | 'tools/list'
    | 'files/read'
    | 'files/edit'
    | 'session/create'
    | 'session/resume';
```

### 4.4 Package.json Configuration

```json
{
  "name": "selfware",
  "displayName": "Selfware",
  "version": "0.1.0",
  "engines": { "vscode": "^1.74.0" },
  "categories": ["Machine Learning", "Debuggers"],
  "activationEvents": ["onStartupFinished"],
  "main": "./out/extension.js",
  "contributes": {
    "commands": [
      {
        "command": "selfware.openChat",
        "title": "Open Chat",
        "category": "Selfware"
      },
      {
        "command": "selfware.explainSelection",
        "title": "Explain Selection",
        "category": "Selfware"
      },
      {
        "command": "selfware.editSelection",
        "title": "Edit with Selfware",
        "category": "Selfware"
      }
    ],
    "menus": {
      "editor/context": [
        {
          "command": "selfware.explainSelection",
          "group": "selfware@1",
          "when": "editorHasSelection"
        }
      ]
    },
    "keybindings": [
      {
        "command": "selfware.openChat",
        "key": "ctrl+shift+c",
        "mac": "cmd+shift+c"
      }
    ]
  }
}
```

---

## 5. Plan Mode Implementation

### 5.1 Mode States

```rust
pub enum AgentMode {
    /// Normal interactive mode
    Normal,
    /// Read-only planning mode
    Plan,
    /// Autonomous execution mode
    Auto,
}

pub struct PlanSession {
    mode: AgentMode,
    plan: Option<Plan>,
    approved: bool,
}

pub struct Plan {
    steps: Vec<PlanStep>,
    estimated_tokens: u64,
    files_affected: Vec<PathBuf>,
}

pub struct PlanStep {
    description: String,
    tool: String,
    file: Option<PathBuf>,
    dependencies: Vec<usize>, // indices of prerequisite steps
}
```

### 5.2 Plan Generation

```rust
impl Agent {
    pub async fn generate_plan(&mut self, task: &str) -> Result<Plan> {
        // Switch to read-only mode
        self.set_mode(AgentMode::Plan);
        
        // Use PDVR cycle for planning
        let context = self.gather_context(task).await?;
        let analysis = self.analyze_requirements(task, &context).await?;
        let plan = self.create_plan(&analysis).await?;
        
        Ok(plan)
    }
    
    async fn create_plan(&self, analysis: &Analysis) -> Result<Plan> {
        let prompt = format!(
            r#"Create a detailed implementation plan for the following task.
            
Task: {}
Analysis: {}

Create a step-by-step plan with:
1. Each step should be atomic and verifiable
2. Include file paths for each step
3. Identify dependencies between steps
4. Estimate token usage

Format as JSON:
{{
  "steps": [
    {{"description": "...", "tool": "Read|Write|Edit|Bash", "file": "...", "dependencies": []}}
  ],
  "estimated_tokens": 1234,
  "files_affected": ["..."]
}}"#,
            analysis.task, analysis.details
        );
        
        let response = self.llm.complete(&prompt).await?;
        let plan: Plan = serde_json::from_str(&response)?;
        
        Ok(plan)
    }
}
```

### 5.3 User Approval Flow

```rust
impl PlanSession {
    pub async fn present_for_approval(&self) -> Result<UserDecision> {
        println!("\n📋 Implementation Plan");
        println!("=====================");
        
        for (i, step) in self.plan.steps.iter().enumerate() {
            println!("{}. {} - {}", i + 1, step.tool, step.description);
            if let Some(file) = &step.file {
                println!("   File: {}", file.display());
            }
        }
        
        println!("\nEstimated tokens: {}", self.plan.estimated_tokens);
        println!("Files affected: {}", self.plan.files_affected.len());
        
        println!("\n[execute] Execute plan");
        println!("[modify]  Modify plan");
        println!("[cancel]  Cancel");
        
        // Get user input
        let decision = self.get_user_input().await?;
        Ok(decision)
    }
}
```

---

## 6. Named Sessions Implementation

### 6.1 Session Structure

```rust
pub struct SessionManager {
    sessions_dir: PathBuf,
    active_session: Option<String>,
}

pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub checkpoint: Checkpoint,
    pub metadata: SessionMetadata,
}

pub struct SessionMetadata {
    pub project_path: PathBuf,
    pub task_description: String,
    pub files_accessed: Vec<PathBuf>,
    pub total_tokens_used: u64,
}
```

### 6.2 Session Persistence

```rust
impl SessionManager {
    pub async fn create_session(&self, name: &str, project_path: &Path) -> Result<Session> {
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            checkpoint: Checkpoint::new(),
            metadata: SessionMetadata {
                project_path: project_path.to_path_buf(),
                task_description: String::new(),
                files_accessed: Vec::new(),
                total_tokens_used: 0,
            },
        };
        
        self.save_session(&session).await?;
        Ok(session)
    }
    
    pub async fn resume_session(&self, name: &str) -> Result<Session> {
        let session_path = self.sessions_dir.join(format!("{}.json", name));
        let data = fs::read_to_string(&session_path).await?;
        let mut session: Session = serde_json::from_str(&data)?;
        
        session.last_accessed = Utc::now();
        self.save_session(&session).await?;
        
        Ok(session)
    }
    
    pub async fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let mut sessions = Vec::new();
        
        let mut entries = fs::read_dir(&self.sessions_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension() == Some("json".as_ref()) {
                let data = fs::read_to_string(entry.path()).await?;
                let session: Session = serde_json::from_str(&data)?;
                sessions.push(SessionSummary {
                    name: session.name,
                    created_at: session.created_at,
                    last_accessed: session.last_accessed,
                    task_preview: session.metadata.task_description.chars().take(50).collect(),
                });
            }
        }
        
        sessions.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        Ok(sessions)
    }
}
```

### 6.3 CLI Integration

```rust
// In CLI handler
match command {
    Commands::Resume { name } => {
        let session = session_manager.resume_session(&name).await?;
        let mut agent = Agent::from_session(session).await?;
        agent.run().await?;
    }
    
    Commands::Sessions => {
        let sessions = session_manager.list_sessions().await?;
        for session in sessions {
            println!("{} - {} - {}", 
                session.name,
                session.last_accessed.format("%Y-%m-%d %H:%M"),
                session.task_preview
            );
        }
    }
    
    Commands::Rename { old_name, new_name } => {
        session_manager.rename_session(&old_name, &new_name).await?;
    }
}
```

---

## 7. Integration with Continue.dev

### 7.1 Architecture

Instead of building a full VS Code extension from scratch, Selfware can integrate as a backend for Continue.dev:

```
┌─────────────────┐
│   Continue.dev  │  (VS Code extension)
│   Extension     │
└────────┬────────┘
         │ JSON-RPC
┌────────▼────────┐
│  Selfware Core  │  (Continue config adapter)
│  - MCP tools    │
│  - Swarm agents │
│  - Hooks        │
└─────────────────┘
```

### 7.2 Continue.dev Config Adapter

```rust
pub struct ContinueAdapter {
    agent: Agent,
}

impl ContinueAdapter {
    /// Convert Selfware response to Continue format
    pub fn to_continue_message(&self, response: AgentResponse) -> ContinueMessage {
        ContinueMessage {
            role: if response.from_agent { "assistant" } else { "user" },
            content: response.content,
            tool_calls: response.tool_calls.into_iter()
                .map(|t| self.to_continue_tool_call(t))
                .collect(),
        }
    }
    
    /// Handle Continue.dev tool format
    pub async fn execute_continue_tool(&self, tool: ContinueTool) -> Result<ToolResult> {
        match tool.name.as_str() {
            "selfware_readFile" => {
                self.agent.tools.execute("Read", json!({
                    "file_path": tool.args["path"]
                })).await
            }
            "selfware_editFile" => {
                self.agent.tools.execute("Edit", json!({
                    "file_path": tool.args["path"],
                    "old_string": tool.args["oldString"],
                    "new_string": tool.args["newString"]
                })).await
            }
            "selfware_runCommand" => {
                self.agent.tools.execute("Bash", json!({
                    "command": tool.args["command"]
                })).await
            }
            _ => Err(Error::UnknownTool),
        }
    }
}
```

### 7.3 Benefits

1. **Faster time-to-market**: Leverage Continue's IDE integration
2. **Broader IDE support**: VS Code + JetBrains
3. **Established user base**: 26K+ GitHub stars
4. **Focus on differentiators**: Swarm, evolution, VLM

---

## 8. Testing Strategy

### 8.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mcp_client_initialization() {
        let client = McpClient::new_stdio("npx", &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"])
            .await
            .unwrap();
        
        let tools = client.list_tools().await.unwrap();
        assert!(!tools.is_empty());
    }
    
    #[tokio::test]
    async fn test_hook_execution() {
        let hook = Hook {
            event: HookEvent::PreToolUse,
            matcher: Some("Write".to_string()),
            handler: HookHandler::Command {
                command: "echo 'blocked' && exit 2".to_string(),
            },
        };
        
        let result = hook.execute(&tool_input).await;
        assert!(result.is_blocked());
    }
}
```

### 8.2 Integration Tests

```rust
#[tokio::test]
async fn test_full_workflow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut agent = Agent::new(Config::default()).await.unwrap();
    
    // Test file creation
    let result = agent.run_task("Create a hello world Rust program").await;
    assert!(result.is_ok());
    
    // Verify file exists
    let main_rs = temp_dir.path().join("src/main.rs");
    assert!(main_rs.exists());
    
    // Test compilation
    let output = Command::new("cargo")
        .args(&["build"])
        .current_dir(temp_dir.path())
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
}
```

### 8.3 Benchmark Tests

Extend existing SAB benchmark to cover new features:

```rust
pub struct McpBenchmark;

impl Benchmark for McpBenchmark {
    fn name(&self) -> &str {
        "mcp_tool_execution"
    }
    
    async fn run(&self, agent: &mut Agent) -> BenchmarkResult {
        // Test MCP tool discovery
        let start = Instant::now();
        let tools = agent.list_mcp_tools().await.unwrap();
        let discovery_time = start.elapsed();
        
        // Test MCP tool execution
        let start = Instant::now();
        let result = agent.execute_mcp_tool("filesystem_read", json!({
            "path": "/tmp/test.txt"
        })).await.unwrap();
        let execution_time = start.elapsed();
        
        BenchmarkResult {
            score: calculate_score(discovery_time, execution_time, result),
            metrics: vec![
                ("discovery_ms", discovery_time.as_millis() as f64),
                ("execution_ms", execution_time.as_millis() as f64),
            ],
        }
    }
}
```

---

## 9. Documentation Requirements

### 9.1 User Documentation

1. **Quick Start Guide**
   - Installation (`cargo install selfware`)
   - First task
   - Basic configuration

2. **Feature Documentation**
   - MCP setup and usage
   - Hooks configuration
   - IDE integration
   - Swarm mode

3. **Tutorials**
   - Building a web app with Selfware
   - Refactoring with Plan Mode
   - Setting up custom agents

### 9.2 API Documentation

- Rust docs (docs.rs)
- MCP protocol implementation
- LSP client API
- Configuration schema

### 9.3 Contributor Documentation

- Architecture overview
- Development setup
- Testing guidelines
- Contribution workflow

---

## 10. Migration Path

### 10.1 From Claude Code

```bash
# Export Claude Code configuration
claude config export > claude-config.json

# Import to Selfware
selfware config import --from-claude claude-config.json

# Migrate CLAUDE.md to Selfware format
cp CLAUDE.md .selfware/instructions.md
```

### 10.2 From Aider

```bash
# Aider uses similar Git-native approach
# Selfware can read .aider.conf.yml
selfware config import --from-aider .aider.conf.yml
```

---

*Technical recommendations based on analysis of Selfware v0.1.4 and competitive landscape.*
