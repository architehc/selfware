//! Structured logging setup

use crate::config::LoggingConfig;
use crate::error::SelfwareError;
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Initialize logging
pub fn init_logging(config: &LoggingConfig) -> Result<(), SelfwareError> {
    // Ensure log directory exists
    std::fs::create_dir_all(&config.log_dir).map_err(|e| {
        SelfwareError::Config(format!("Failed to create log directory: {}", e))
    })?;
    
    // Create file appender with rotation
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("selfware")
        .filename_suffix("log")
        .max_log_files(config.retention_days as usize)
        .build(&config.log_dir)
        .map_err(|e| SelfwareError::Config(format!("Failed to create file appender: {}", e)))?;
    
    // Create layers based on format
    let (file_layer, console_layer) = match config.format.as_str() {
        "json" => {
            let file = fmt::layer()
                .json()
                .with_writer(file_appender)
                .with_timer(fmt::time::UtcTime::rfc_3339())
                .with_current_span(true)
                .with_span_list(true);
            
            let console = fmt::layer()
                .json()
                .with_writer(std::io::stdout)
                .with_timer(fmt::time::UtcTime::rfc_3339());
            
            (Some(file), Some(console))
        }
        _ => {
            let file = fmt::layer()
                .with_writer(file_appender)
                .with_timer(fmt::time::UtcTime::rfc_3339())
                .with_ansi(false);
            
            let console = fmt::layer()
                .pretty()
                .with_timer(fmt::time::UtcTime::rfc_3339());
            
            (Some(file), Some(console))
        }
    };
    
    // Build filter
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));
    
    // Initialize subscriber
    let registry = tracing_subscriber::registry().with(filter);
    
    if let Some(file) = file_layer {
        if let Some(console) = console_layer {
            registry.with(file).with(console).init();
        } else {
            registry.with(file).init();
        }
    } else if let Some(console) = console_layer {
        registry.with(console).init();
    }
    
    tracing::info!(log_dir = %config.log_dir.display(), "Logging initialized");
    
    Ok(())
}

/// Log rotation configuration
#[derive(Debug, Clone)]
pub struct LogRotation {
    pub max_files: usize,
    pub max_size_mb: u64,
    pub compress: bool,
}

impl Default for LogRotation {
    fn default() -> Self {
        Self {
            max_files: 7,
            max_size_mb: 100,
            compress: true,
        }
    }
}

/// Get log file path for a date
pub fn log_file_path(log_dir: &PathBuf, date: &chrono::NaiveDate) -> PathBuf {
    log_dir.join(format!("selfware-{}.log", date.format("%Y-%m-%d")))
}

/// Clean up old log files
pub fn cleanup_old_logs(log_dir: &PathBuf, retention_days: u32) -> Result<usize, std::io::Error> {
    let mut cleaned = 0;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let modified = chrono::DateTime::<chrono::Utc>::from(modified);
                    if modified < cutoff {
                        if let Err(e) = std::fs::remove_file(entry.path()) {
                            tracing::warn!(path = %entry.path().display(), error = %e, "Failed to remove old log");
                        } else {
                            cleaned += 1;
                        }
                    }
                }
            }
        }
    }
    
    Ok(cleaned)
}
