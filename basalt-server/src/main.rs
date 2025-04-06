use anyhow::Context;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_env("BASALT_SERVER_LOGGING")
                .unwrap_or_else(|_| "basalt_server=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    cli::handle_cmd().await.context("Failed to handle command")
}
