## 4. Computer Control Module

### 4.1 Mouse Movement and Click Simulation

**Technology Choice: `enigo` crate for Rust**
- Cross-platform (Linux X11, macOS, Windows)
- Active development
- Simple API
- Serde support for serialization

```rust
// src/computer/mouse.rs
use enigo::{Enigo, Mouse, MouseButton, Coordinate};

pub struct MouseController {
    enigo: Enigo,
    movement_profile: MovementProfile,
    click_delay_ms: u64,
}

#[derive(Debug, Clone)]
pub struct MovementProfile {
    pub curve_type: MovementCurve,
    pub speed_variation: f64,
    pub overshoot: f64,
}

#[derive(Debug, Clone)]
pub enum MovementCurve {
    Linear,
    EaseInOut,
    Bezier(Vec<(f64, f64)>),
}

impl MouseController {
    pub fn new() -> Result<Self, ComputerError> {
        Ok(Self {
            enigo: Enigo::new(&enigo::Settings::default())
                .map_err(|e| ComputerError::Initialization(e.to_string()))?,
            movement_profile: MovementProfile::human_like(),
            click_delay_ms: 50,
        })
    }
    
    /// Move mouse to absolute coordinates with human-like motion
    pub async fn move_to(&mut self, x: i32, y: i32) -> Result<(), ComputerError> {
        let current = self.enigo.location()?;
        match self.movement_profile.curve_type {
            MovementCurve::Linear => {
                self.enigo.move_mouse(x, y, Coordinate::Abs)?;
            }
            MovementCurve::EaseInOut => {
                self.move_human_like(current.0, current.1, x, y).await?;
            }
            MovementCurve::Bezier(points) => {
                self.move_bezier(current.0, current.1, x, y, &points).await?;
            }
        }
        Ok(())
    }
    
    /// Human-like mouse movement with Bezier curves
    async fn move_human_like(
        &mut self,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    ) -> Result<(), ComputerError> {
        let distance = ((to_x - from_x).pow(2) + (to_y - from_y).pow(2)) as f64;
        let steps = (distance.sqrt() / 10.0).max(10.0) as i32;
        
        let control_x = (from_x + to_x) / 2 + rand::random::<i32>() % 50 - 25;
        let control_y = (from_y + to_y) / 2 + rand::random::<i32>() % 50 - 25;
        
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = ((1.0 - t).powi(2) * from_x as f64
                + 2.0 * (1.0 - t) * t * control_x as f64
                + t.powi(2) * to_x as f64) as i32;
            let y = ((1.0 - t).powi(2) * from_y as f64
                + 2.0 * (1.0 - t) * t * control_y as f64
                + t.powi(2) * to_y as f64) as i32;
            
            self.enigo.move_mouse(x, y, Coordinate::Abs)?;
            let delay = 5 + rand::random::<u64>() % 10;
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        Ok(())
    }
    
    pub async fn click(&mut self, button: MouseButton) -> Result<(), ComputerError> {
        self.enigo.button(button, enigo::Direction::Click)?;
        tokio::time::sleep(Duration::from_millis(self.click_delay_ms)).await;
        Ok(())
    }
    
    pub async fn click_at(&mut self, x: i32, y: i32, button: MouseButton) 
        -> Result<(), ComputerError> {
        self.move_to(x, y).await?;
        self.click(button).await
    }
    
    pub async fn scroll(&mut self, delta_x: i32, delta_y: i32) -> Result<(), ComputerError> {
        self.enigo.scroll(delta_y, enigo::Axis::Vertical)?;
        if delta_x != 0 {
            self.enigo.scroll(delta_x, enigo::Axis::Horizontal)?;
        }
        Ok(())
    }
    
    pub async fn drag(
        &mut self,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    ) -> Result<(), ComputerError> {
        self.move_to(from_x, from_y).await?;
        self.enigo.button(MouseButton::Left, enigo::Direction::Press)?;
        self.move_human_like(from_x, from_y, to_x, to_y).await?;
        self.enigo.button(MouseButton::Left, enigo::Direction::Release)?;
        Ok(())
    }
}
```

### 4.2 Keyboard Input Simulation

```rust
// src/computer/keyboard.rs
use enigo::{Enigo, Keyboard, Key};

pub struct KeyboardController {
    enigo: Enigo,
    typing_profile: TypingProfile,
}

#[derive(Debug, Clone)]
pub struct TypingProfile {
    pub base_delay_ms: u64,
    pub variation_ms: u64,
    pub mistake_rate: f64,
    pub correction_delay_ms: u64,
}

impl TypingProfile {
    pub fn human_like() -> Self {
        Self {
            base_delay_ms: 80,
            variation_ms: 40,
            mistake_rate: 0.02,
            correction_delay_ms: 200,
        }
    }
    
    pub fn fast() -> Self {
        Self {
            base_delay_ms: 30,
            variation_ms: 10,
            mistake_rate: 0.0,
            correction_delay_ms: 0,
        }
    }
}

impl KeyboardController {
    pub fn new() -> Result<Self, ComputerError> {
        Ok(Self {
            enigo: Enigo::new(&enigo::Settings::default())
                .map_err(|e| ComputerError::Initialization(e.to_string()))?,
            typing_profile: TypingProfile::human_like(),
        })
    }
    
    /// Type text with human-like characteristics
    pub async fn type_text(&mut self, text: &str) -> Result<(), ComputerError> {
        for ch in text.chars() {
            // Occasional typo
            if self.typing_profile.mistake_rate > 0.0 
                && rand::random::<f64>() < self.typing_profile.mistake_rate {
                let wrong_char = self.get_adjacent_key(ch);
                self.enigo.key(Key::Unicode(wrong_char), enigo::Direction::Click)?;
                tokio::time::sleep(Duration::from_millis(
                    self.typing_profile.correction_delay_ms
                )).await;
                self.enigo.key(Key::Backspace, enigo::Direction::Click)?;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            
            self.enigo.key(Key::Unicode(ch), enigo::Direction::Click)?;
            let delay = self.typing_profile.base_delay_ms
                + rand::random::<u64>() % self.typing_profile.variation_ms;
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        Ok(())
    }
    
    /// Press a key combination (e.g., Ctrl+C)
    pub async fn key_combo(&mut self, keys: &[Key]) -> Result<(), ComputerError> {
        for key in keys {
            self.enigo.key(*key, enigo::Direction::Press)?;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        for key in keys.iter().rev() {
            self.enigo.key(*key, enigo::Direction::Release)?;
        }
        Ok(())
    }
}
```

### 4.3 Screen Capture and Analysis

```rust
// src/computer/screen.rs
use xcap::{Monitor, Window};

pub struct ScreenCapture {
    vision_client: Arc<dyn VisionClient>,
    ocr_engine: Arc<dyn OcrEngine>,
}

#[derive(Debug, Clone)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct ScreenAnalysis {
    pub screenshot: Screenshot,
    pub detected_elements: Vec<UiElement>,
    pub extracted_text: Vec<TextRegion>,
    pub applications: Vec<ApplicationWindow>,
}

#[derive(Debug, Clone)]
pub struct UiElement {
    pub element_type: ElementType,
    pub bounds: CaptureRegion,
    pub confidence: f64,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum ElementType {
    Button,
    TextField,
    Checkbox,
    Dropdown,
    Link,
    Image,
    Text,
    Icon,
    Menu,
    Unknown,
}

impl ScreenCapture {
    pub async fn capture_full_screen(&self) -> Result<Screenshot, ComputerError> {
        let monitors = Monitor::all()?;
        let primary = monitors.iter()
            .find(|m| m.is_primary())
            .or_else(|| monitors.first())
            .ok_or(ComputerError::NoDisplay)?;
        
        let image = primary.capture_image()?;
        Ok(Screenshot {
            id: Uuid::new_v4(),
            data: image.to_png()?,
            dimensions: (image.width(), image.height()),
            timestamp: Instant::now(),
        })
    }
    
    pub async fn capture_region(&self, region: CaptureRegion) -> Result<Screenshot, ComputerError> {
        let image = Monitor::all()?[0].capture_image()?;
        let cropped = image.crop(
            region.x as u32,
            region.y as u32,
            region.width,
            region.height,
        );
        Ok(Screenshot {
            id: Uuid::new_v4(),
            data: cropped.to_png()?,
            dimensions: (region.width, region.height),
            timestamp: Instant::now(),
        })
    }
    
    pub async fn analyze_screen(&self) -> Result<ScreenAnalysis, ComputerError> {
        let screenshot = self.capture_full_screen().await?;
        let detected_elements = self.vision_client
            .detect_elements(&screenshot.data).await?;
        let extracted_text = self.ocr_engine
            .extract_text(&screenshot.data).await?;
        let applications = self.list_application_windows().await?;
        
        Ok(ScreenAnalysis {
            screenshot,
            detected_elements,
            extracted_text,
            applications,
        })
    }
}
```

### 4.4 Window Management

```rust
// src/computer/window.rs
pub struct WindowManager {
    platform: Box<dyn WindowPlatform>,
}

#[async_trait]
pub trait WindowPlatform: Send + Sync {
    async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError>;
    async fn focus_window(&self, window_id: WindowId) -> Result<(), WindowError>;
    async fn minimize_window(&self, window_id: WindowId) -> Result<(), WindowError>;
    async fn maximize_window(&self, window_id: WindowId) -> Result<(), WindowError>;
    async fn move_window(&self, window_id: WindowId, x: i32, y: i32) -> Result<(), WindowError>;
    async fn resize_window(&self, window_id: WindowId, width: u32, height: u32) 
        -> Result<(), WindowError>;
    async fn close_window(&self, window_id: WindowId) -> Result<(), WindowError>;
    async fn get_active_window(&self) -> Result<WindowInfo, WindowError>;
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: String,
    pub app_name: String,
    pub bounds: WindowBounds,
    pub is_minimized: bool,
    pub is_maximized: bool,
    pub is_focused: bool,
}

impl WindowManager {
    pub fn new() -> Result<Self, ComputerError> {
        let platform: Box<dyn WindowPlatform> = match std::env::consts::OS {
            "macos" => Box::new(MacWindowPlatform::new()?),
            "linux" => Box::new(LinuxWindowPlatform::new()?),
            "windows" => Box::new(WindowsWindowPlatform::new()?),
            _ => return Err(ComputerError::UnsupportedPlatform),
        };
        Ok(Self { platform })
    }
    
    pub async fn find_window(&self, title_pattern: &str) -> Result<Option<WindowInfo>, WindowError> {
        let windows = self.platform.list_windows().await?;
        Ok(windows.into_iter()
            .find(|w| w.title.contains(title_pattern) || w.app_name.contains(title_pattern)))
    }
    
    pub async fn switch_to_application(&self, app_name: &str) -> Result<(), WindowError> {
        let windows = self.platform.list_windows().await?;
        let app_window = windows.into_iter()
            .find(|w| w.app_name.to_lowercase() == app_name.to_lowercase())
            .ok_or_else(|| WindowError::ApplicationNotFound(app_name.to_string()))?;
        self.platform.focus_window(app_window.id).await
    }
}
```

### 4.5 Application Launching and Control

```rust
// src/computer/application.rs
pub struct ApplicationController {
    process_manager: ProcessManager,
    window_manager: WindowManager,
}

#[derive(Debug, Clone)]
pub struct ApplicationConfig {
    pub name: String,
    pub executable: String,
    pub args: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
    pub timeout_ms: u64,
}

impl ApplicationController {
    pub async fn launch(&self, config: &ApplicationConfig) -> Result<ApplicationInstance, ComputerError> {
        let mut cmd = Command::new(&config.executable);
        cmd.args(&config.args).envs(&config.env_vars);
        if let Some(dir) = &config.working_dir {
            cmd.current_dir(dir);
        }
        
        let child = cmd.spawn()
            .map_err(|e| ComputerError::LaunchFailed(e.to_string()))?;
        let pid = child.id() as i32;
        
        let window = self.wait_for_window(&config.name, config.timeout_ms).await?;
        Ok(ApplicationInstance {
            pid,
            window_id: window.map(|w| w.id),
            name: config.name.clone(),
        })
    }
    
    pub async fn terminate(&self, instance: &ApplicationInstance) -> Result<(), ComputerError> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            
            kill(Pid::from_raw(instance.pid), Signal::SIGTERM)
                .map_err(|e| ComputerError::TerminationFailed(e.to_string()))?;
            
            tokio::time::timeout(Duration::from_secs(5), async {
                while self.process_exists(instance.pid).await {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }).await.ok();
            
            if self.process_exists(instance.pid).await {
                kill(Pid::from_raw(instance.pid), Signal::SIGKILL)
                    .map_err(|e| ComputerError::TerminationFailed(e.to_string()))?;
            }
        }
        Ok(())
    }
}
```

---

## 5. Claude Code-Like Terminal/Shell Integration

### 5.1 Terminal Integration Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    TERMINAL/SHELL INTEGRATION MODULE                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                      Shell Session Manager                           │   │
│  │  - PTY allocation              - Session persistence                 │   │
│  │  - Multi-shell support         - Environment management              │   │
│  └───────────────────────────────┬─────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                      Command Executor                                │   │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐   │   │
│  │  │   Safety    │ │   Process   │ │   Output    │ │   Exit      │   │   │
│  │  │   Checker   │ │   Spawner   │ │   Handler   │ │   Handler   │   │   │
│  │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                    File System Operations                            │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐  │   │
│  │  │   Read   │ │  Write   │ │   Edit   │ │  Search  │ │  Watch   │  │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                  │                                          │
│  ┌───────────────────────────────┴─────────────────────────────────────┐   │
│  │                    Code Editing Engine                               │   │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐   │   │
│  │  │   Diff      │ │   Patch     │ │   Search    │ │   LSP       │   │   │
│  │  │   Viewer    │ │   Applier   │ │   Replace   │ │   Client    │   │   │
│  │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Shell Session Management

```rust
// src/terminal/shell_session.rs
use portable_pty::{PtySize, PtySystem, NativePtySystem};

pub struct ShellSessionManager {
    sessions: HashMap<String, ShellSession>,
    pty_system: Box<dyn PtySystem>,
}

pub struct ShellSession {
    id: String,
    pty: Box<dyn portable_pty::MasterPty>,
    reader: tokio::io::BufReader<Box<dyn std::io::Read + Send>>,
    writer: Box<dyn std::io::Write + Send>,
    current_dir: PathBuf,
    env_vars: HashMap<String, String>,
}

impl ShellSessionManager {
    pub fn new() -> Result<Self, TerminalError> {
        Ok(Self {
            sessions: HashMap::new(),
            pty_system: NativePtySystem::default(),
        })
    }
    
    pub async fn create_session(
        &mut self,
        name: &str,
        shell: Option<&str>,
    ) -> Result<&ShellSession, TerminalError> {
        let shell = shell.unwrap_or_else(|| {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        });
        
        let pair = self.pty_system.openpty(PtySize {
            rows: 24, cols: 80,
            pixel_width: 0, pixel_height: 0,
        })?;
        
        let cmd = CommandBuilder::new(&shell);
        pair.slave.spawn_command(cmd)?;
        
        let reader = tokio::io::BufReader::new(pair.master.try_clone_reader()?);
        let writer = pair.master.take_writer()?;
        
        let session = ShellSession {
            id: name.to_string(),
            pty: pair.master,
            reader, writer,
            current_dir: std::env::current_dir()?,
            env_vars: HashMap::new(),
        };
        
        self.sessions.insert(name.to_string(), session);
        Ok(self.sessions.get(name).unwrap())
    }
    
    pub async fn execute_command(
        &mut self,
        session_name: &str,
        command: &str,
    ) -> Result<CommandOutput, TerminalError> {
        let session = self.sessions.get_mut(session_name)
            .ok_or_else(|| TerminalError::SessionNotFound(session_name.to_string()))?;
        
        session.writer.write_all(command.as_bytes())?;
        session.writer.write_all(b"\n")?;
        session.writer.flush()?;
        
        self.read_output_with_timeout(session, Duration::from_secs(30)).await
    }
}
```

### 5.3 Command Execution with Safety Controls

```rust
// src/terminal/command_executor.rs
pub struct CommandExecutor {
    safety_checker: Arc<SafetyChecker>,
    audit_logger: Arc<AuditLogger>,
    allowed_commands: HashSet<String>,
    blocked_patterns: Vec<Regex>,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub env_vars: HashMap<String, String>,
    pub timeout_ms: u64,
    pub require_approval: ApprovalLevel,
}

#[derive(Debug, Clone, Copy)]
pub enum ApprovalLevel {
    None, Low, Medium, High, Critical,
}

impl CommandExecutor {
    pub async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult, TerminalError> {
        let safety_result = self.safety_checker.check(&request).await?;
        
        if safety_result.is_blocked() {
            self.audit_logger.log_blocked(&request, &safety_result).await;
            return Err(TerminalError::BlockedBySafety(safety_result.reason));
        }
        
        if self.requires_approval(&request, &safety_result) {
            let approved = self.request_human_approval(&request).await?;
            if !approved {
                self.audit_logger.log_rejected(&request).await;
                return Err(TerminalError::RejectedByUser);
            }
        }
        
        self.audit_logger.log_execution_start(&request).await;
        let result = self.spawn_and_monitor(request.clone()).await;
        
        match &result {
            Ok(exec_result) => self.audit_logger.log_execution_success(&request, exec_result).await,
            Err(e) => self.audit_logger.log_execution_failure(&request, e).await,
        }
        
        result
    }
    
    async fn spawn_and_monitor(&self, request: ExecutionRequest) 
        -> Result<ExecutionResult, TerminalError> {
        let mut cmd = Command::new(&request.command);
        cmd.args(&request.args);
        if let Some(dir) = &request.working_dir {
            cmd.current_dir(dir);
        }
        cmd.envs(&request.env_vars);
        
        let output = tokio::time::timeout(
            Duration::from_millis(request.timeout_ms),
            cmd.output()
        ).await.map_err(|_| TerminalError::Timeout)??;
        
        Ok(ExecutionResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            execution_time_ms: 0,
        })
    }
    
    fn requires_approval(&self, request: &ExecutionRequest, safety: &SafetyResult) -> bool {
        match request.require_approval {
            ApprovalLevel::None => false,
            ApprovalLevel::Low => safety.risk_score > 0.2,
            ApprovalLevel::Medium => safety.risk_score > 0.4,
            ApprovalLevel::High => safety.risk_score > 0.6,
            ApprovalLevel::Critical => true,
        }
    }
}
```

### 5.4 File System Operations

```rust
// src/terminal/file_operations.rs
pub struct FileOperations {
    safety_checker: Arc<SafetyChecker>,
    path_validator: PathValidator,
    watcher: FileWatcher,
}

#[derive(Debug, Clone)]
pub struct FileEdit {
    pub path: PathBuf,
    pub old_string: String,
    pub new_string: String,
}

impl FileOperations {
    pub async fn read_file(&self, path: &Path) -> Result<String, TerminalError> {
        self.path_validator.validate(path).await?;
        self.safety_checker.check_file_read(path).await?;
        
        let content = tokio::fs::read_to_string(path).await?;
        if content.len() > 10_000_000 {
            return Err(TerminalError::FileTooLarge);
        }
        Ok(content)
    }
    
    pub async fn write_file(&self, path: &Path, content: &str) -> Result<(), TerminalError> {
        self.path_validator.validate(path).await?;
        self.safety_checker.check_file_write(path).await?;
        
        self.create_backup(path).await?;
        
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, content).await?;
        tokio::fs::rename(&temp_path, path).await?;
        Ok(())
    }
    
    pub async fn edit_file(&self, edit: &FileEdit) -> Result<EditResult, TerminalError> {
        self.path_validator.validate(&edit.path).await?;
        self.safety_checker.check_file_write(&edit.path).await?;
        
        let content = self.read_file(&edit.path).await?;
        if !content.contains(&edit.old_string) {
            return Err(TerminalError::EditNotFound);
        }
        
        let new_content = content.replacen(&edit.old_string, &edit.new_string, 1);
        self.create_backup(&edit.path).await?;
        self.write_file(&edit.path, &new_content).await?;
        
        Ok(EditResult {
            path: edit.path.clone(),
            lines_changed: edit.old_string.lines().count(),
        })
    }
    
    async fn create_backup(&self, path: &Path) -> Result<PathBuf, TerminalError> {
        let backup_dir = std::env::temp_dir().join("selfware_backups");
        tokio::fs::create_dir_all(&backup_dir).await?;
        
        let backup_path = backup_dir.join(format!(
            "{}.{}",
            path.file_name().unwrap().to_string_lossy(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
        ));
        tokio::fs::copy(path, &backup_path).await?;
        Ok(backup_path)
    }
}
```

### 5.5 Code Editing Capabilities

```rust
// src/terminal/code_editor.rs
pub struct CodeEditor {
    file_ops: FileOperations,
    lsp_clients: HashMap<String, Arc<dyn LspClient>>,
}

#[derive(Debug, Clone)]
pub struct CodeChange {
    pub range: TextRange,
    pub new_text: String,
    pub description: String,
}

impl CodeEditor {
    pub async fn view_diff(&self, path: &Path) -> Result<DiffView, TerminalError> {
        let current = self.file_ops.read_file(path).await?;
        let original = self.get_git_original(path).await?;
        let diff = self.generate_diff(&original, &current);
        
        Ok(DiffView {
            path: path.to_path_buf(),
            original, current, diff,
        })
    }
    
    pub async fn apply_changes(
        &self,
        path: &Path,
        changes: &[CodeChange],
    ) -> Result<ApplyResult, TerminalError> {
        let mut content = self.file_ops.read_file(path).await?;
        let lines: Vec<&str> = content.lines().collect();
        
        let mut sorted_changes = changes.to_vec();
        sorted_changes.sort_by(|a, b| b.range.start_line.cmp(&a.range.start_line));
        
        for change in sorted_changes {
            let start_idx = self.line_col_to_index(&lines, change.range.start_line, change.range.start_col);
            let end_idx = self.line_col_to_index(&lines, change.range.end_line, change.range.end_col);
            content.replace_range(start_idx..end_idx, &change.new_text);
        }
        
        self.file_ops.write_file(path, &content).await?;
        Ok(ApplyResult {
            path: path.to_path_buf(),
            changes_applied: changes.len(),
        })
    }
    
    fn generate_diff(&self, original: &str, current: &str) -> Vec<DiffHunk> {
        let diff = similar::TextDiff::from_lines(original, current);
        diff.unified_diff()
            .context_radius(3)
            .iter_hunks()
            .map(|hunk| DiffHunk {
                old_start: hunk.old_range().start,
                old_lines: hunk.old_range().len,
                new_start: hunk.new_range().start,
                new_lines: hunk.new_range().len,
                lines: hunk.iter_changes().map(|c| DiffLine {
                    kind: match c.tag() {
                        similar::ChangeTag::Delete => DiffLineKind::Removed,
                        similar::ChangeTag::Insert => DiffLineKind::Added,
                        similar::ChangeTag::Equal => DiffLineKind::Context,
                    },
                    content: c.value().to_string(),
                }).collect(),
            })
            .collect()
    }
}
```

### 5.6 Human-in-the-Loop Patterns

```rust
// src/terminal/human_in_loop.rs
pub struct HumanInLoop {
    approval_queue: mpsc::Sender<ApprovalRequest>,
    notification_service: Arc<dyn NotificationService>,
    timeout_config: TimeoutConfig,
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub request_type: ApprovalType,
    pub description: String,
    pub details: Value,
    pub requested_at: Instant,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum ApprovalType {
    CommandExecution { command: String, args: Vec<String> },
    FileWrite { path: PathBuf, size: usize },
    FileDelete { path: PathBuf },
    NetworkRequest { url: String, method: String },
    SystemChange { description: String },
    MultipleChanges { count: usize },
}

impl HumanInLoop {
    pub async fn request_approval(&self, request: ApprovalRequest) 
        -> Result<ApprovalResponse, HitlError> {
        self.notification_service.notify(&request).await;
        
        match tokio::time::timeout(request.timeout, self.wait_for_response(request.id)).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(e)) => Err(e),
            Err(_) => Ok(ApprovalResponse {
                request_id: request.id,
                decision: ApprovalDecision::Timeout,
                reason: Some("Approval timeout".to_string()),
            }),
        }
    }
    
    pub async fn confirm(&self, message: &str, default: bool) -> Result<bool, HitlError> {
        self.tui_confirm(message, default).await
    }
    
    pub async fn prompt(&self, message: &str, default: Option<&str>) 
        -> Result<String, HitlError> {
        self.tui_prompt(message, default).await
    }
}
```
