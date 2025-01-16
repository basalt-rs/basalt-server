use hyper_util::rt::TokioIo;
use std::fs;
use std::future::Future;
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tower::service_fn;

use crate::services::auth::AuthService;

pub async fn receive_response<T>(
    response: impl Future<Output = T>,
    serve_future: impl Future<Output = ()>,
) -> T {
    tokio::select! {
        _ = serve_future => panic!("server closed early"),
        r = response => r
    }
}

pub async fn mock_server() -> (impl Future<Output = ()>, Channel) {
    let tempfile = async_tempfile::TempFile::new()
        .await
        .expect("Failed to create temporary file for socket");
    let path = tempfile.file_path();
    fs::remove_file(&path).expect("Failed to remove temp file initially");
    let socket = Arc::new(&*path);

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

    // Connect to the server over a Unix socket at `socket`
    let channel = Endpoint::try_from(path.display().to_string())
        .unwrap()
        .connect_with_connector(service_fn(|uri: Uri| async move {
            // Connect to a Uds socket
            Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(uri.path()).await?))
        }))
        .await
        .unwrap();

    return (serve_future, channel);
}
