use crate::{extractors::auth::OptionalAuthUser, repositories::users::Role, server::AppState};
use axum::{extract::State, Json};
use bedrock::{
    language::LanguageSet,
    packet::{Problem, Test},
};
use std::collections::BTreeSet;
use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TestResponse {
    pub input: String,
    pub output: String,
    pub visible: bool,
}

impl From<&Test> for TestResponse {
    fn from(value: &Test) -> Self {
        Self {
            input: value.input.clone(),
            output: value.output.clone(),
            visible: value.visible,
        }
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct QuestionResponse {
    languages: BTreeSet<String>,
    title: String,
    description: Option<String>,
    tests: Vec<TestResponse>,
    points: Option<i32>,
}

impl QuestionResponse {
    fn from(value: &Problem, languages: &LanguageSet, show_hidden: bool) -> Self {
        Self {
            languages: value
                .languages
                .clone()
                .unwrap_or_else(|| languages.iter().map(|l| l.name().to_string()).collect()),
            title: value.title.clone(),
            description: value.description.as_ref().map(|x| x.html().unwrap()),
            tests: value
                .tests
                .iter()
                .map(TestResponse::from)
                .filter(|t| show_hidden || t.visible)
                .collect(),
            points: value.points,
        }
    }
}

#[axum::debug_handler]
#[utoipa::path(get, path = "/", responses((status = OK, body = Vec<QuestionResponse>, content_type = "application/json")))]
pub async fn get_all(
    OptionalAuthUser(user): OptionalAuthUser,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<QuestionResponse>> {
    let show_hidden = user.is_some_and(|u| matches!(u.user.role, Role::Host));
    let questions = state
        .config
        .packet
        .problems
        .iter()
        .map(|x| QuestionResponse::from(x, &state.config.languages, show_hidden))
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
    OptionalAuthUser(user): OptionalAuthUser,
    axum::extract::Path(question): axum::extract::Path<usize>,
) -> Result<Json<QuestionResponse>, axum::http::StatusCode> {
    state
        .config
        .packet
        .problems
        .get(question)
        .map(|x| {
            Json(QuestionResponse::from(
                x,
                &state.config.languages,
                user.is_some_and(|u| matches!(u.user.role, Role::Host)),
            ))
        })
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
