use anyhow::Context;
use clap::Parser;

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
