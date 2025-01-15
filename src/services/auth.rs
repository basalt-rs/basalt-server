use protoxene::{LoginRequest, LoginResponse};
use tonic::{Request, Response, Status};
use tracing::debug;

/// Provides authentication functionality
#[derive(Default)]
pub struct AuthService {}

#[tonic::async_trait]
impl protoxene::auth_server::Auth for AuthService {
    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        debug!("[login]: {:?}", request);
        Err(Status::unimplemented(
            "login functionality not yet implemented",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::testing;

    use super::*;

    #[tokio::test]
    async fn test_login_unimplemented() {
        let (serve_future, channel) = testing::mock_server().await;
        // create client from channel
        let mut client = protoxene::auth_client::AuthClient::new(channel);
        let response_future = client.login(LoginRequest {
            name: "".into(),
            password: "".into(),
        });

        let response = testing::receive_response(response_future, serve_future).await;

        match response {
            Ok(_) => panic!("should have failed"),
            Err(_) => (),
        }
    }
}
