use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use clap::Parser;
use rand::distributions::Distribution;
use tracing::info;

use basalt_server_lib::{
    server::{self, AppState},
    storage::SqliteLayer,
};

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Port on which to host the server.  Defaults to the port specified in the config file if
    /// omitted.
    #[arg(long, short)]
    port: Option<u16>,
    /// Name of the competition.  Used to determine location for data folder.  Defaults to a random
    /// string if omitted.
    #[arg(long, short)]
    name: Option<String>,
    /// Path to the configuration to use to host the server.
    #[arg(default_value = "basalt.toml")]
    config: PathBuf,
    /// Directory in which files for the web client are stored -- if not specified, the web client
    /// is disabled.
    #[arg(long, short)]
    web_dir: Option<PathBuf>,
}

fn default_name() -> String {
    rand::distributions::Alphanumeric
        .sample_iter(rand::thread_rng())
        .take(12)
        .map(char::from)
        .collect()
}

pub async fn handle(args: RunArgs) -> anyhow::Result<()> {
    info!("Parsing packet configurations");

    let file = tokio::fs::File::open(&args.config)
        .await
        .context("Opening packet file")?;

    let mut file = tokio::io::BufReader::new(file);

    let file_name = args
        .config
        .file_name()
        .expect("call to File::open would fail if this does")
        .to_str();

    let config = match bedrock::Config::read_async(&mut file, file_name).await {
        Ok(config) => config,
        Err(err @ bedrock::ConfigReadError::ReadError(_)) => Err(err)?,
        Err(bedrock::ConfigReadError::MalformedData(err)) => {
            eprintln!("{:?}", err);
            anyhow::bail!("parsing config");
        }
    };

    let name = &args.name.unwrap_or_else(default_name);
    info!(name, "Creating Sqlite layer");
    let (init, db) = SqliteLayer::new(name)
        .await
        .context("Creating Sqlite Layer")?;

    if init {
        db.ingest(&config)
            .await
            .context("Failed to ingest config data")?;
    }

    let addr: SocketAddr = format!("[::]:{}", args.port.unwrap_or(config.port))
        .parse()
        .unwrap();
    info!(?addr, "Serving via HTTP");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        server::router(Arc::new(AppState::new(db, config, args.web_dir)))
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
