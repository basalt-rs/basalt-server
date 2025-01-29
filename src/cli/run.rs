use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use clap::Parser;
use rand::distributions::Distribution;
use tokio::sync::RwLock;
use tracing::info;

use crate::{
    server::{router, AppState},
    storage::SqliteLayer,
};

#[derive(Parser, Debug)]
pub struct RunArgs {
    #[arg()]
    port: u16,
    #[arg(short)]
    name: Option<String>,
    packet: String,
}

fn default_name() -> String {
    let bytes = rand::distributions::Alphanumeric
        .sample_iter(rand::thread_rng())
        .take(12)
        .collect::<Vec<u8>>();
    std::str::from_utf8(&bytes).unwrap().to_owned()
}

pub async fn handle(args: RunArgs) -> anyhow::Result<()> {
    info!("Parsing packet configurations");
    let file = tokio::fs::File::open(&args.packet)
        .await
        .context("Failed to open file")?;
    let mut reader = tokio::io::BufReader::new(file);
    let _ = bedrock::Config::read_async(&mut reader, Some(args.packet))
        .await
        .context("Failed to parse configurations")?;

    info!("Creating Sqlite layer");
    let db = SqliteLayer::new(args.name.unwrap_or(default_name()))
        .await
        .context("Creating Sqlite Layer")?;

    let addr: SocketAddr = format!("[::1]:{}", args.port).parse().unwrap();
    info!("Serving via HTTP");
    let service = router(Arc::new(AppState {
        db: RwLock::new(db),
    }));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, service).await?;

    Ok(())
}
