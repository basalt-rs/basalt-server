pub mod cli;
pub mod services;

#[tokio::main]
async fn main() -> anyhow::Result<(), String> {
    cli::handle_cmd().await?;
    Ok(())
}
