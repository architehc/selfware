# Selfware Computer Control Capabilities - Architectural Plan

## Executive Summary

This document provides a comprehensive architectural plan for adding computer control capabilities to Selfware, an agentic AI harness written in Rust. The system will enable Selfware to control computers like a human through MCP integration, browser automation, mouse/keyboard simulation, and screen understanding.

**Current Selfware Foundation:**
- ~197k lines of Rust code across 199 source files
- 54 built-in tools for file, git, cargo, search, shell operations
- Multi-agent swarm supporting up to 16 concurrent agents
- TUI dashboard built with ratatui
- PDVR cognitive cycle (Plan-Do-Verify-Reflect)
- Multi-layer safety framework

---

## 1. High-Level System Architecture

### 1.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SELFWARE AGENTIC HARNESS                          │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │   MCP Host   │  │   Cognitive  │  │  Agent Swarm │  │   TUI/CLI    │   │
│  │   (Client)   │  │   Engine     │  │  Orchestrator│  │   Interface  │   │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘   │
│         │                 │                 │                 │           │
│  ┌──────┴─────────────────┴─────────────────┴─────────────────┴───────┐   │
│  │                    CORE CONTROL LAYER                              │   │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐  │   │
│  │  │   Browser   │ │  Computer   │ │   Shell/    │ │   Sales     │  │   │
│  │  │   Control   │ │   Control   │ │   Terminal  │ │ Automation  │  │   │
│  │  │   Module    │ │   Module    │ │   Module    │ │   Module    │  │   │
│  │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘  │   │
│  └────────────────────────────────────────────────────────────────────┘   │
│                                    │                                       │
│  ┌─────────────────────────────────┴─────────────────────────────────┐   │
│  │                      SAFETY & SECURITY LAYER                      │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │   │
│  │  │ Permission│ │  Audit   │ │  Human   │ │  Sandbox │ │  Rate    │ │   │
│  │  │  System   │ │  Logger  │ │ Approval │ │  Manager │ │  Limiter │ │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘ │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                       │
│  ┌─────────────────────────────────┴─────────────────────────────────┐   │
│  │                    PLATFORM ABSTRACTION LAYER                     │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │   │
│  │  │  macOS   │ │  Linux   │ │ Windows  │ │   WASM   │ │  Docker  │ │   │
│  │  │  Driver  │ │  Driver  │ │  Driver  │ │  Runtime │ │  Sandbox │ │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘ │   │
│  └───────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Core Modules and Responsibilities

| Module | Responsibility | Current Status |
|--------|---------------|----------------|
| `mcp_host` | MCP client implementation, server discovery, tool registration | NEW |
| `browser_control` | Browser automation via Playwright/CDP, multi-tab management | NEW |
| `computer_control` | Mouse/keyboard simulation, screen capture, window management | NEW |
| `terminal_control` | Shell integration, command execution, PTY management | EXTEND |
| `sales_automation` | Email/LinkedIn workflows, CRM integration, sequence management | NEW |
| `vision_engine` | Screen understanding, OCR, element detection | NEW |
| `safety_framework` | Permission validation, audit logging, human approval | EXTEND |

### 1.3 Communication Patterns

```
┌─────────────────────────────────────────────────────────────────┐
│                    Communication Flow                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   User Request → Agent Loop → Tool Router → Capability Module   │
│        ↑                                              │         │
│        └────────────── Observation ← Result ──────────┘         │
│                                                                  │
│   MCP Communication (JSON-RPC 2.0):                              │
│   ┌─────────┐    initialize     ┌─────────┐                     │
│   │  Client │ ────────────────→ │  Server │                     │
│   │         │ ←──────────────── │         │                     │
│   │         │   capabilities    │         │                     │
│   │         │ ────────────────→ │         │                     │
│   │         │  tools/resources  │         │                     │
│   │         │  ───────────────→ │         │                     │
│   │         │   tool_call       │         │                     │
│   │         │ ←───────────────  │         │                     │
│   │         │   tool_result     │         │                     │
│   └─────────┘                   └─────────┘                     │
│                                                                  │
│   Internal Event Bus (Tokio mpsc channels):                      │
│   - CommandChannel: Agent → Control Modules                      │
│   - ObservationChannel: Control → Agent                          │
│   - SafetyChannel: All → Safety Framework                        │
│   - AuditChannel: All → Audit Logger                             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. MCP (Model Context Protocol) Integration

### 2.1 MCP Architecture Design

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         MCP INTEGRATION ARCHITECTURE                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        MCP HOST (Selfware)                          │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐ │   │
│  │  │   Client    │  │   Client    │  │   Client    │  │   Client   │ │   │
│  │  │  Manager    │  │   (GitHub)  │  │   (Browser) │  │   (Files)  │ │   │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └─────┬──────┘ │   │
│  │         │                │                │               │        │   │
│  │  ┌──────┴────────────────┴────────────────┴───────────────┴──────┐ │   │
│  │  │                    Session Manager                             │ │   │
│  │  │  - Connection pooling    - Capability negotiation              │ │   │
│  │  │  - Protocol versioning   - Health monitoring                   │ │   │
│  │  └──────────────────────────────┬────────────────────────────────┘ │   │
│  │                                 │                                  │   │
│  │  ┌──────────────────────────────┴────────────────────────────────┐ │   │
│  │  │                   Transport Layer                               │ │   │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │ │   │
│  │  │  │  STDIO   │  │ HTTP/SSE │  │WebSocket │  │  Custom  │       │ │   │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘       │ │   │
│  │  └───────────────────────────────────────────────────────────────┘ │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│  ┌─────────────────────────────────┴─────────────────────────────────────┐ │
│  │                         MCP SERVERS                                    │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐    │ │
│  │  │  Files   │ │   Git    │ │  Browser │ │  Search  │ │  Custom  │    │ │
│  │  │  System  │ │  Server  │ │  Server  │ │  Server  │ │  Servers │    │ │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘    │ │
│  └──────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 MCP Server and Client Architecture

#### Client Implementation (Rust)

```rust
// src/mcp/client.rs
pub struct McpClient {
    session: Arc<McpSession>,
    transport: Box<dyn McpTransport>,
    capabilities: ClientCapabilities,
    server_capabilities: Option<ServerCapabilities>,
    tool_registry: ToolRegistry,
}

impl McpClient {
    pub async fn connect(transport: Box<dyn McpTransport>) -> Result<Self, McpError> {
        let client = Self {
            session: Arc::new(McpSession::new()),
            transport,
            capabilities: ClientCapabilities::default(),
            server_capabilities: None,
            tool_registry: ToolRegistry::new(),
        };
        client.initialize().await?;
        Ok(client)
    }
    
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<ToolResult, McpError> {
        let tool = self.tool_registry.get(name)?;
        self.session.execute_tool(tool, args).await
    }
}
```

#### Server Implementation (Rust)

```rust
// src/mcp/server.rs
pub struct McpServer {
    name: String,
    version: String,
    capabilities: ServerCapabilities,
    tools: HashMap<String, Box<dyn McpTool>>,
    resources: HashMap<String, Box<dyn McpResource>>,
    prompts: HashMap<String, Box<dyn McpPrompt>>,
}

#[async_trait]
pub trait McpTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError>;
}
```

### 2.3 Tool Registration and Discovery

```rust
// src/mcp/tool_registry.rs
pub struct ToolRegistry {
    tools: DashMap<String, RegisteredTool>,
    categories: DashMap<String, Vec<String>>,
}

pub struct RegisteredTool {
    pub name: String,
    pub description: String,
    pub schema: ToolSchema,
    pub handler: Arc<dyn ToolHandler>,
    pub source: ToolSource,
    pub permissions: ToolPermissions,
}

impl ToolRegistry {
    pub fn register_builtin<T: ToolHandler>(&mut self, tool: T) -> Result<(), RegistryError> {
        let registered = RegisteredTool {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            schema: tool.schema(),
            handler: Arc::new(tool),
            source: ToolSource::BuiltIn,
            permissions: tool.default_permissions(),
        };
        self.tools.insert(registered.name.clone(), registered);
        Ok(())
    }
    
    pub async fn discover_from_mcp(&mut self, client: &McpClient) -> Result<Vec<String>, McpError> {
        let tools = client.list_tools().await?;
        for tool in tools {
            let registered = RegisteredTool {
                name: tool.name,
                description: tool.description,
                schema: tool.input_schema,
                handler: Arc::new(McpToolAdapter::new(client.clone(), tool.name)),
                source: ToolSource::McpServer(client.server_name()),
                permissions: ToolPermissions::default(),
            };
            self.tools.insert(registered.name.clone(), registered);
        }
        Ok(tools.iter().map(|t| t.name.clone()).collect())
    }
}
```

### 2.4 Context Management Strategies

```rust
// src/mcp/context_manager.rs
pub struct ContextManager {
    working_memory: Arc<RwLock<ContextWindow>>,
    episodic_memory: Arc<RwLock<EpisodicStore>>,
    semantic_memory: Arc<dyn VectorStore>,
    token_budget: TokenBudget,
}

impl ContextManager {
    pub async fn build_tool_context(
        &self,
        tool_name: &str,
        user_query: &str,
    ) -> Result<ToolContext, ContextError> {
        let mut context = ToolContext::new();
        context.add_section(self.working_memory.read().await.to_section());
        
        let relevant_episodes = self.episodic_memory
            .read().await
            .search_similar(tool_name, 5).await?;
        context.add_section(ContextSection::Episodic(relevant_episodes));
        
        if self.requires_knowledge(tool_name) {
            let knowledge = self.semantic_memory.search(user_query, 3).await?;
            context.add_section(ContextSection::Semantic(knowledge));
        }
        
        self.token_budget.compress(&mut context)?;
        Ok(context)
    }
}
```

---

## 3. Browser Control Module

### 3.1 Browser Automation Approach

**Recommendation: Playwright (Primary) + CDP (Fallback)**

| Criteria | Playwright | Puppeteer | Selenium |
|----------|------------|-----------|----------|
| **Performance** | Fastest (4.51s avg) | Fast (4.78s avg) | Moderate (4.59s avg) |
| **Browser Support** | Chromium, Firefox, WebKit | Chromium (best), Firefox (exp) | All major browsers |
| **Language** | JS/TS, Python, .NET, Java | JS/TS only | Multi-language |
| **Parallel Execution** | Built-in workers | Requires custom setup | Selenium Grid |
| **Visual Testing** | Native screenshot comparison | Third-party required | Not native |
| **Rust Bindings** | Via FFI/CLI | Via FFI/CLI | WebDriver protocol |
| **Debugging** | Excellent (Inspector, Trace Viewer) | Good | Basic |

**Decision: Use Playwright via Rust FFI bindings or CLI integration**

### 3.2 Browser Control Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        BROWSER CONTROL MODULE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                      Browser Manager                                 │   │
│  │  - Browser lifecycle management    - Context isolation              │   │
│  │  - Resource cleanup                - Connection pooling             │   │
│  └───────────────────────────────┬─────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                      Page Controller                                 │   │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐   │   │
│  │  │  Navigation │ │   Element   │ │   Action    │ │   Wait/     │   │   │
│  │  │   Engine    │ │   Locator   │ │   Executor  │ │   Assert    │   │   │
│  │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                    Multi-Context Manager                             │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │   │
│  │  │ Context  │ │ Context  │ │ Context  │ │ Context  │ │ Context  │  │   │
│  │  │    1     │ │    2     │ │    3     │ │    4     │ │    N     │  │   │
│  │  │ (Page A) │ │ (Page B) │ │ (Page C) │ │ (Page D) │ │ (Page E) │  │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                   Playwright Integration                             │   │
│  │  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐ │   │
│  │  │  Playwright CLI │ or │  Playwright     │ or │   Chrome Dev    │ │   │
│  │  │  (Process spawn)│    │  Server (WS)    │    │   Protocol      │ │   │
│  │  └─────────────────┘    └─────────────────┘    └─────────────────┘ │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.3 Rust Implementation Structure

```rust
// src/browser/mod.rs
pub mod browser_manager;
pub mod page_controller;
pub mod element_locator;
pub mod action_executor;
pub mod context_manager;
pub mod playwright_bridge;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub headless: bool,
    pub browser_type: BrowserType,
    pub viewport: Viewport,
    pub user_agent: Option<String>,
    pub proxy: Option<ProxyConfig>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct BrowserManager {
    playwright: PlaywrightBridge,
    contexts: HashMap<String, BrowserContext>,
    config: BrowserConfig,
}

impl BrowserManager {
    pub async fn launch(config: BrowserConfig) -> Result<Self, BrowserError> {
        let playwright = PlaywrightBridge::install_and_launch().await?;
        Ok(Self {
            playwright,
            contexts: HashMap::new(),
            config,
        })
    }
}
```

### 3.4 Page Controller Implementation

```rust
// src/browser/page_controller.rs
pub struct PageController {
    page: PlaywrightPage,
    locator: ElementLocator,
    action_executor: ActionExecutor,
    screenshot_manager: ScreenshotManager,
}

impl PageController {
    pub async fn navigate(&self, url: &str) -> Result<NavigationResult, BrowserError> {
        let response = self.page.goto(url).await?;
        self.page.wait_for_load_state("networkidle").await?;
        Ok(NavigationResult {
            url: self.page.url().await?,
            status: response.status(),
            title: self.page.title().await?,
        })
    }
    
    pub async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        let element = self.locator.find(selector).await?;
        element.scroll_into_view().await?;
        self.action_executor.click_with_retry(&element).await
    }
    
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<(), BrowserError> {
        let element = self.locator.find(selector).await?;
        element.fill("").await?;
        self.action_executor.type_human_like(&element, text).await
    }
    
    pub async fn screenshot(&self, options: ScreenshotOptions) -> Result<Screenshot, BrowserError> {
        self.screenshot_manager.capture(&self.page, options).await
    }
}
```

### 3.5 Multi-Tab and Multi-Window Management

```rust
// src/browser/context_manager.rs
pub struct ContextManager {
    contexts: HashMap<String, BrowserContext>,
    active_context: Option<String>,
    event_bus: EventBus,
}

pub struct BrowserContext {
    id: String,
    pages: Vec<PageHandle>,
    active_page: Option<usize>,
    cookies: CookieStore,
    storage_state: StorageState,
}

impl ContextManager {
    pub async fn create_context(&mut self, name: &str) -> Result<&BrowserContext, BrowserError> {
        let context = BrowserContext::new(name).await?;
        self.contexts.insert(name.to_string(), context);
        self.event_bus.publish(ContextEvent::Created(name.to_string()));
        Ok(self.contexts.get(name).unwrap())
    }
    
    pub async fn new_tab(&mut self, context_name: &str, url: Option<&str>) 
        -> Result<PageHandle, BrowserError> {
        let context = self.contexts.get_mut(context_name)
            .ok_or_else(|| BrowserError::ContextNotFound(context_name.to_string()))?;
        let page = context.new_page().await?;
        if let Some(url) = url {
            page.goto(url).await?;
        }
        Ok(page)
    }
}
```

### 3.6 Handling Dynamic Content and SPAs

```rust
// src/browser/dynamic_content.rs
pub struct DynamicContentHandler {
    wait_strategies: Vec<Box<dyn WaitStrategy>>,
}

#[async_trait]
pub trait WaitStrategy: Send + Sync {
    async fn wait(&self, page: &PlaywrightPage) -> Result<(), WaitError>;
}

impl DynamicContentHandler {
    pub async fn wait_for_spa_navigation(&self, page: &PlaywrightPage) -> Result<(), BrowserError> {
        for strategy in &self.wait_strategies {
            if let Err(e) = strategy.wait(page).await {
                log::warn!("Wait strategy failed: {}", e);
            }
        }
        self.wait_for_js_frameworks(page).await
    }
    
    async fn wait_for_js_frameworks(&self, page: &PlaywrightPage) -> Result<(), BrowserError> {
        let js_check = r#"
            () => {
                if (window.__REACT_LOADED__) return true;
                if (window.__VUE__) return true;
                if (window.getAllAngularRootElements) return true;
                return document.readyState === 'complete';
            }
        "#;
        page.wait_for_function(js_check).await?;
        Ok(())
    }
}
```

### 3.7 Screenshot and Visual Understanding

```rust
// src/browser/screenshot.rs
pub struct ScreenshotManager {
    storage: Arc<dyn ScreenshotStorage>,
    vision_client: Option<Arc<dyn VisionClient>>,
}

impl ScreenshotManager {
    pub async fn capture(
        &self,
        page: &PlaywrightPage,
        options: ScreenshotOptions,
    ) -> Result<Screenshot, BrowserError> {
        let bytes = page.screenshot(options.into()).await?;
        let screenshot = Screenshot {
            id: Uuid::new_v4(),
            timestamp: Instant::now(),
            data: bytes,
            format: options.format,
            metadata: self.extract_metadata(page).await?,
        };
        self.storage.store(&screenshot).await?;
        Ok(screenshot)
    }
    
    pub async fn capture_with_analysis(
        &self,
        page: &PlaywrightPage,
        options: ScreenshotOptions,
        query: &str,
    ) -> Result<AnalyzedScreenshot, BrowserError> {
        let screenshot = self.capture(page, options).await?;
        if let Some(vision) = &self.vision_client {
            let analysis = vision.analyze(&screenshot.data, query).await?;
            Ok(AnalyzedScreenshot { screenshot, analysis })
        } else {
            Err(BrowserError::VisionNotConfigured)
        }
    }
}
```
