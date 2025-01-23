use anyhow::Context;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod list;
mod run;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Run(run::RunArgs),
    List,
}

pub async fn handle_cmd() -> anyhow::Result<()> {
    let args = Cli::parse();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_env("BASALT_SERVER_LOGGING")
                .unwrap_or_else(|_| "basalt_server=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    match args.command {
        Command::Run(args) => run::handle(args)
            .await
            .context("Failed to execute `run` command")?,
        Command::List => list::handle()
            .await
            .context("Failed to execute `list` command")?,
    };

    Ok(())
}
