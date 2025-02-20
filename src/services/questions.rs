use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::AppState;

mod questions {
    use std::{collections::HashSet, ops::Deref, sync::Arc};

    use crate::server::AppState;
    use axum::{extract::State, Json};
    use bedrock::packet::{Problem, Test};

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
        languages: Option<HashSet<String>>,
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

    #[derive(serde::Serialize, utoipa::ToSchema)]
    pub struct AllQuestionResponse(Vec<QuestionResponse>);

    #[derive(serde::Serialize, utoipa::ToSchema)]
    pub struct SpecificQuestionResponse(QuestionResponse);

    #[axum::debug_handler]
    #[utoipa::path(get, path = "", responses((status = OK, body = AllQuestionResponse, content_type = "application/json")))]
    pub async fn get_all(State(state): State<Arc<AppState>>) -> Json<AllQuestionResponse> {
        let questions = state
            .config
            .packet
            .problems
            .iter()
            .map(|x| x.deref().into())
            .collect();

        Json(AllQuestionResponse(questions))
    }

    #[axum::debug_handler]
    #[utoipa::path(get, path = "/{id}", responses((status = OK, body = SpecificQuestionResponse, content_type = "application/json")))]
    pub async fn get_specific_question(
        State(state): State<Arc<AppState>>,
        axum::extract::Path(question): axum::extract::Path<usize>,
    ) -> Result<Json<SpecificQuestionResponse>, axum::http::StatusCode> {
        state
            .config
            .packet
            .problems
            .get(question)
            .map(|x| Json(SpecificQuestionResponse(x.deref().into())))
            .ok_or(axum::http::StatusCode::NOT_FOUND)
    }
}

pub fn question_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(questions::get_all))
        .routes(routes!(questions::get_specific_question))
}

pub fn question_service() -> axum::Router<Arc<AppState>> {
    question_router().split_for_parts().0
}
