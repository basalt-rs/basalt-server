use protoxene;

/// Provides authentication functionality
#[derive(Default)]
pub struct AuthService {}

#[tonic::async_trait]
impl protoxene::auth_server::Auth for AuthService {
    async fn login(
        &self,
        _request: tonic::Request<protoxene::LoginRequest>,
    ) -> Result<tonic::Response<protoxene::LoginResponse>, tonic::Status> {
        todo!("login functionality not yet implemented")
    }
}
