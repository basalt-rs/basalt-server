use crate::{extractors::auth::OptionalUser, repositories::users::Role, server::AppState};
use axum::{extract::State, Json};
use bedrock::{
    language::{Language, LanguageSet, Syntax},
    packet::{Problem, Test},
    Config,
};
use std::sync::Arc;
use tokio::sync::OnceCell;
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TestResponse {
    pub input: String,
    pub output: String,
    pub visible: bool,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct LanguageSyntax {
    name: String,
    #[schema(value_type = String)]
    syntax: Syntax,
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

impl From<&Language> for LanguageSyntax {
    fn from(value: &Language) -> Self {
        Self {
            name: value.name().to_string(),
            syntax: value.syntax(),
        }
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct QuestionResponse {
    languages: Vec<LanguageSyntax>,
    title: String,
    description: Option<String>,
    tests: Vec<TestResponse>,
    points: Option<i32>,
}

impl QuestionResponse {
    fn from(
        value: &Problem,
        languages: &LanguageSet,
        default_points: Option<i32>,
        show_hidden: bool,
    ) -> Self {
        Self {
            languages: value
                .languages
                .as_ref()
                .map(|p| {
                    p.iter()
                        .map(|l| languages.get_by_str(l).unwrap().into())
                        .collect()
                })
                .unwrap_or_else(|| languages.iter().map(LanguageSyntax::from).collect()),
            title: value.title.clone(),
            description: value.description.as_ref().map(|x| x.html().unwrap()),
            tests: value
                .tests
                .iter()
                .map(TestResponse::from)
                .filter(|t| show_hidden || t.visible)
                .collect(),
            points: value.points.or(default_points),
        }
    }
}

// Questions with test cases hidden
static QUESTIONS_VISIBLE: OnceCell<Vec<QuestionResponse>> = OnceCell::const_new();
// Questions with all test cases
static QUESTIONS_FULL: OnceCell<Vec<QuestionResponse>> = OnceCell::const_new();

pub async fn get_or_init_questions(
    config: &Config,
    show_hidden: bool,
) -> &'static [QuestionResponse] {
    let questions = if show_hidden {
        &QUESTIONS_FULL
    } else {
        &QUESTIONS_VISIBLE
    };

    questions
        .get_or_init(|| async {
            config
                .packet
                .problems
                .iter()
                .map(|x| {
                    QuestionResponse::from(
                        x,
                        &config.languages,
                        match &config.game {
                            bedrock::Game::Points(x) => Some(x.question_point_value),
                            bedrock::Game::Race(_) => None,
                        },
                        show_hidden,
                    )
                })
                .collect::<Vec<_>>()
        })
        .await
}

#[axum::debug_handler]
#[utoipa::path(get, tag = "questions", path = "/", responses((status = OK, body = &[QuestionResponse], content_type = "application/json")))]
pub async fn get_all(
    OptionalUser(user): OptionalUser,
    State(state): State<Arc<AppState>>,
) -> Json<&'static [QuestionResponse]> {
    let show_hidden = user.is_some_and(|u| matches!(u.role, Role::Host));
    let questions = get_or_init_questions(&state.config, show_hidden).await;

    Json(questions)
}

#[axum::debug_handler]
#[utoipa::path(
    get,
    path = "/{id}", tag = "questions",
    responses(
        (status=OK, body=QuestionResponse, content_type="application/json"),
        (status=404, description="Question Not Found"),
    )
)]
pub async fn get_specific_question(
    State(state): State<Arc<AppState>>,
    OptionalUser(user): OptionalUser,
    axum::extract::Path(question): axum::extract::Path<usize>,
) -> Result<Json<&'static QuestionResponse>, axum::http::StatusCode> {
    let show_hidden = user.is_some_and(|u| matches!(u.role, Role::Host));
    get_or_init_questions(&state.config, show_hidden)
        .await
        .get(question)
        .map(Json)
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
