//! Selfware - Autonomous AI Agent Runtime
//! 
//! Main entry point for the Selfware autonomous agent system.

use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{info, warn, error};

use selfware::config::Config;
use selfware::observability::{init_logging, init_metrics};
use selfware::checkpoint::CheckpointManager;
use selfware::resource::ResourceManager;
use selfware::llm::ModelLifecycleManager;
use selfware::supervision::{Supervisor, SupervisionStrategy, RestartPolicy, BackoffStrategy};

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(name = "selfware")]
#[command(about = "Autonomous AI agent runtime with recursive self-improvement")]
struct Args {
    /// Configuration file path
    #[arg(short, long, env = "SELFWARE_CONFIG")]
    config: Option<std::path::PathBuf>,
    
    /// Recover from last checkpoint
    #[arg(long, env = "SELFWARE_RECOVER")]
    recover: bool,
    
    /// Specific checkpoint ID to recover from
    #[arg(long, env = "SELFWARE_CHECKPOINT_ID")]
    checkpoint_id: Option<String>,
    
    /// Session ID (auto-generated if not provided)
    #[arg(short, long, env = "SELFWARE_SESSION_ID")]
    session_id: Option<String>,
    
    /// Maximum runtime in hours
    #[arg(long, env = "SELFWARE_MAX_RUNTIME_HOURS")]
    max_runtime_hours: Option<u32>,
    
    /// Log level override
    #[arg(short, long, env = "RUST_LOG")]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse arguments
    let args = Args::parse();
    
    // Load configuration
    let config = if let Some(config_path) = args.config {
        Config::from_file(config_path)?
    } else {
        Config::load()?
    };
    
    // Initialize logging
    init_logging(&config.observability.logging)?;
    init_metrics(&config.observability.metrics)?;
    
    // Set session ID
    let session_id = args.session_id
        .or_else(|| std::env::var("SELFWARE_SESSION_ID").ok())
        .unwrap_or_else(|| {
            let id = uuid::Uuid::new_v4().to_string();
            std::env::set_var("SELFWARE_SESSION_ID", &id);
            id
        });
    
    info!(
        session_id = %session_id,
        version = env!("CARGO_PKG_VERSION"),
        "Starting Selfware autonomous runtime"
    );
    
    // Initialize core components
    let checkpoint_manager = Arc::new(
        CheckpointManager::new(&config.checkpoint).await?
    );
    
    let resource_manager = Arc::new(
        ResourceManager::new(&config.resources).await?
    );
    
    let model_manager = Arc::new(
        ModelLifecycleManager::new(&config.llm).await?
    );
    
    // Attempt recovery if requested or if ungraceful shutdown detected
    let recovered_state = if args.recover || checkpoint_manager.needs_recovery().await? {
        info!("Attempting recovery from checkpoint");
        match checkpoint_manager.recover(args.checkpoint_id.as_deref()).await? {
            Some(state) => {
                info!("Successfully recovered from checkpoint");
                Some(state)
            }
            None => {
                warn!("No checkpoint found, starting fresh");
                None
            }
        }
    } else {
        None
    };
    
    // Create supervision tree
    let supervisor = Supervisor::builder()
        .with_strategy(config.supervision.strategy.clone())
        .with_restart_policy(RestartPolicy {
            max_restarts: config.supervision.max_restarts,
            max_seconds: config.supervision.max_seconds,
            backoff_strategy: config.supervision.backoff_strategy.clone(),
        })
        .add_child("checkpoint_manager", checkpoint_manager.clone())
        .add_child("resource_manager", resource_manager.clone())
        .add_child("model_manager", model_manager.clone())
        .build();
    
    // Start supervision tree
    let _supervisor_handle = supervisor.start().await?;
    
    // Start resource monitoring
    let resource_monitor = tokio::spawn({
        let rm = resource_manager.clone();
        async move { rm.monitor_loop().await }
    });
    
    // Start checkpoint scheduler
    let checkpoint_scheduler = tokio::spawn({
        let cm = checkpoint_manager.clone();
        async move { cm.scheduler_loop().await }
    });
    
    // Start health server
    let health_server = tokio::spawn({
        let port = config.intervention.port;
        async move {
            let app = selfware::observability::health_router();
            let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
                .await
                .expect("Failed to bind health server");
            axum::serve(listener, app).await.expect("Health server failed");
        }
    });
    
    // Set up shutdown handler
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let shutdown_clone = shutdown.clone();
    
    tokio::spawn(async move {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to create SIGTERM handler");
        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
            .expect("Failed to create SIGINT handler");
        
        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, initiating graceful shutdown");
            }
            _ = signal::ctrl_c() => {
                info!("Received Ctrl+C, initiating graceful shutdown");
            }
        }
        
        shutdown_clone.notify_one();
    });
    
    // Set up max runtime timeout if specified
    let max_runtime = args.max_runtime_hours
        .map(|h| Duration::from_secs(h as u64 * 3600))
        .or_else(|| Some(Duration::from_secs(config.system.max_runtime_hours as u64 * 3600)));
    
    // Main execution loop
    let main_loop = tokio::spawn({
        let cm = checkpoint_manager.clone();
        let mm = model_manager.clone();
        let rm = resource_manager.clone();
        let state = recovered_state;
        async move {
            run_agent_loop(state, cm, mm, rm).await
        }
    });
    
    // Wait for shutdown signal or main loop completion
    tokio::select! {
        _ = shutdown.notified() => {
            info!("Shutdown signal received, stopping main loop");
        }
        result = main_loop => {
            match result {
                Ok(Ok(())) => info!("Main loop completed successfully"),
                Ok(Err(e)) => error!("Main loop failed: {:?}", e),
                Err(e) => error!("Main loop panicked: {:?}", e),
            }
        }
    }
    
    // Graceful shutdown
    info!("Initiating graceful shutdown");
    
    // Create final checkpoint
    if let Err(e) = checkpoint_manager.create_graceful_shutdown_checkpoint().await {
        error!("Failed to create final checkpoint: {:?}", e);
    }
    
    // Flush all pending writes
    if let Err(e) = checkpoint_manager.flush().await {
        error!("Failed to flush checkpoint manager: {:?}", e);
    }
    
    // Cancel background tasks
    resource_monitor.abort();
    checkpoint_scheduler.abort();
    health_server.abort();
    
    info!("Graceful shutdown complete");
    
    Ok(())
}

/// Main agent execution loop
async fn run_agent_loop(
    initial_state: Option<selfware::checkpoint::RecoveredState>,
    checkpoint_manager: Arc<CheckpointManager>,
    model_manager: Arc<ModelLifecycleManager>,
    resource_manager: Arc<ResourceManager>,
) -> Result<(), selfware::error::SelfwareError> {
    use selfware::agent::{AgentState, Task, TaskResult};
    
    let mut state = initial_state
        .map(|s| s.into_agent_state())
        .unwrap_or_else(AgentState::new);
    
    let mut iteration = 0u64;
    
    loop {
        iteration += 1;
        
        // Periodic checkpoint
        if iteration % 10 == 0 {
            checkpoint_manager.checkpoint_session(&state).await?;
        }
        
        // Check resource pressure
        let pressure = resource_manager.get_resource_pressure().await;
        if pressure.is_critical() {
            warn!("Critical resource pressure detected, pausing for recovery");
            tokio::time::sleep(Duration::from_secs(30)).await;
            continue;
        }
        
        // Get next task
        let task = match state.next_task().await {
            Some(task) => task,
            None => {
                // Generate self-improvement task
                info!("Generating self-improvement task");
                generate_self_improvement_task(&state, &model_manager).await?
            }
        };
        
        // Execute task with recovery
        let task_id = task.id.clone();
        let result = execute_task_with_recovery(&task, &model_manager).await;
        
        match result {
            Ok(task_result) => {
                info!(task_id = %task_id, "Task completed successfully");
                state.record_completion(task, task_result);
            }
            Err(e) if e.is_recoverable() => {
                warn!(task_id = %task_id, error = %e, "Task failed (recoverable)");
                state.requeue_task(task);
                
                // Apply backoff
                let backoff = Duration::from_secs(2u64.pow(state.consecutive_failures.min(5)));
                tokio::time::sleep(backoff).await;
            }
            Err(e) => {
                error!(task_id = %task_id, error = %e, "Task failed (unrecoverable)");
                state.record_failure(task, e);
            }
        }
        
        // Periodic maintenance every 100 iterations
        if iteration % 100 == 0 {
            info!(iteration = iteration, "Performing periodic maintenance");
            state = perform_maintenance(state, &resource_manager).await?;
        }
    }
}

/// Generate a self-improvement task
async fn generate_self_improvement_task(
    state: &selfware::agent::AgentState,
    model_manager: &Arc<ModelLifecycleManager>,
) -> Result<selfware::agent::Task, selfware::error::SelfwareError> {
    // This would use the LLM to generate improvement tasks
    // For now, return a placeholder
    Ok(selfware::agent::Task::new(
        "self_improvement",
        "Analyze recent performance and suggest improvements",
        selfware::Priority::Background,
    ))
}

/// Execute a task with automatic recovery
async fn execute_task_with_recovery(
    task: &selfware::agent::Task,
    model_manager: &Arc<ModelLifecycleManager>,
) -> Result<selfware::agent::TaskResult, selfware::error::SelfwareError> {
    use selfware::error::Recoverable;
    
    let max_retries = 3;
    let mut last_error = None;
    
    for attempt in 0..max_retries {
        match execute_task(task, model_manager).await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_recoverable() && attempt < max_retries - 1 => {
                warn!(attempt = attempt + 1, error = %e, "Task execution failed, retrying");
                last_error = Some(e);
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
            }
            Err(e) => return Err(e),
        }
    }
    
    Err(last_error.unwrap_or_else(|| {
        selfware::error::SelfwareError::Unknown("Max retries exceeded".to_string())
    }))
}

/// Execute a single task
async fn execute_task(
    task: &selfware::agent::Task,
    model_manager: &Arc<ModelLifecycleManager>,
) -> Result<selfware::agent::TaskResult, selfware::error::SelfwareError> {
    // Task execution logic here
    // This would integrate with the LLM and other components
    Ok(selfware::agent::TaskResult::success())
}

/// Perform periodic maintenance
async fn perform_maintenance(
    state: selfware::agent::AgentState,
    resource_manager: &Arc<ResourceManager>,
) -> Result<selfware::agent::AgentState, selfware::error::SelfwareError> {
    // Run garbage collection hints
    
    // Compact state if needed
    let compacted = state.compact();
    
    // Report metrics
    resource_manager.report_metrics().await?;
    
    Ok(compacted)
}
