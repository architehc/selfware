use std::process::ExitCode;

/// Grace period after shutdown signal before force-exiting (seconds).
const SHUTDOWN_GRACE_SECS: u64 = 10;

/// Stack size for the thread the CLI actually runs on. Building the clap
/// command tree (`Cli::parse`) and polling the large `cli::run()` dispatch
/// each need hundreds of KB of frames in debug builds, which fits in the
/// 8MB Linux/macOS main-thread stack but overflows Windows' 1MB default.
/// Run the real entry point on a dedicated thread with a rustc-style
/// large stack so every subcommand works there.
const MAIN_STACK_SIZE: usize = 16 * 1024 * 1024;

fn main() -> ExitCode {
    std::thread::Builder::new()
        .name("main".into())
        .stack_size(MAIN_STACK_SIZE)
        .spawn(async_main)
        .expect("failed to spawn main thread")
        .join()
        .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

#[tokio::main]
async fn async_main() -> ExitCode {
    // Spawn a signal handler that sets the global shutdown flag.
    // Components (task runner, REPL, TUI) already check for ctrl_c or
    // can poll `selfware::is_shutdown_requested()` to wind down.
    // After the grace period, force-exit to avoid hanging on stuck I/O.
    tokio::spawn(async {
        let mut signals = ShutdownSignals::new();

        // First signal: ask every component to wind down and start a bounded
        // grace timer.
        signals.recv().await;
        selfware::request_shutdown();
        eprintln!(
            "\nReceived shutdown signal, winding down... \
             (press Ctrl-C again to force quit)"
        );

        // Race the grace period against a SECOND signal. An impatient operator
        // pressing Ctrl-C again should exit immediately instead of waiting out
        // the full grace period (the previous handler ignored further signals,
        // so an active run could hang for the whole window).
        tokio::select! {
            _ = signals.recv() => {
                eprintln!("\nSecond signal received, forcing immediate exit.");
                std::process::exit(130); // 128 + SIGINT
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(SHUTDOWN_GRACE_SECS)) => {
                eprintln!("Shutdown grace period expired, forcing exit.");
                std::process::exit(1);
            }
        }
    });

    // Optional localhost health endpoint for container probes.
    selfware::supervision::health::maybe_start_health_endpoint();

    // Let cli::run() complete naturally.  The task runner, interactive
    // REPL, and TUI all have their own signal handling and will wind
    // down when a signal arrives — we no longer race them with
    // tokio::select! which would drop the future mid-flight.
    let result = selfware::cli::run().await;

    selfware::shutdown_tracing();

    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {:?}", e);
            ExitCode::from(selfware::errors::get_exit_code(&e))
        }
    }
}

/// A re-armable source of shutdown signals. Unlike a bare `tokio::signal::ctrl_c()`
/// future (which must be recreated per await), `recv()` can be awaited any number
/// of times, so the caller can distinguish the first shutdown request from a
/// second "force quit" press.
#[cfg(unix)]
struct ShutdownSignals {
    sigint: tokio::signal::unix::Signal,
    sigterm: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl ShutdownSignals {
    fn new() -> Self {
        use tokio::signal::unix::{signal, SignalKind};
        Self {
            sigint: signal(SignalKind::interrupt()).expect("failed to register SIGINT handler"),
            sigterm: signal(SignalKind::terminate()).expect("failed to register SIGTERM handler"),
        }
    }

    async fn recv(&mut self) {
        tokio::select! {
            _ = self.sigint.recv() => {},
            _ = self.sigterm.recv() => {},
        }
    }
}

#[cfg(not(unix))]
struct ShutdownSignals;

#[cfg(not(unix))]
impl ShutdownSignals {
    fn new() -> Self {
        Self
    }

    async fn recv(&mut self) {
        // ctrl_c() yields a fresh future each call, so it is naturally re-armable.
        let _ = tokio::signal::ctrl_c().await;
    }
}
