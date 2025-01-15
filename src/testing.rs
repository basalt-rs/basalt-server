use hyper_util::rt::TokioIo;
use std::future::Future;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tower::service_fn;

use crate::services::auth::AuthService;

pub async fn mock_server() -> (impl Future<Output = ()>, Channel) {
    let socket = NamedTempFile::new().unwrap();
    let socket = Arc::new(socket.into_temp_path());
    std::fs::remove_file(&*socket).unwrap();

    let uds = UnixListener::bind(&*socket).unwrap();
    let stream = UnixListenerStream::new(uds);
    let serve_future = async {
        let result = Server::builder()
            .add_service(protoxene::auth_server::AuthServer::new(
                AuthService::default(),
            ))
            .serve_with_incoming(stream)
            .await;
        // Server must be running fine...
        assert!(result.is_ok());
    };

    println!("DISPLAY: {}", socket.display());

    // Connect to the server over a Unix socket at `socket`
    let channel = Endpoint::try_from(format!("file://localhost{}", socket.display()))
        .unwrap()
        .connect_with_connector(service_fn(|uri: Uri| async move {
            // Connect to a Uds socket
            Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(uri.path()).await?))
        }))
        .await
        .unwrap();

    return (serve_future, channel);
}
