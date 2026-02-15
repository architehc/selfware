//! Embedded & Hardware Development Tools
//!
//! Provides microcontroller memory analysis, RTOS task scheduling,
//! hardware-in-the-loop testing, and bitfield/register manipulation helpers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TASK_COUNTER: AtomicU64 = AtomicU64::new(1);
static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// Memory Analysis
// ============================================================================

/// Memory region type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryRegion {
    Flash,
    Ram,
    Eeprom,
    ExternalRam,
    Ccm,  // Core-Coupled Memory
    Dtcm, // Data Tightly Coupled Memory
    Itcm, // Instruction Tightly Coupled Memory
}

impl MemoryRegion {
    pub fn is_volatile(&self) -> bool {
        matches!(
            self,
            MemoryRegion::Ram
                | MemoryRegion::ExternalRam
                | MemoryRegion::Ccm
                | MemoryRegion::Dtcm
                | MemoryRegion::Itcm
        )
    }

    pub fn is_executable(&self) -> bool {
        matches!(self, MemoryRegion::Flash | MemoryRegion::Itcm)
    }
}

/// Memory section (from linker script)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySection {
    /// Section name (.text, .data, .bss, etc.)
    pub name: String,
    /// Memory region
    pub region: MemoryRegion,
    /// Start address
    pub start_address: u32,
    /// Size in bytes
    pub size: u32,
    /// Used bytes
    pub used: u32,
}

impl MemorySection {
    pub fn new(
        name: impl Into<String>,
        region: MemoryRegion,
        start_address: u32,
        size: u32,
    ) -> Self {
        Self {
            name: name.into(),
            region,
            start_address,
            size,
            used: 0,
        }
    }

    pub fn usage_percent(&self) -> f64 {
        if self.size == 0 {
            0.0
        } else {
            (self.used as f64 / self.size as f64) * 100.0
        }
    }

    pub fn available(&self) -> u32 {
        self.size.saturating_sub(self.used)
    }
}

/// Memory budget for a component/module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBudget {
    /// Component name
    pub component: String,
    /// Flash budget in bytes
    pub flash_budget: u32,
    /// RAM budget in bytes
    pub ram_budget: u32,
    /// Actual flash usage
    pub flash_used: u32,
    /// Actual RAM usage
    pub ram_used: u32,
}

impl MemoryBudget {
    pub fn new(component: impl Into<String>, flash_budget: u32, ram_budget: u32) -> Self {
        Self {
            component: component.into(),
            flash_budget,
            ram_budget,
            flash_used: 0,
            ram_used: 0,
        }
    }

    pub fn update_usage(&mut self, flash_used: u32, ram_used: u32) {
        self.flash_used = flash_used;
        self.ram_used = ram_used;
    }

    pub fn is_over_budget(&self) -> bool {
        self.flash_used > self.flash_budget || self.ram_used > self.ram_budget
    }

    pub fn flash_margin(&self) -> i32 {
        self.flash_budget as i32 - self.flash_used as i32
    }

    pub fn ram_margin(&self) -> i32 {
        self.ram_budget as i32 - self.ram_used as i32
    }
}

/// Symbol size info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Symbol name
    pub name: String,
    /// Size in bytes
    pub size: u32,
    /// Memory region
    pub region: MemoryRegion,
    /// Section name
    pub section: String,
    /// Source file
    pub source_file: Option<String>,
}

/// Memory analyzer for embedded systems
#[derive(Debug, Clone)]
pub struct MemoryAnalyzer {
    /// Target device name
    pub device: String,
    /// Memory sections
    pub sections: Vec<MemorySection>,
    /// Component budgets
    pub budgets: HashMap<String, MemoryBudget>,
    /// Symbol information
    pub symbols: Vec<SymbolInfo>,
}

impl MemoryAnalyzer {
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            sections: Vec::new(),
            budgets: HashMap::new(),
            symbols: Vec::new(),
        }
    }

    pub fn add_section(&mut self, section: MemorySection) {
        self.sections.push(section);
    }

    pub fn add_budget(&mut self, budget: MemoryBudget) {
        self.budgets.insert(budget.component.clone(), budget);
    }

    pub fn add_symbol(&mut self, symbol: SymbolInfo) {
        self.symbols.push(symbol);
    }

    pub fn total_flash_used(&self) -> u32 {
        self.sections
            .iter()
            .filter(|s| s.region == MemoryRegion::Flash)
            .map(|s| s.used)
            .sum()
    }

    pub fn total_ram_used(&self) -> u32 {
        self.sections
            .iter()
            .filter(|s| s.region == MemoryRegion::Ram)
            .map(|s| s.used)
            .sum()
    }

    pub fn total_flash_available(&self) -> u32 {
        self.sections
            .iter()
            .filter(|s| s.region == MemoryRegion::Flash)
            .map(|s| s.size)
            .sum()
    }

    pub fn total_ram_available(&self) -> u32 {
        self.sections
            .iter()
            .filter(|s| s.region == MemoryRegion::Ram)
            .map(|s| s.size)
            .sum()
    }

    pub fn flash_usage_percent(&self) -> f64 {
        let total = self.total_flash_available();
        if total == 0 {
            0.0
        } else {
            (self.total_flash_used() as f64 / total as f64) * 100.0
        }
    }

    pub fn ram_usage_percent(&self) -> f64 {
        let total = self.total_ram_available();
        if total == 0 {
            0.0
        } else {
            (self.total_ram_used() as f64 / total as f64) * 100.0
        }
    }

    pub fn largest_symbols(&self, n: usize) -> Vec<&SymbolInfo> {
        let mut sorted: Vec<_> = self.symbols.iter().collect();
        sorted.sort_by(|a, b| b.size.cmp(&a.size));
        sorted.truncate(n);
        sorted
    }

    pub fn over_budget_components(&self) -> Vec<&MemoryBudget> {
        self.budgets
            .values()
            .filter(|b| b.is_over_budget())
            .collect()
    }

    pub fn generate_map_report(&self) -> String {
        let mut report = format!("Memory Map for {}\n", self.device);
        report.push_str(&"=".repeat(50));
        report.push('\n');

        report.push_str(&format!(
            "Flash: {}/{} bytes ({:.1}%)\n",
            self.total_flash_used(),
            self.total_flash_available(),
            self.flash_usage_percent()
        ));

        report.push_str(&format!(
            "RAM: {}/{} bytes ({:.1}%)\n",
            self.total_ram_used(),
            self.total_ram_available(),
            self.ram_usage_percent()
        ));

        report.push_str("\nSections:\n");
        for section in &self.sections {
            report.push_str(&format!(
                "  {:16} {:?}: {}/{} ({:.1}%)\n",
                section.name,
                section.region,
                section.used,
                section.size,
                section.usage_percent()
            ));
        }

        report
    }
}

// ============================================================================
// RTOS Task Scheduling
// ============================================================================

/// RTOS type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RtosType {
    FreeRTOS,
    Zephyr,
    ThreadX,
    RTLinux,
    VxWorks,
    NuttX,
    Custom,
}

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Suspended,
    Deleted,
}

/// RTOS task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtosTask {
    /// Task ID
    pub task_id: String,
    /// Task name
    pub name: String,
    /// Priority (higher = more priority)
    pub priority: u8,
    /// Stack size in bytes
    pub stack_size: u32,
    /// Current stack usage
    pub stack_used: u32,
    /// Current state
    pub state: TaskState,
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Execution count
    pub execution_count: u64,
    /// Total execution time (microseconds)
    pub total_execution_us: u64,
    /// Worst-case execution time (microseconds)
    pub wcet_us: u64,
}

impl RtosTask {
    pub fn new(name: impl Into<String>, priority: u8, stack_size: u32) -> Self {
        let task_id = format!("task_{}", TASK_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            task_id,
            name: name.into(),
            priority,
            stack_size,
            stack_used: 0,
            state: TaskState::Ready,
            cpu_usage: 0.0,
            execution_count: 0,
            total_execution_us: 0,
            wcet_us: 0,
        }
    }

    pub fn stack_usage_percent(&self) -> f64 {
        if self.stack_size == 0 {
            0.0
        } else {
            (self.stack_used as f64 / self.stack_size as f64) * 100.0
        }
    }

    pub fn average_execution_us(&self) -> f64 {
        if self.execution_count == 0 {
            0.0
        } else {
            self.total_execution_us as f64 / self.execution_count as f64
        }
    }

    pub fn stack_headroom(&self) -> u32 {
        self.stack_size.saturating_sub(self.stack_used)
    }
}

/// Scheduling event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEvent {
    /// Timestamp (microseconds from start)
    pub timestamp_us: u64,
    /// Task ID
    pub task_id: String,
    /// Event type
    pub event: ScheduleEventType,
    /// Duration (for execution events)
    pub duration_us: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleEventType {
    Start,
    Stop,
    Preempt,
    Block,
    Resume,
    Idle,
}

/// RTOS scheduler analyzer
#[derive(Debug, Clone)]
pub struct SchedulerAnalyzer {
    /// RTOS type
    pub rtos: RtosType,
    /// Tasks
    pub tasks: HashMap<String, RtosTask>,
    /// Schedule trace
    pub trace: Vec<ScheduleEvent>,
    /// Analysis period (microseconds)
    pub analysis_period_us: u64,
}

impl SchedulerAnalyzer {
    pub fn new(rtos: RtosType) -> Self {
        Self {
            rtos,
            tasks: HashMap::new(),
            trace: Vec::new(),
            analysis_period_us: 0,
        }
    }

    pub fn add_task(&mut self, task: RtosTask) {
        self.tasks.insert(task.task_id.clone(), task);
    }

    pub fn record_event(
        &mut self,
        task_id: &str,
        event: ScheduleEventType,
        timestamp_us: u64,
        duration_us: Option<u64>,
    ) {
        self.trace.push(ScheduleEvent {
            timestamp_us,
            task_id: task_id.to_string(),
            event,
            duration_us,
        });

        if timestamp_us > self.analysis_period_us {
            self.analysis_period_us = timestamp_us;
        }

        // Update task stats
        if let Some(task) = self.tasks.get_mut(task_id) {
            if let Some(dur) = duration_us {
                task.execution_count += 1;
                task.total_execution_us += dur;
                if dur > task.wcet_us {
                    task.wcet_us = dur;
                }
            }
        }
    }

    pub fn calculate_cpu_usage(&mut self) {
        if self.analysis_period_us == 0 {
            return;
        }

        for task in self.tasks.values_mut() {
            task.cpu_usage =
                (task.total_execution_us as f64 / self.analysis_period_us as f64) * 100.0;
        }
    }

    pub fn total_cpu_usage(&self) -> f64 {
        self.tasks.values().map(|t| t.cpu_usage).sum()
    }

    pub fn idle_percentage(&self) -> f64 {
        100.0 - self.total_cpu_usage().min(100.0)
    }

    pub fn highest_priority_task(&self) -> Option<&RtosTask> {
        self.tasks.values().max_by_key(|t| t.priority)
    }

    pub fn tasks_by_cpu_usage(&self) -> Vec<&RtosTask> {
        let mut tasks: Vec<_> = self.tasks.values().collect();
        tasks.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
        tasks
    }

    pub fn potential_stack_overflows(&self) -> Vec<&RtosTask> {
        self.tasks
            .values()
            .filter(|t| t.stack_usage_percent() > 80.0)
            .collect()
    }

    pub fn priority_inversion_risks(&self) -> Vec<(&RtosTask, &RtosTask)> {
        let mut risks = Vec::new();
        let tasks: Vec<_> = self.tasks.values().collect();

        for i in 0..tasks.len() {
            for j in (i + 1)..tasks.len() {
                let (high, low) = if tasks[i].priority > tasks[j].priority {
                    (tasks[i], tasks[j])
                } else {
                    (tasks[j], tasks[i])
                };

                // If low priority task has higher CPU usage, potential issue
                if low.cpu_usage > high.cpu_usage * 2.0 && low.cpu_usage > 10.0 {
                    risks.push((high, low));
                }
            }
        }

        risks
    }

    pub fn generate_gantt_data(&self) -> Vec<(String, u64, u64)> {
        let mut gantt = Vec::new();
        let mut current_start: HashMap<String, u64> = HashMap::new();

        for event in &self.trace {
            match event.event {
                ScheduleEventType::Start | ScheduleEventType::Resume => {
                    current_start.insert(event.task_id.clone(), event.timestamp_us);
                }
                ScheduleEventType::Stop | ScheduleEventType::Block | ScheduleEventType::Preempt => {
                    if let Some(start) = current_start.remove(&event.task_id) {
                        gantt.push((event.task_id.clone(), start, event.timestamp_us));
                    }
                }
                ScheduleEventType::Idle => {}
            }
        }

        gantt
    }
}

// ============================================================================
// Hardware-in-the-Loop Testing
// ============================================================================

/// HIL test result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HilTestResult {
    Pass,
    Fail,
    Timeout,
    Error,
    Skipped,
}

/// HIL signal type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalType {
    Digital,
    Analog,
    Pwm,
    Serial,
    I2c,
    Spi,
    Can,
}

/// HIL test signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HilSignal {
    /// Signal name
    pub name: String,
    /// Signal type
    pub signal_type: SignalType,
    /// Pin/channel assignment
    pub pin: String,
    /// Direction (input/output)
    pub is_output: bool,
    /// Current value
    pub value: f64,
    /// Expected value (for assertions)
    pub expected: Option<f64>,
    /// Tolerance for comparison
    pub tolerance: f64,
}

impl HilSignal {
    pub fn digital_output(name: impl Into<String>, pin: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            signal_type: SignalType::Digital,
            pin: pin.into(),
            is_output: true,
            value: 0.0,
            expected: None,
            tolerance: 0.0,
        }
    }

    pub fn analog_input(name: impl Into<String>, pin: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            signal_type: SignalType::Analog,
            pin: pin.into(),
            is_output: false,
            value: 0.0,
            expected: None,
            tolerance: 0.1,
        }
    }

    pub fn set_value(&mut self, value: f64) {
        self.value = value;
    }

    pub fn expect(&mut self, expected: f64) {
        self.expected = Some(expected);
    }

    pub fn matches_expected(&self) -> bool {
        match self.expected {
            Some(exp) => (self.value - exp).abs() <= self.tolerance,
            None => true,
        }
    }
}

/// HIL test step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HilTestStep {
    /// Step description
    pub description: String,
    /// Signals to set
    pub set_signals: Vec<(String, f64)>,
    /// Signals to check
    pub check_signals: Vec<(String, f64, f64)>, // (name, expected, tolerance)
    /// Wait time before check (milliseconds)
    pub wait_ms: u64,
}

impl HilTestStep {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            set_signals: Vec::new(),
            check_signals: Vec::new(),
            wait_ms: 0,
        }
    }

    pub fn set(mut self, signal: impl Into<String>, value: f64) -> Self {
        self.set_signals.push((signal.into(), value));
        self
    }

    pub fn expect(mut self, signal: impl Into<String>, expected: f64, tolerance: f64) -> Self {
        self.check_signals
            .push((signal.into(), expected, tolerance));
        self
    }

    pub fn wait(mut self, ms: u64) -> Self {
        self.wait_ms = ms;
        self
    }
}

/// HIL test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HilTestCase {
    /// Test ID
    pub test_id: String,
    /// Test name
    pub name: String,
    /// Description
    pub description: String,
    /// Test steps
    pub steps: Vec<HilTestStep>,
    /// Result
    pub result: Option<HilTestResult>,
    /// Failure message
    pub failure_message: Option<String>,
    /// Execution time (milliseconds)
    pub execution_time_ms: Option<u64>,
}

impl HilTestCase {
    pub fn new(name: impl Into<String>) -> Self {
        let test_id = format!("hil_{}", TEST_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            test_id,
            name: name.into(),
            description: String::new(),
            steps: Vec::new(),
            result: None,
            failure_message: None,
            execution_time_ms: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn add_step(&mut self, step: HilTestStep) {
        self.steps.push(step);
    }

    pub fn set_result(&mut self, result: HilTestResult, execution_time_ms: u64) {
        self.result = Some(result);
        self.execution_time_ms = Some(execution_time_ms);
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.result = Some(HilTestResult::Fail);
        self.failure_message = Some(message.into());
    }

    pub fn passed(&self) -> bool {
        self.result == Some(HilTestResult::Pass)
    }
}

/// HIL test runner
#[derive(Debug, Clone)]
pub struct HilTestRunner {
    /// Test device name
    pub device: String,
    /// Signals
    pub signals: HashMap<String, HilSignal>,
    /// Test cases
    pub tests: Vec<HilTestCase>,
    /// Connection status
    pub connected: bool,
}

impl HilTestRunner {
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            signals: HashMap::new(),
            tests: Vec::new(),
            connected: false,
        }
    }

    pub fn add_signal(&mut self, signal: HilSignal) {
        self.signals.insert(signal.name.clone(), signal);
    }

    pub fn add_test(&mut self, test: HilTestCase) {
        self.tests.push(test);
    }

    pub fn connect(&mut self) -> Result<(), String> {
        // Simulate connection
        self.connected = true;
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
    }

    pub fn set_signal(&mut self, name: &str, value: f64) -> Result<(), String> {
        if !self.connected {
            return Err("Not connected".to_string());
        }
        let signal = self.signals.get_mut(name).ok_or("Signal not found")?;
        signal.set_value(value);
        Ok(())
    }

    pub fn read_signal(&self, name: &str) -> Result<f64, String> {
        if !self.connected {
            return Err("Not connected".to_string());
        }
        let signal = self.signals.get(name).ok_or("Signal not found")?;
        Ok(signal.value)
    }

    pub fn run_test(&mut self, test_index: usize) -> Result<HilTestResult, String> {
        if !self.connected {
            return Err("Not connected".to_string());
        }

        let test = self.tests.get_mut(test_index).ok_or("Test not found")?;
        let start = std::time::Instant::now();

        for step in &test.steps {
            // Set signals
            for (name, value) in &step.set_signals {
                if let Some(signal) = self.signals.get_mut(name) {
                    signal.set_value(*value);
                }
            }

            // Wait
            std::thread::sleep(Duration::from_millis(step.wait_ms));

            // Check signals
            for (name, expected, tolerance) in &step.check_signals {
                if let Some(signal) = self.signals.get(name) {
                    if (signal.value - expected).abs() > *tolerance {
                        test.fail(format!(
                            "Signal {} expected {} (±{}) but got {}",
                            name, expected, tolerance, signal.value
                        ));
                        return Ok(HilTestResult::Fail);
                    }
                }
            }
        }

        test.set_result(HilTestResult::Pass, start.elapsed().as_millis() as u64);
        Ok(HilTestResult::Pass)
    }

    pub fn passed_count(&self) -> usize {
        self.tests.iter().filter(|t| t.passed()).count()
    }

    pub fn failed_tests(&self) -> Vec<&HilTestCase> {
        self.tests
            .iter()
            .filter(|t| t.result == Some(HilTestResult::Fail))
            .collect()
    }
}

// ============================================================================
// Bitfield & Register Manipulation
// ============================================================================

/// Register field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterField {
    /// Field name
    pub name: String,
    /// Bit offset
    pub offset: u8,
    /// Bit width
    pub width: u8,
    /// Description
    pub description: String,
    /// Enumerated values (if any)
    pub enum_values: HashMap<u32, String>,
}

impl RegisterField {
    pub fn new(name: impl Into<String>, offset: u8, width: u8) -> Self {
        Self {
            name: name.into(),
            offset,
            width,
            description: String::new(),
            enum_values: HashMap::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_enum_value(mut self, value: u32, name: impl Into<String>) -> Self {
        self.enum_values.insert(value, name.into());
        self
    }

    pub fn mask(&self) -> u32 {
        ((1u32 << self.width) - 1) << self.offset
    }

    pub fn extract(&self, register_value: u32) -> u32 {
        (register_value >> self.offset) & ((1 << self.width) - 1)
    }

    pub fn insert(&self, register_value: u32, field_value: u32) -> u32 {
        let mask = self.mask();
        (register_value & !mask) | ((field_value << self.offset) & mask)
    }

    pub fn value_name(&self, value: u32) -> Option<&String> {
        self.enum_values.get(&value)
    }
}

/// Hardware register definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Register {
    /// Register name
    pub name: String,
    /// Address
    pub address: u32,
    /// Width in bits (8, 16, 32)
    pub width: u8,
    /// Fields
    pub fields: Vec<RegisterField>,
    /// Reset value
    pub reset_value: u32,
    /// Description
    pub description: String,
    /// Access mode
    pub access: RegisterAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegisterAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
    ReadWriteOnce,
}

impl Register {
    pub fn new(name: impl Into<String>, address: u32, width: u8) -> Self {
        Self {
            name: name.into(),
            address,
            width,
            fields: Vec::new(),
            reset_value: 0,
            description: String::new(),
            access: RegisterAccess::ReadWrite,
        }
    }

    pub fn with_field(mut self, field: RegisterField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn with_reset_value(mut self, value: u32) -> Self {
        self.reset_value = value;
        self
    }

    pub fn with_access(mut self, access: RegisterAccess) -> Self {
        self.access = access;
        self
    }

    pub fn add_field(&mut self, field: RegisterField) {
        self.fields.push(field);
    }

    pub fn get_field(&self, name: &str) -> Option<&RegisterField> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn decode(&self, value: u32) -> HashMap<String, (u32, Option<String>)> {
        let mut decoded = HashMap::new();
        for field in &self.fields {
            let field_value = field.extract(value);
            let enum_name = field.value_name(field_value).cloned();
            decoded.insert(field.name.clone(), (field_value, enum_name));
        }
        decoded
    }

    pub fn encode(&self, field_values: &HashMap<String, u32>) -> u32 {
        let mut value = self.reset_value;
        for field in &self.fields {
            if let Some(&field_value) = field_values.get(&field.name) {
                value = field.insert(value, field_value);
            }
        }
        value
    }
}

/// Peripheral register block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peripheral {
    /// Peripheral name
    pub name: String,
    /// Base address
    pub base_address: u32,
    /// Registers
    pub registers: Vec<Register>,
    /// Description
    pub description: String,
}

impl Peripheral {
    pub fn new(name: impl Into<String>, base_address: u32) -> Self {
        Self {
            name: name.into(),
            base_address,
            registers: Vec::new(),
            description: String::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn add_register(&mut self, mut register: Register) {
        // Adjust address to be relative to base
        register.address += self.base_address;
        self.registers.push(register);
    }

    pub fn get_register(&self, name: &str) -> Option<&Register> {
        self.registers.iter().find(|r| r.name == name)
    }

    pub fn register_at(&self, address: u32) -> Option<&Register> {
        self.registers.iter().find(|r| r.address == address)
    }
}

/// Register debugger
#[derive(Debug, Clone)]
pub struct RegisterDebugger {
    /// Peripherals
    pub peripherals: HashMap<String, Peripheral>,
    /// Register values (address -> value)
    pub values: HashMap<u32, u32>,
    /// Value history
    pub history: Vec<(u64, u32, u32, u32)>, // (timestamp, address, old_value, new_value)
}

impl RegisterDebugger {
    pub fn new() -> Self {
        Self {
            peripherals: HashMap::new(),
            values: HashMap::new(),
            history: Vec::new(),
        }
    }

    pub fn add_peripheral(&mut self, peripheral: Peripheral) {
        // Initialize register values with reset values
        for reg in &peripheral.registers {
            self.values.insert(reg.address, reg.reset_value);
        }
        self.peripherals.insert(peripheral.name.clone(), peripheral);
    }

    pub fn read(&self, address: u32) -> u32 {
        *self.values.get(&address).unwrap_or(&0)
    }

    pub fn write(&mut self, address: u32, value: u32) {
        let old_value = self.read(address);
        self.values.insert(address, value);
        self.history
            .push((current_timestamp(), address, old_value, value));
    }

    pub fn modify(&mut self, address: u32, mask: u32, value: u32) {
        let current = self.read(address);
        let new_value = (current & !mask) | (value & mask);
        self.write(address, new_value);
    }

    pub fn decode_register(
        &self,
        peripheral_name: &str,
        register_name: &str,
    ) -> Option<HashMap<String, (u32, Option<String>)>> {
        let peripheral = self.peripherals.get(peripheral_name)?;
        let register = peripheral.get_register(register_name)?;
        let value = self.read(register.address);
        Some(register.decode(value))
    }

    pub fn set_field(
        &mut self,
        peripheral_name: &str,
        register_name: &str,
        field_name: &str,
        field_value: u32,
    ) -> Result<(), String> {
        let peripheral = self
            .peripherals
            .get(peripheral_name)
            .ok_or("Peripheral not found")?;
        let register = peripheral
            .get_register(register_name)
            .ok_or("Register not found")?;
        let field = register.get_field(field_name).ok_or("Field not found")?;

        let current = self.read(register.address);
        let new_value = field.insert(current, field_value);
        self.write(register.address, new_value);
        Ok(())
    }

    pub fn generate_c_header(&self) -> String {
        let mut header = String::new();
        header.push_str("/* Auto-generated register definitions */\n\n");

        for peripheral in self.peripherals.values() {
            header.push_str(&format!(
                "#define {}_BASE 0x{:08X}\n\n",
                peripheral.name.to_uppercase(),
                peripheral.base_address
            ));

            for register in &peripheral.registers {
                header.push_str(&format!(
                    "#define {}_{} (*(volatile uint{}_t*)0x{:08X})\n",
                    peripheral.name.to_uppercase(),
                    register.name.to_uppercase(),
                    register.width,
                    register.address
                ));

                for field in &register.fields {
                    let prefix = format!(
                        "{}_{}",
                        peripheral.name.to_uppercase(),
                        register.name.to_uppercase()
                    );
                    header.push_str(&format!(
                        "#define {}_{}_OFFSET {}\n",
                        prefix,
                        field.name.to_uppercase(),
                        field.offset
                    ));
                    header.push_str(&format!(
                        "#define {}_{}_MASK 0x{:08X}\n",
                        prefix,
                        field.name.to_uppercase(),
                        field.mask()
                    ));
                }
                header.push('\n');
            }
        }

        header
    }
}

impl Default for RegisterDebugger {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Memory analysis tests
    #[test]
    fn test_memory_section() {
        let mut section = MemorySection::new(".text", MemoryRegion::Flash, 0x0800_0000, 256 * 1024);
        section.used = 128 * 1024;

        assert_eq!(section.usage_percent(), 50.0);
        assert_eq!(section.available(), 128 * 1024);
    }

    #[test]
    fn test_memory_region_properties() {
        assert!(!MemoryRegion::Flash.is_volatile());
        assert!(MemoryRegion::Flash.is_executable());
        assert!(MemoryRegion::Ram.is_volatile());
        assert!(!MemoryRegion::Ram.is_executable());
    }

    #[test]
    fn test_memory_budget() {
        let mut budget = MemoryBudget::new("app", 64 * 1024, 16 * 1024);
        budget.update_usage(50 * 1024, 12 * 1024);

        assert!(!budget.is_over_budget());
        assert_eq!(budget.flash_margin(), 14 * 1024);
        assert_eq!(budget.ram_margin(), 4 * 1024);
    }

    #[test]
    fn test_memory_budget_over() {
        let mut budget = MemoryBudget::new("app", 64 * 1024, 16 * 1024);
        budget.update_usage(70 * 1024, 12 * 1024);

        assert!(budget.is_over_budget());
    }

    #[test]
    fn test_memory_analyzer() {
        let mut analyzer = MemoryAnalyzer::new("STM32F407");

        let mut text = MemorySection::new(".text", MemoryRegion::Flash, 0x0800_0000, 512 * 1024);
        text.used = 100 * 1024;
        analyzer.add_section(text);

        let mut data = MemorySection::new(".data", MemoryRegion::Ram, 0x2000_0000, 128 * 1024);
        data.used = 50 * 1024;
        analyzer.add_section(data);

        assert_eq!(analyzer.total_flash_used(), 100 * 1024);
        assert_eq!(analyzer.total_ram_used(), 50 * 1024);
    }

    #[test]
    fn test_memory_analyzer_report() {
        let mut analyzer = MemoryAnalyzer::new("TestMCU");

        let mut section = MemorySection::new(".text", MemoryRegion::Flash, 0, 1024);
        section.used = 512;
        analyzer.add_section(section);

        let report = analyzer.generate_map_report();
        assert!(report.contains("TestMCU"));
        assert!(report.contains("Flash"));
    }

    // RTOS scheduling tests
    #[test]
    fn test_rtos_task() {
        let mut task = RtosTask::new("sensor_task", 3, 1024);
        task.stack_used = 512;
        task.execution_count = 100;
        task.total_execution_us = 50000;

        assert_eq!(task.stack_usage_percent(), 50.0);
        assert_eq!(task.average_execution_us(), 500.0);
        assert_eq!(task.stack_headroom(), 512);
    }

    #[test]
    fn test_scheduler_analyzer() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::FreeRTOS);

        let task1 = RtosTask::new("task1", 5, 2048);
        let task2 = RtosTask::new("task2", 3, 1024);
        let task1_id = task1.task_id.clone();
        let task2_id = task2.task_id.clone();

        analyzer.add_task(task1);
        analyzer.add_task(task2);

        analyzer.record_event(&task1_id, ScheduleEventType::Start, 0, None);
        analyzer.record_event(&task1_id, ScheduleEventType::Stop, 1000, Some(1000));
        analyzer.record_event(&task2_id, ScheduleEventType::Start, 1000, None);
        analyzer.record_event(&task2_id, ScheduleEventType::Stop, 2000, Some(1000));

        assert_eq!(analyzer.tasks.len(), 2);
    }

    #[test]
    fn test_scheduler_cpu_usage() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::FreeRTOS);

        let mut task = RtosTask::new("task1", 5, 2048);
        task.total_execution_us = 50000;
        let task_id = task.task_id.clone();
        analyzer.add_task(task);
        analyzer.analysis_period_us = 100000;
        analyzer.calculate_cpu_usage();

        let task = analyzer.tasks.get(&task_id).unwrap();
        assert_eq!(task.cpu_usage, 50.0);
    }

    #[test]
    fn test_scheduler_gantt() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::Zephyr);

        analyzer.record_event("task1", ScheduleEventType::Start, 0, None);
        analyzer.record_event("task1", ScheduleEventType::Stop, 100, None);
        analyzer.record_event("task2", ScheduleEventType::Start, 100, None);
        analyzer.record_event("task2", ScheduleEventType::Stop, 200, None);

        let gantt = analyzer.generate_gantt_data();
        assert_eq!(gantt.len(), 2);
        assert_eq!(gantt[0], ("task1".to_string(), 0, 100));
    }

    #[test]
    fn test_stack_overflow_detection() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::FreeRTOS);

        let mut task = RtosTask::new("risky", 5, 1024);
        task.stack_used = 900; // 87.9% usage
        analyzer.add_task(task);

        let risks = analyzer.potential_stack_overflows();
        assert_eq!(risks.len(), 1);
    }

    // HIL testing tests
    #[test]
    fn test_hil_signal() {
        let mut signal = HilSignal::analog_input("temperature", "A0");
        signal.set_value(25.05);
        signal.expect(25.0);

        assert!(signal.matches_expected()); // Within 0.1 tolerance
    }

    #[test]
    fn test_hil_signal_mismatch() {
        let mut signal = HilSignal::analog_input("voltage", "A1");
        signal.set_value(5.0);
        signal.expect(3.3);

        assert!(!signal.matches_expected());
    }

    #[test]
    fn test_hil_test_step() {
        let step = HilTestStep::new("Set LED high and check feedback")
            .set("LED", 1.0)
            .expect("FEEDBACK", 1.0, 0.1)
            .wait(10);

        assert_eq!(step.set_signals.len(), 1);
        assert_eq!(step.check_signals.len(), 1);
        assert_eq!(step.wait_ms, 10);
    }

    #[test]
    fn test_hil_test_case() {
        let mut test = HilTestCase::new("LED test").with_description("Test LED control");

        test.add_step(HilTestStep::new("Turn on LED").set("LED", 1.0));
        test.set_result(HilTestResult::Pass, 100);

        assert!(test.passed());
    }

    #[test]
    fn test_hil_runner() {
        let mut runner = HilTestRunner::new("TestBoard");
        runner.add_signal(HilSignal::digital_output("LED", "D0"));
        runner.add_signal(HilSignal::analog_input("TEMP", "A0"));

        assert!(!runner.connected);
        runner.connect().unwrap();
        assert!(runner.connected);

        runner.set_signal("LED", 1.0).unwrap();
        assert_eq!(runner.read_signal("LED").unwrap(), 1.0);
    }

    // Register manipulation tests
    #[test]
    fn test_register_field() {
        let field = RegisterField::new("EN", 0, 1);

        assert_eq!(field.mask(), 0x0000_0001);
        assert_eq!(field.extract(0x0000_0001), 1);
        assert_eq!(field.extract(0x0000_0000), 0);
        assert_eq!(field.insert(0, 1), 1);
    }

    #[test]
    fn test_register_field_multi_bit() {
        let field = RegisterField::new("PRESCALER", 4, 4);

        assert_eq!(field.mask(), 0x0000_00F0);
        assert_eq!(field.extract(0x0000_0050), 5);
        assert_eq!(field.insert(0, 0xA), 0xA0);
    }

    #[test]
    fn test_register_field_with_enum() {
        let field = RegisterField::new("MODE", 0, 2)
            .with_enum_value(0, "OFF")
            .with_enum_value(1, "LOW")
            .with_enum_value(2, "MEDIUM")
            .with_enum_value(3, "HIGH");

        assert_eq!(field.value_name(1), Some(&"LOW".to_string()));
        assert_eq!(field.value_name(5), None);
    }

    #[test]
    fn test_register() {
        let register = Register::new("CR", 0x00, 32)
            .with_field(RegisterField::new("EN", 0, 1))
            .with_field(RegisterField::new("MODE", 1, 2))
            .with_reset_value(0);

        assert!(register.get_field("EN").is_some());
        assert!(register.get_field("INVALID").is_none());
    }

    #[test]
    fn test_register_decode() {
        let register = Register::new("SR", 0x04, 32)
            .with_field(RegisterField::new("BUSY", 0, 1))
            .with_field(RegisterField::new("ERROR", 1, 1));

        let decoded = register.decode(0b11);
        assert_eq!(decoded.get("BUSY").unwrap().0, 1);
        assert_eq!(decoded.get("ERROR").unwrap().0, 1);
    }

    #[test]
    fn test_register_encode() {
        let register = Register::new("CR", 0x00, 32)
            .with_field(RegisterField::new("EN", 0, 1))
            .with_field(RegisterField::new("MODE", 1, 2));

        let mut values = HashMap::new();
        values.insert("EN".to_string(), 1u32);
        values.insert("MODE".to_string(), 2u32);

        let encoded = register.encode(&values);
        assert_eq!(encoded, 0b101);
    }

    #[test]
    fn test_peripheral() {
        let mut gpio = Peripheral::new("GPIOA", 0x4002_0000).with_description("GPIO Port A");

        gpio.add_register(Register::new("MODER", 0x00, 32));
        gpio.add_register(Register::new("ODR", 0x14, 32));

        assert!(gpio.get_register("MODER").is_some());
        assert_eq!(gpio.get_register("MODER").unwrap().address, 0x4002_0000);
        assert_eq!(gpio.get_register("ODR").unwrap().address, 0x4002_0014);
    }

    #[test]
    fn test_register_debugger() {
        let mut debugger = RegisterDebugger::new();

        let mut gpio = Peripheral::new("GPIOA", 0x4002_0000);
        gpio.add_register(
            Register::new("ODR", 0x14, 32)
                .with_field(RegisterField::new("OD0", 0, 1))
                .with_field(RegisterField::new("OD1", 1, 1)),
        );
        debugger.add_peripheral(gpio);

        debugger.write(0x4002_0014, 0x03);
        assert_eq!(debugger.read(0x4002_0014), 0x03);

        let decoded = debugger.decode_register("GPIOA", "ODR").unwrap();
        assert_eq!(decoded.get("OD0").unwrap().0, 1);
        assert_eq!(decoded.get("OD1").unwrap().0, 1);
    }

    #[test]
    fn test_register_debugger_modify() {
        let mut debugger = RegisterDebugger::new();

        let mut periph = Peripheral::new("TEST", 0x1000);
        periph.add_register(Register::new("REG", 0, 32).with_reset_value(0xFF));
        debugger.add_peripheral(periph);

        // Modify bits 4-7
        debugger.modify(0x1000, 0xF0, 0x50);
        assert_eq!(debugger.read(0x1000), 0x5F);
    }

    #[test]
    fn test_register_debugger_set_field() {
        let mut debugger = RegisterDebugger::new();

        let mut periph = Peripheral::new("UART", 0x2000);
        periph.add_register(
            Register::new("CR", 0, 32)
                .with_field(RegisterField::new("EN", 0, 1))
                .with_field(RegisterField::new("BAUD", 8, 8))
                .with_reset_value(0),
        );
        debugger.add_peripheral(periph);

        debugger.set_field("UART", "CR", "EN", 1).unwrap();
        debugger.set_field("UART", "CR", "BAUD", 0x55).unwrap();

        assert_eq!(debugger.read(0x2000), 0x5501);
    }

    #[test]
    fn test_generate_c_header() {
        let mut debugger = RegisterDebugger::new();

        let mut periph = Peripheral::new("GPIO", 0x4000_0000);
        periph
            .add_register(Register::new("ODR", 0, 32).with_field(RegisterField::new("OD0", 0, 1)));
        debugger.add_peripheral(periph);

        let header = debugger.generate_c_header();
        assert!(header.contains("GPIO_BASE"));
        assert!(header.contains("GPIO_ODR"));
        assert!(header.contains("GPIO_ODR_OD0_MASK"));
    }

    // Additional comprehensive tests

    #[test]
    fn test_memory_region_all_variants() {
        let variants = [
            MemoryRegion::Flash,
            MemoryRegion::Ram,
            MemoryRegion::Eeprom,
            MemoryRegion::ExternalRam,
            MemoryRegion::Ccm,
            MemoryRegion::Dtcm,
            MemoryRegion::Itcm,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_memory_region_serialize() {
        let region = MemoryRegion::Flash;
        let json = serde_json::to_string(&region).unwrap();
        let parsed: MemoryRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, region);
    }

    #[test]
    fn test_memory_region_volatile() {
        assert!(!MemoryRegion::Flash.is_volatile());
        assert!(!MemoryRegion::Eeprom.is_volatile());
        assert!(MemoryRegion::Ram.is_volatile());
        assert!(MemoryRegion::ExternalRam.is_volatile());
        assert!(MemoryRegion::Ccm.is_volatile());
        assert!(MemoryRegion::Dtcm.is_volatile());
        assert!(MemoryRegion::Itcm.is_volatile());
    }

    #[test]
    fn test_memory_region_executable() {
        assert!(MemoryRegion::Flash.is_executable());
        assert!(MemoryRegion::Itcm.is_executable());
        assert!(!MemoryRegion::Ram.is_executable());
        assert!(!MemoryRegion::Eeprom.is_executable());
    }

    #[test]
    fn test_memory_section_serialize() {
        let section = MemorySection::new(".text", MemoryRegion::Flash, 0x0800_0000, 1024);
        let json = serde_json::to_string(&section).unwrap();
        let parsed: MemorySection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, section.name);
    }

    #[test]
    fn test_memory_section_zero_size() {
        let section = MemorySection::new(".empty", MemoryRegion::Ram, 0, 0);
        assert_eq!(section.usage_percent(), 0.0);
        assert_eq!(section.available(), 0);
    }

    #[test]
    fn test_memory_section_clone() {
        let section = MemorySection::new(".bss", MemoryRegion::Ram, 0x2000_0000, 4096);
        let cloned = section.clone();
        assert_eq!(cloned.name, section.name);
    }

    #[test]
    fn test_memory_budget_serialize() {
        let budget = MemoryBudget::new("core", 32768, 8192);
        let json = serde_json::to_string(&budget).unwrap();
        let parsed: MemoryBudget = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.component, "core");
    }

    #[test]
    fn test_memory_budget_clone() {
        let budget = MemoryBudget::new("app", 10000, 5000);
        let cloned = budget.clone();
        assert_eq!(cloned.flash_budget, budget.flash_budget);
    }

    #[test]
    fn test_symbol_info_struct() {
        let symbol = SymbolInfo {
            name: "main".to_string(),
            size: 256,
            region: MemoryRegion::Flash,
            section: ".text".to_string(),
            source_file: Some("main.c".to_string()),
        };
        assert_eq!(symbol.name, "main");
        assert_eq!(symbol.size, 256);
    }

    #[test]
    fn test_symbol_info_serialize() {
        let symbol = SymbolInfo {
            name: "test_func".to_string(),
            size: 100,
            region: MemoryRegion::Flash,
            section: ".text".to_string(),
            source_file: None,
        };
        let json = serde_json::to_string(&symbol).unwrap();
        assert!(json.contains("test_func"));
    }

    #[test]
    fn test_memory_analyzer_largest_symbols() {
        let mut analyzer = MemoryAnalyzer::new("TestMCU");
        analyzer.add_symbol(SymbolInfo {
            name: "small".to_string(),
            size: 10,
            region: MemoryRegion::Flash,
            section: ".text".to_string(),
            source_file: None,
        });
        analyzer.add_symbol(SymbolInfo {
            name: "large".to_string(),
            size: 1000,
            region: MemoryRegion::Flash,
            section: ".text".to_string(),
            source_file: None,
        });

        let largest = analyzer.largest_symbols(1);
        assert_eq!(largest.len(), 1);
        assert_eq!(largest[0].name, "large");
    }

    #[test]
    fn test_memory_analyzer_over_budget() {
        let mut analyzer = MemoryAnalyzer::new("TestMCU");
        let mut budget = MemoryBudget::new("app", 100, 50);
        budget.update_usage(150, 30); // Over flash budget
        analyzer.add_budget(budget);

        let over = analyzer.over_budget_components();
        assert_eq!(over.len(), 1);
    }

    #[test]
    fn test_memory_analyzer_clone() {
        let analyzer = MemoryAnalyzer::new("Device");
        let cloned = analyzer.clone();
        assert_eq!(cloned.device, analyzer.device);
    }

    #[test]
    fn test_rtos_type_all_variants() {
        let variants = [
            RtosType::FreeRTOS,
            RtosType::Zephyr,
            RtosType::ThreadX,
            RtosType::RTLinux,
            RtosType::VxWorks,
            RtosType::NuttX,
            RtosType::Custom,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_rtos_type_serialize() {
        let rtos = RtosType::FreeRTOS;
        let json = serde_json::to_string(&rtos).unwrap();
        let parsed: RtosType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, rtos);
    }

    #[test]
    fn test_task_state_all_variants() {
        let variants = [
            TaskState::Ready,
            TaskState::Running,
            TaskState::Blocked,
            TaskState::Suspended,
            TaskState::Deleted,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_task_state_serialize() {
        let state = TaskState::Running;
        let json = serde_json::to_string(&state).unwrap();
        let parsed: TaskState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[test]
    fn test_rtos_task_serialize() {
        let task = RtosTask::new("test_task", 5, 2048);
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("test_task"));
    }

    #[test]
    fn test_rtos_task_zero_execution() {
        let task = RtosTask::new("idle", 0, 512);
        assert_eq!(task.average_execution_us(), 0.0);
    }

    #[test]
    fn test_rtos_task_zero_stack() {
        let mut task = RtosTask::new("tiny", 1, 0);
        task.stack_used = 0;
        assert_eq!(task.stack_usage_percent(), 0.0);
    }

    #[test]
    fn test_schedule_event_type_all_variants() {
        let variants = [
            ScheduleEventType::Start,
            ScheduleEventType::Stop,
            ScheduleEventType::Preempt,
            ScheduleEventType::Block,
            ScheduleEventType::Resume,
            ScheduleEventType::Idle,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_schedule_event_serialize() {
        let event = ScheduleEvent {
            timestamp_us: 1000,
            task_id: "task1".to_string(),
            event: ScheduleEventType::Start,
            duration_us: Some(50),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("task1"));
    }

    #[test]
    fn test_scheduler_analyzer_idle_percentage() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::FreeRTOS);
        let mut task = RtosTask::new("task1", 5, 1024);
        task.cpu_usage = 30.0;
        analyzer.add_task(task);

        assert_eq!(analyzer.idle_percentage(), 70.0);
    }

    #[test]
    fn test_scheduler_analyzer_highest_priority() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::FreeRTOS);
        analyzer.add_task(RtosTask::new("low", 1, 512));
        analyzer.add_task(RtosTask::new("high", 10, 1024));
        analyzer.add_task(RtosTask::new("medium", 5, 768));

        let highest = analyzer.highest_priority_task().unwrap();
        assert_eq!(highest.priority, 10);
    }

    #[test]
    fn test_scheduler_analyzer_tasks_by_cpu() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::Zephyr);

        let mut t1 = RtosTask::new("t1", 1, 512);
        t1.cpu_usage = 10.0;
        analyzer.add_task(t1);

        let mut t2 = RtosTask::new("t2", 2, 512);
        t2.cpu_usage = 50.0;
        analyzer.add_task(t2);

        let sorted = analyzer.tasks_by_cpu_usage();
        assert!(sorted[0].cpu_usage > sorted[1].cpu_usage);
    }

    #[test]
    fn test_scheduler_analyzer_clone() {
        let analyzer = SchedulerAnalyzer::new(RtosType::NuttX);
        let cloned = analyzer.clone();
        assert_eq!(cloned.rtos, RtosType::NuttX);
    }

    #[test]
    fn test_hil_test_result_all_variants() {
        let variants = [
            HilTestResult::Pass,
            HilTestResult::Fail,
            HilTestResult::Timeout,
            HilTestResult::Error,
            HilTestResult::Skipped,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_hil_test_result_serialize() {
        let result = HilTestResult::Pass;
        let json = serde_json::to_string(&result).unwrap();
        let parsed: HilTestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, result);
    }

    #[test]
    fn test_signal_type_all_variants() {
        let variants = [
            SignalType::Digital,
            SignalType::Analog,
            SignalType::Pwm,
            SignalType::Serial,
            SignalType::I2c,
            SignalType::Spi,
            SignalType::Can,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_signal_type_serialize() {
        let sig = SignalType::Pwm;
        let json = serde_json::to_string(&sig).unwrap();
        let parsed: SignalType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, sig);
    }

    #[test]
    fn test_hil_signal_digital_output() {
        let signal = HilSignal::digital_output("LED", "D0");
        assert_eq!(signal.name, "LED");
        assert!(signal.is_output);
        assert_eq!(signal.signal_type, SignalType::Digital);
    }

    #[test]
    fn test_hil_signal_serialize() {
        let signal = HilSignal::analog_input("sensor", "A0");
        let json = serde_json::to_string(&signal).unwrap();
        assert!(json.contains("sensor"));
    }

    #[test]
    fn test_hil_signal_clone() {
        let signal = HilSignal::digital_output("out", "P1");
        let cloned = signal.clone();
        assert_eq!(cloned.name, signal.name);
    }

    #[test]
    fn test_hil_signal_no_expected() {
        let signal = HilSignal::digital_output("test", "D1");
        assert!(signal.matches_expected()); // No expected value = always matches
    }

    #[test]
    fn test_hil_test_step_serialize() {
        let step = HilTestStep::new("test step").set("LED", 1.0).wait(100);
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("test step"));
    }

    #[test]
    fn test_hil_test_step_clone() {
        let step = HilTestStep::new("step1");
        let cloned = step.clone();
        assert_eq!(cloned.description, step.description);
    }

    #[test]
    fn test_hil_test_case_fail() {
        let mut test = HilTestCase::new("fail_test");
        test.fail("Expected error");

        assert!(!test.passed());
        assert_eq!(test.result, Some(HilTestResult::Fail));
        assert!(test.failure_message.is_some());
    }

    #[test]
    fn test_hil_test_case_serialize() {
        let test = HilTestCase::new("serializable").with_description("test");
        let json = serde_json::to_string(&test).unwrap();
        assert!(json.contains("serializable"));
    }

    #[test]
    fn test_hil_test_runner_not_connected() {
        let mut runner = HilTestRunner::new("device");
        assert!(runner.set_signal("LED", 1.0).is_err());
        assert!(runner.read_signal("LED").is_err());
    }

    #[test]
    fn test_hil_test_runner_signal_not_found() {
        let mut runner = HilTestRunner::new("device");
        runner.connect().unwrap();
        assert!(runner.set_signal("nonexistent", 1.0).is_err());
    }

    #[test]
    fn test_hil_test_runner_clone() {
        let runner = HilTestRunner::new("board");
        let cloned = runner.clone();
        assert_eq!(cloned.device, runner.device);
    }

    #[test]
    fn test_hil_test_runner_failed_tests() {
        let mut runner = HilTestRunner::new("board");
        let mut test = HilTestCase::new("test1");
        test.fail("error");
        runner.add_test(test);

        let failed = runner.failed_tests();
        assert_eq!(failed.len(), 1);
    }

    #[test]
    fn test_register_field_serialize() {
        let field = RegisterField::new("EN", 0, 1).with_description("Enable bit");
        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("Enable"));
    }

    #[test]
    fn test_register_field_clone() {
        let field = RegisterField::new("MODE", 4, 2);
        let cloned = field.clone();
        assert_eq!(cloned.name, field.name);
    }

    #[test]
    fn test_register_access_all_variants() {
        let variants = [
            RegisterAccess::ReadOnly,
            RegisterAccess::WriteOnly,
            RegisterAccess::ReadWrite,
            RegisterAccess::ReadWriteOnce,
        ];
        for v in &variants {
            let _ = format!("{:?}", v);
        }
    }

    #[test]
    fn test_register_access_serialize() {
        let access = RegisterAccess::ReadOnly;
        let json = serde_json::to_string(&access).unwrap();
        let parsed: RegisterAccess = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, access);
    }

    #[test]
    fn test_register_with_access() {
        let reg = Register::new("STATUS", 0x10, 32).with_access(RegisterAccess::ReadOnly);
        assert_eq!(reg.access, RegisterAccess::ReadOnly);
    }

    #[test]
    fn test_register_serialize() {
        let reg = Register::new("CTRL", 0x00, 32).with_reset_value(0xFF);
        let json = serde_json::to_string(&reg).unwrap();
        assert!(json.contains("CTRL"));
    }

    #[test]
    fn test_register_clone() {
        let reg = Register::new("REG", 0x04, 16);
        let cloned = reg.clone();
        assert_eq!(cloned.name, reg.name);
    }

    #[test]
    fn test_peripheral_register_at() {
        let mut periph = Peripheral::new("UART", 0x4000_0000);
        periph.add_register(Register::new("CR", 0x00, 32));
        periph.add_register(Register::new("SR", 0x04, 32));

        assert!(periph.register_at(0x4000_0000).is_some());
        assert!(periph.register_at(0x4000_0004).is_some());
        assert!(periph.register_at(0x4000_0008).is_none());
    }

    #[test]
    fn test_peripheral_serialize() {
        let periph = Peripheral::new("SPI", 0x4001_3000).with_description("SPI1");
        let json = serde_json::to_string(&periph).unwrap();
        assert!(json.contains("SPI"));
    }

    #[test]
    fn test_peripheral_clone() {
        let periph = Peripheral::new("TIM", 0x4000_0400);
        let cloned = periph.clone();
        assert_eq!(cloned.name, periph.name);
    }

    #[test]
    fn test_register_debugger_default() {
        let debugger = RegisterDebugger::default();
        assert!(debugger.peripherals.is_empty());
        assert!(debugger.values.is_empty());
    }

    #[test]
    fn test_register_debugger_clone() {
        let debugger = RegisterDebugger::new();
        let cloned = debugger.clone();
        assert!(cloned.peripherals.is_empty());
    }

    #[test]
    fn test_register_debugger_read_unknown() {
        let debugger = RegisterDebugger::new();
        assert_eq!(debugger.read(0x1234), 0);
    }

    #[test]
    fn test_register_debugger_history() {
        let mut debugger = RegisterDebugger::new();
        let mut periph = Peripheral::new("TEST", 0x1000);
        periph.add_register(Register::new("REG", 0, 32).with_reset_value(0));
        debugger.add_peripheral(periph);

        debugger.write(0x1000, 0xFF);
        debugger.write(0x1000, 0xAA);

        assert_eq!(debugger.history.len(), 2);
    }

    #[test]
    fn test_register_debugger_set_field_errors() {
        let mut debugger = RegisterDebugger::new();

        assert!(debugger.set_field("NONE", "REG", "FIELD", 1).is_err());

        let periph = Peripheral::new("EXISTS", 0x1000);
        debugger.add_peripheral(periph);

        assert!(debugger.set_field("EXISTS", "NONE", "FIELD", 1).is_err());
    }

    #[test]
    fn test_scheduler_analyzer_zero_analysis_period() {
        let mut analyzer = SchedulerAnalyzer::new(RtosType::FreeRTOS);
        analyzer.calculate_cpu_usage(); // Should not panic
    }
}
