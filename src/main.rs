use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match selfware::cli::run().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {:?}", e);
            
            ExitCode::from(selfware::errors::get_exit_code(&e))
        }
    }
}
