use anyhow::Context;
use clap::Parser;
use tonic::transport::Server;
use tracing::info;

use crate::services::auth::AuthService;

#[derive(Parser, Debug)]
pub struct RunArgs {
    #[arg()]
    port: u16,
}

pub async fn handle(args: RunArgs) -> anyhow::Result<()> {
    // TODO: Parse configurations
    let addr = format!("[::1]:{}", args.port).parse().unwrap();
    info!("Serving via gRPC");
    Server::builder()
        .accept_http1(true)
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(protoxene::auth_server::AuthServer::new(
            AuthService::default(),
        ))
        .serve(addr)
        .await
        .context("Failed to build server")?;
    Ok(())
}
