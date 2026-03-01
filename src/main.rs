use std::process::ExitCode;

/// Grace period after shutdown signal before force-exiting (seconds).
const SHUTDOWN_GRACE_SECS: u64 = 10;

#[tokio::main]
async fn main() -> ExitCode {
    // Spawn a signal handler that sets the global shutdown flag.
    // Components (task runner, REPL, TUI) already check for ctrl_c or
    // can poll `selfware::is_shutdown_requested()` to wind down.
    // After the grace period, force-exit to avoid hanging on stuck I/O.
    tokio::spawn(async {
        shutdown_signal().await;
        selfware::request_shutdown();
        eprintln!("\nReceived shutdown signal, exiting gracefully...");

        tokio::time::sleep(std::time::Duration::from_secs(SHUTDOWN_GRACE_SECS)).await;
        eprintln!("Shutdown grace period expired, forcing exit.");
        std::process::exit(1);
    });

    // Let cli::run() complete naturally.  The task runner, interactive
    // REPL, and TUI all have their own signal handling and will wind
    // down when a signal arrives — we no longer race them with
    // tokio::select! which would drop the future mid-flight.
    let result = selfware::cli::run().await;

    selfware::observability::telemetry::shutdown_tracing();

    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {:?}", e);
            ExitCode::from(selfware::errors::get_exit_code(&e))
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}
