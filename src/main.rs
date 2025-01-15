pub mod cli;
pub mod services;

#[cfg(test)]
mod testing;

#[tokio::main]
async fn main() -> anyhow::Result<(), String> {
    cli::handle_cmd().await?;
    Ok(())
}
