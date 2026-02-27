use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let result = tokio::select! {
        result = selfware::cli::run() => result,
        _ = shutdown_signal() => {
            eprintln!("\nReceived shutdown signal, exiting gracefully...");
            Ok(())
        }
    };

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
