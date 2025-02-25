use crate::server::AppState;
use axum::{extract::State, Json};
use bedrock::packet::{Problem, Test};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::{collections::HashSet, ops::Deref};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TestResponse {
    pub input: String,
    pub output: String,
}

impl From<&Test> for TestResponse {
    fn from(value: &Test) -> Self {
        Self {
            input: value.input.clone(),
            output: value.output.clone(),
        }
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct QuestionResponse {
    languages: Option<BTreeSet<String>>,
    title: String,
    description: Option<String>,
    tests: Vec<TestResponse>,
}

impl From<&Problem> for QuestionResponse {
    fn from(value: &Problem) -> Self {
        Self {
            languages: value.languages.clone(),
            title: value.title.clone(),
            description: value.description.as_ref().map(|x| x.html().unwrap()),
            tests: value.tests.iter().map(From::from).collect(),
        }
    }
}

#[axum::debug_handler]
#[utoipa::path(get, path = "/", responses((status = OK, body = Vec<QuestionResponse>, content_type = "application/json")))]
pub async fn get_all(State(state): State<Arc<AppState>>) -> Json<Vec<QuestionResponse>> {
    let questions = state
        .config
        .packet
        .problems
        .iter()
        .map(|x| x.deref().into())
        .collect();

    Json(questions)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}",
    responses(
        (status=OK, body=QuestionResponse, content_type="application/json"),
        (status=404, description="Question Not Found"),
    )
)]
pub async fn get_specific_question(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(question): axum::extract::Path<usize>,
) -> Result<Json<QuestionResponse>, axum::http::StatusCode> {
    state
        .config
        .packet
        .problems
        .get(question)
        .map(|x| Json(x.deref().into()))
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_all))
        .routes(routes!(get_specific_question))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
