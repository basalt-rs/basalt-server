use anyhow::Context;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(debug_assertions)]
mod docgen;
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
    #[cfg(debug_assertions)]
    DocGen,
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
        #[cfg(debug_assertions)]
        Command::DocGen => docgen::handle()
            .await
            .context("Failed to execute `doc-gen` command")?,
    };

    Ok(())
}
