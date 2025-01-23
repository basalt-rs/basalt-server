use std::sync::Arc;
use tokio::sync::RwLock;

use protoxene::{LoginRequest, LoginResponse};
use tonic::{Request, Response, Status};
use tracing::debug;

use crate::storage::SqliteLayer;

/// Provides authentication functionality
pub struct AuthService {
    sqlite: Arc<RwLock<SqliteLayer>>,
}

impl AuthService {
    pub fn new(sqlite: Arc<RwLock<SqliteLayer>>) -> Self {
        Self { sqlite }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct User {
    id: String,
    name: String,
    password_hash: String,
}

#[tonic::async_trait]
impl protoxene::auth_server::Auth for AuthService {
    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        debug!("[login]: {:?}", request);
        let sqlite = self.sqlite.read().await;
        let users: Vec<User> = sqlx::query_as("SELECT * FROM users")
            .fetch_all(&sqlite.db)
            .await
            .unwrap();
        dbg!("printing users");
        dbg!(users);
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
        let (_, db) = testing::mock_db().await;
        let (serve_future, channel) = testing::mock_server(db).await;
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

    #[tokio::test]
    async fn test_login_unimplemented_2() {
        let (_, db) = testing::mock_db().await;
        let (serve_future, channel) = testing::mock_server(db).await;
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
