use clap::Parser;
use tonic::transport::Server;
use tracing::info;

use crate::services::auth::AuthService;

#[derive(Parser, Debug)]
pub struct RunArgs {
    #[arg()]
    port: u16,
}

pub async fn handle(args: RunArgs) -> anyhow::Result<(), String> {
    // TODO: Parse configurations
    let addr = format!("[::1]:{}", args.port).parse().unwrap();
    info!("Serving via gRPC");
    Server::builder()
        .add_service(protoxene::auth_server::AuthServer::new(
            AuthService::default(),
        ))
        .serve(addr)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
