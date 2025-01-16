use anyhow::Context;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod run;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Run(run::RunArgs),
}

pub async fn handle_cmd() -> anyhow::Result<()> {
    let args = Cli::try_parse().context("Failed to parse CLI arguments")?;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_env("BASALT_SERVER_LOGGING")
                .unwrap_or_else(|_| "basalt_server=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let result = match args.command {
        Command::Run(args) => run::handle(args)
            .await
            .context("Failed to execute `run` command")?,
    };

    Ok(result)
}
