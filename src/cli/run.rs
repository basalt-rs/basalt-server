use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use clap::Parser;
use rand::distributions::Distribution;
use tokio::sync::RwLock;
use tonic::transport::Server;
use tracing::info;

use crate::{services::auth::AuthService, storage::SqliteLayer};

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
    return std::str::from_utf8(&bytes).unwrap().to_owned();
}

pub async fn handle(args: RunArgs) -> anyhow::Result<()> {
    // TODO: Parse configurations
    info!("Parsing packet configurations");
    let _ = packet::Packet::try_from(PathBuf::from(args.packet))
        .context("Parsing packet configurations")?;

    info!("Creating Sqlite layer");
    let db = SqliteLayer::new(args.name.unwrap_or(default_name()))
        .await
        .context("Creating Sqlite Layer")?;
    let db = Arc::new(RwLock::new(db));

    let addr = format!("[::1]:{}", args.port).parse().unwrap();
    info!("Serving via gRPC");
    Server::builder()
        .accept_http1(true)
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(protoxene::auth_server::AuthServer::new(AuthService::new(
            db,
        )))
        .serve(addr)
        .await
        .context("Failed to build server")?;
    Ok(())
}
