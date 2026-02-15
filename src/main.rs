use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    selfware::cli::run().await
}
