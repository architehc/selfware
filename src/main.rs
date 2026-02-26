use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match selfware::cli::run().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            
            if selfware::errors::is_confirmation_error(&e) {
                return ExitCode::from(6); // Confirmation required
            }

            if let Some(selfware_err) = e.downcast_ref::<selfware::errors::SelfwareError>() {
                match selfware_err {
                    selfware::errors::SelfwareError::Config(_) => return ExitCode::from(2),
                    selfware::errors::SelfwareError::Api(_) => return ExitCode::from(4),
                    selfware::errors::SelfwareError::Safety(_) => return ExitCode::from(5),
                    _ => return ExitCode::from(1),
                }
            }

            // Provide fallback checking for direct error enum usage in Anyhow wrapper
            let msg = e.to_string().to_lowercase();
            if msg.contains("config") {
                return ExitCode::from(2);
            } else if msg.contains("api error") || msg.contains("network") {
                return ExitCode::from(4);
            } else if msg.contains("safety") || msg.contains("blocked") {
                return ExitCode::from(5);
            }

            ExitCode::from(1)
        }
    }
}
