use anyhow::Context;

pub mod cli;
pub mod services;
mod storage;

#[cfg(test)]
mod testing;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::handle_cmd()
        .await
        .context("Failed to handle command")?;
    Ok(())
}
