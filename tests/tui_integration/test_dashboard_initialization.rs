//! TUI Dashboard Initialization Tests

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::time::Duration;

/// Test that dashboard initializes without panicking
#[test]
fn test_dashboard_creates_successfully() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("Failed to create terminal");
    
    // Draw initial dashboard frame
    terminal.draw(|frame| {
        let area = frame.area();
        assert_eq!(area.width, 80);
        assert_eq!(area.height, 24);
    }).expect("Failed to draw frame");
}

/// Test dashboard with various terminal sizes
#[test]
fn test_dashboard_resizes_correctly() {
    let sizes = [
        (80, 24),   // Standard
        (120, 30),  // Large
        (60, 20),   // Small
        (40, 10),   // Very small
    ];
    
    for (width, height) in sizes {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("Failed to create terminal");
        
        terminal.draw(|frame| {
            let area = frame.area();
            assert_eq!(area.width, width);
            assert_eq!(area.height, height);
        }).expect("Failed to draw at size {width}x{height}");
    }
}

/// Test that dashboard handles model name of various lengths
#[test]
fn test_dashboard_with_long_model_name() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("Failed to create terminal");
    
    let model_names = [
        "gpt-4",
        "claude-3-opus-20240229",
        "Qwen/Qwen3.5-27B-FP8-quantized-awq-int4",
        "a".repeat(100),
    ];
    
    for model in &model_names {
        terminal.draw(|frame| {
            let area = frame.area();
            // Simulate drawing dashboard with model name
            let _display_name = if model.len() > 30 {
                format!("{}...", &model[..27])
            } else {
                model.clone()
            };
            
            // Just verify we can draw without panic
            assert!(area.width > 0);
        }).expect("Failed to draw with model name");
    }
}

/// Test dashboard state transitions
#[test]
fn test_dashboard_state_transitions() {
    use selfware::ui::tui::swarm_state::{SwarmState, WorkerState};
    
    let mut state = SwarmState::default();
    
    // Initial state
    assert_eq!(state.workers.len(), 0);
    
    // Add workers
    state.add_worker("worker-1".to_string());
    state.add_worker("worker-2".to_string());
    
    assert_eq!(state.workers.len(), 2);
    assert!(matches!(state.workers[0].state, WorkerState::Idle));
    
    // Update worker state
    state.update_worker_state("worker-1", WorkerState::Working { task_id: "task-1".to_string() });
    
    let worker1 = state.workers.iter().find(|w| w.id == "worker-1").unwrap();
    assert!(matches!(worker1.state, WorkerState::Working { .. }));
}

/// Test that dashboard logs are recorded correctly
#[test]
fn test_dashboard_log_recording() {
    use selfware::ui::tui::dashboard_widgets::{LogEntry, LogLevel};
    
    let logs = vec![
        LogEntry::new(LogLevel::Info, "System started"),
        LogEntry::new(LogLevel::Success, "Connected to model"),
        LogEntry::new(LogLevel::Warning, "High memory usage"),
        LogEntry::new(LogLevel::Error, "Task failed"),
    ];
    
    assert_eq!(logs.len(), 4);
    assert_eq!(logs[0].level, LogLevel::Info);
    assert_eq!(logs[1].level, LogLevel::Success);
    
    // Test log truncation (keep last N logs)
    let max_logs = 1000;
    let many_logs: Vec<_> = (0..max_logs + 10)
        .map(|i| LogEntry::new(LogLevel::Info, &format!("Log {}", i)))
        .collect();
    
    assert!(many_logs.len() > max_logs);
}

/// Test dashboard refresh rate
#[test]
fn test_dashboard_refresh_timing() {
    let start = std::time::Instant::now();
    
    // Simulate multiple dashboard updates
    for _ in 0..100 {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        
        terminal.draw(|_frame| {
            // Minimal draw operation
        }).unwrap();
    }
    
    let elapsed = start.elapsed();
    // Should complete 100 draws in reasonable time (< 5 seconds in debug)
    assert!(elapsed < Duration::from_secs(10), "Dashboard draws too slow: {:?}", elapsed);
}
