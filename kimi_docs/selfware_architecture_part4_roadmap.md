## 8. Implementation Roadmap

### 8.1 Phased Development Approach

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        IMPLEMENTATION ROADMAP                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  PHASE 1: FOUNDATION (Weeks 1-4)                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  [x] MCP Client Core Implementation                                  │   │
│  │  [x] Basic Tool Registry and Discovery                               │   │
│  │  [x] Safety Framework Foundation                                     │   │
│  │  [x] Audit Logging System                                            │   │
│  │  [x] Permission System v1                                            │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  PHASE 2: BROWSER CONTROL (Weeks 5-8)                                       │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  [ ] Playwright Integration                                          │   │
│  │  [ ] Page Controller Implementation                                  │   │
│  │  [ ] Multi-Context Management                                        │   │
│  │  [ ] Screenshot and Visual Analysis                                  │   │
│  │  [ ] SPA and Dynamic Content Handling                                │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  PHASE 3: COMPUTER CONTROL (Weeks 9-12)                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  [ ] Mouse Control (enigo integration)                               │   │
│  │  [ ] Keyboard Control                                                │   │
│  │  [ ] Screen Capture (xcap/scrap)                                     │   │
│  │  [ ] Window Management                                               │   │
│  │  [ ] Application Controller                                          │   │
│  │  [ ] Vision Engine Integration                                       │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  PHASE 4: TERMINAL ENHANCEMENT (Weeks 13-15)                                │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  [ ] PTY-based Shell Sessions                                        │   │
│  │  [ ] Command Safety Framework                                        │   │
│  │  [ ] Enhanced File Operations                                        │   │
│  │  [ ] Code Editing Engine                                             │   │
│  │  [ ] Human-in-the-Loop Patterns                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  PHASE 5: SALES AUTOMATION (Weeks 16-20)                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  [ ] Email Automation                                                │   │
│  │  [ ] LinkedIn Automation                                             │   │
│  │  [ ] CRM Integration (HubSpot, Salesforce)                           │   │
│  │  [ ] Follow-Up Sequences                                             │   │
│  │  [ ] Compliance Engine                                               │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  PHASE 6: ADVANCED FEATURES (Weeks 21-24)                                   │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  [ ] MCP Server Implementation                                       │   │
│  │  [ ] Advanced Safety Features                                        │   │
│  │  [ ] Performance Optimization                                        │   │
│  │  [ ] Comprehensive Testing                                           │   │
│  │  [ ] Documentation and Examples                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 8.2 Priority Features for MVP

| Priority | Feature | Description | Est. Effort |
|----------|---------|-------------|-------------|
| P0 | MCP Client | Core MCP protocol implementation with stdio transport | 1 week |
| P0 | Tool Registry | Dynamic tool registration and discovery | 3 days |
| P0 | Safety Framework | Basic permission and audit systems | 1 week |
| P0 | Browser Launch | Playwright integration with basic navigation | 1 week |
| P1 | Element Interaction | Click, type, screenshot capabilities | 5 days |
| P1 | Mouse Control | Human-like mouse movement and clicking | 3 days |
| P1 | Keyboard Control | Text input and key combinations | 3 days |
| P1 | Screen Capture | Full screen and region capture | 3 days |
| P2 | Multi-tab Support | Context and tab management | 5 days |
| P2 | Window Management | Focus, minimize, application switching | 3 days |
| P2 | Shell Sessions | PTY-based interactive shells | 5 days |
| P3 | Email Automation | SMTP-based email sending | 1 week |
| P3 | LinkedIn Automation | Browser-based LinkedIn actions | 1 week |

### 8.3 Technology Stack Recommendations

| Component | Technology | Alternative | Rationale |
|-----------|------------|-------------|-----------|
| **Core Framework** | Rust + Tokio | - | Existing Selfware stack |
| **MCP Transport** | JSON-RPC 2.0 | - | Protocol standard |
| **Browser Control** | Playwright | Puppeteer | Best performance, multi-browser |
| **Mouse/Keyboard** | enigo | rdev | Cross-platform, simple API |
| **Screen Capture** | xcap | scrap | Cross-platform, Rust-native |
| **Window Management** | Platform APIs | - | Native performance |
| **PTY/Terminal** | portable-pty | - | Cross-platform PTY |
| **Vision/OCR** | Tesseract | EasyOCR | Open source, proven |
| **Email** | lettre | - | Rust email library |
| **HTTP Client** | reqwest | - | Standard for Rust |
| **Serialization** | serde | - | Standard for Rust |
| **Database** | SQLite | PostgreSQL | Embedded, zero-config |
| **TUI** | ratatui | - | Existing in Selfware |

---

## 9. Integration with Existing Selfware

### 9.1 Module Integration Points

```rust
// src/lib.rs - Extended module structure
pub mod mcp {
    pub mod client;
    pub mod server;
    pub mod transport;
    pub mod tools;
    pub mod resources;
    pub mod prompts;
}

pub mod browser {
    pub mod manager;
    pub mod page;
    pub mod locator;
    pub mod actions;
    pub mod screenshot;
    pub mod playwright;
}

pub mod computer {
    pub mod mouse;
    pub mod keyboard;
    pub mod screen;
    pub mod window;
    pub mod application;
}

pub mod terminal {
    pub mod shell;
    pub mod executor;
    pub mod file_ops;
    pub mod code_editor;
    pub mod hitl;
}

pub mod sales {
    pub mod email;
    pub mod linkedin;
    pub mod crm;
    pub mod sequences;
    pub mod compliance;
}

pub mod vision {
    pub mod detector;
    pub mod ocr;
    pub mod analyzer;
}

// Extend existing tools module
pub mod tools {
    // ... existing tools ...
    
    // New computer control tools
    pub mod browser_tool;
    pub mod mouse_tool;
    pub mod keyboard_tool;
    pub mod screen_tool;
    pub mod shell_tool;
    pub mod email_tool;
}
```

### 9.2 Tool Registration Integration

```rust
// src/tools/mod.rs - Extended tool registration
impl ToolRegistry {
    pub fn register_computer_control_tools(&mut self) -> Result<(), RegistryError> {
        // Browser tools
        self.register(Box::new(BrowserNavigateTool))?;
        self.register(Box::new(BrowserClickTool))?;
        self.register(Box::new(BrowserTypeTool))?;
        self.register(Box::new(BrowserScreenshotTool))?;
        self.register(Box::new(BrowserExtractTool))?;
        
        // Computer control tools
        self.register(Box::new(MouseMoveTool))?;
        self.register(Box::new(MouseClickTool))?;
        self.register(Box::new(MouseScrollTool))?;
        self.register(Box::new(KeyboardTypeTool))?;
        self.register(Box::new(KeyboardPressTool))?;
        self.register(Box::new(ScreenCaptureTool))?;
        self.register(Box::new(WindowFocusTool))?;
        self.register(Box::new(AppLaunchTool))?;
        
        // Terminal tools
        self.register(Box::new(ShellExecuteTool))?;
        self.register(Box::new(FileReadTool))?;
        self.register(Box::new(FileWriteTool))?;
        self.register(Box::new(FileEditTool))?;
        self.register(Box::new(FileSearchTool))?;
        
        // Sales automation tools
        self.register(Box::new(EmailSendTool))?;
        self.register(Box::new(LinkedInConnectTool))?;
        self.register(Box::new(LinkedInMessageTool))?;
        self.register(Box::new(CrmSyncTool))?;
        
        Ok(())
    }
}
```

---

## 10. Summary

This architectural plan provides a comprehensive roadmap for adding computer control capabilities to Selfware. The key architectural decisions are:

### Key Architectural Decisions

1. **MCP Integration**: Three-layer architecture (Client/Server, Session, Transport) with JSON-RPC 2.0 communication
2. **Browser Control**: Playwright as primary automation tool with multi-context support
3. **Computer Control**: enigo for mouse/keyboard, xcap for screen capture, platform APIs for window management
4. **Terminal Integration**: PTY-based shell sessions with comprehensive safety controls
5. **Sales Automation**: Modular design with CRM connectors, sequence engine, and compliance framework
6. **Safety Framework**: Multi-layer permission system, audit logging, human approval workflows

### Technology Choices Summary

| Layer | Primary Technology | Rationale |
|-------|-------------------|-----------|
| MCP Protocol | JSON-RPC 2.0 over stdio/HTTP | Protocol standard, widely adopted |
| Browser | Playwright | Best performance, multi-browser, active development |
| Mouse/Keyboard | enigo | Cross-platform, Rust-native, simple API |
| Screen Capture | xcap | Cross-platform, Rust-native |
| Terminal | portable-pty | Cross-platform PTY support |
| Vision/OCR | Tesseract | Open source, proven accuracy |

### Implementation Timeline

- **Phase 1 (Weeks 1-4)**: Foundation - MCP, Safety, Tool Registry
- **Phase 2 (Weeks 5-8)**: Browser Control - Playwright integration
- **Phase 3 (Weeks 9-12)**: Computer Control - Mouse, keyboard, screen
- **Phase 4 (Weeks 13-15)**: Terminal Enhancement - PTY, file ops, code editing
- **Phase 5 (Weeks 16-20)**: Sales Automation - Email, LinkedIn, CRM
- **Phase 6 (Weeks 21-24)**: Advanced Features - MCP Server, optimization

### Risk Mitigation

1. **Platform Compatibility**: Abstract platform-specific code behind traits
2. **Safety**: Multi-layer permission system with human approval
3. **Rate Limiting**: Built-in rate limiting for all external operations
4. **Audit Logging**: Comprehensive audit trail for all actions
5. **Error Recovery**: Graceful degradation and retry mechanisms

### Success Metrics

- **MVP (Week 8)**: Basic browser automation, mouse/keyboard control
- **Beta (Week 15)**: Full terminal integration, file operations
- **Release (Week 24)**: Complete sales automation, advanced safety features

---

## Appendix A: Additional Resources

### Recommended Crates

```toml
[dependencies]
# MCP / JSON-RPC
jsonrpc-core = "18.0"
serde_json = "1.0"

# Browser automation
playwright = "0.1"  # or custom FFI bindings
reqwest = "0.11"

# Computer control
enigo = "0.2"
xcap = "0.0"

# Terminal
portable-pty = "0.8"
tokio-process = "0.2"

# Vision/OCR
tesseract = "0.15"
image = "0.24"

# Email
lettre = "0.11"

# Safety/Security
regex = "1.10"
dashmap = "5.5"

# Async runtime
tokio = { version = "1.35", features = ["full"] }
```

### External Dependencies

- **Playwright**: Node.js-based browser automation
- **Tesseract OCR**: Open source OCR engine
- **Docker**: Optional sandboxing

### Testing Strategy

1. **Unit Tests**: All core modules
2. **Integration Tests**: Browser automation, computer control
3. **E2E Tests**: Full workflows
4. **Security Tests**: Penetration testing, fuzzing
5. **Performance Tests**: Load testing, benchmark suite

---

*Document generated for Selfware Computer Control Architecture Planning*
*Based on research of MCP protocol, Playwright, enigo, and Claude Code architecture*
*Total estimated implementation time: 24 weeks*
