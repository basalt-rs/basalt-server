use hyper::rt::{Read, Write};
use hyper_util::rt::TokioIo;
use protoxene::auth_client::AuthClient;
use std::future::Future;
use std::sync::Arc;
use tempfile::{NamedTempFile, TempPath};
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tonic::{Request, Response, Status};
use tower::service_fn;

use crate::services::auth::AuthService;

pub async fn mock_server() -> (Arc<TempPath>, impl Future<Output = ()>, AuthClient<Channel>) {
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

    let socket = Arc::clone(&socket);
    // Connect to the server over a Unix socket
    // The URL will be ignored.
    let channel = Endpoint::try_from("http://[::]:50051")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| async {
            // Connect to a Uds socket
            let socket = Arc::clone(&socket);
            Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(&*socket).await?))
        }))
        .await
        .unwrap();

    let client = protoxene::auth_client::AuthClient::new(channel);

    return (socket, serve_future, client);
}
