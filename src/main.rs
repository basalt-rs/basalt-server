use std::process::exit;

pub mod cli;
pub mod services;

#[tokio::main]
async fn main() {
    if let Err(msg) = cli::handle_cmd().await {
        eprintln!("{}", msg);
        exit(1);
    }
}
