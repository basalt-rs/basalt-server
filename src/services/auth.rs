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
    impl AuthService {
        /// Create a mock instance of AuthService for the sake
        /// of testing its functionality
        pub fn mock() -> Self {
            Self {}
        }
    }
    use super::*;
    #[test]
    fn test_login() {
        let auth_service = AuthService::mock();
    }
}
