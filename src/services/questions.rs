use std::sync::Arc;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::server::AppState;

mod questions {
    use std::{collections::HashSet, ops::Deref, sync::Arc};

    use axum::{extract::State, response::Response, Json};
    use bedrock::packet::{Problem, Test};

    use crate::server::AppState;

    #[derive(serde::Deserialize, utoipa::ToSchema)]
    pub struct QuestionBody {
        question: usize,
    }

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

    #[axum::debug_handler]
    #[utoipa::path(get, path = "/all", responses((status = OK, body = AllQuestionResponse, content_type = "application/json")))]
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
}

pub fn question_router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new().routes(routes!(questions::get_all))
}

pub fn question_service() -> axum::Router<Arc<AppState>> {
    question_router().split_for_parts().0
}
