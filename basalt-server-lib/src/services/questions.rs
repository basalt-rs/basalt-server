use crate::{
    extractors::auth::OptionalUser,
    repositories::{
        self,
        submissions::{SubmissionHistory, SubmissionId, TestResults},
        users::{Role, User},
    },
    server::{tester::CreatedSubmission, AppState},
};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    Json,
};
use bedrock::{
    language::{Language, LanguageSet, Syntax},
    packet::{Problem, Test},
    Config,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::error;
use utoipa::ToSchema;
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
) -> Result<Json<&'static QuestionResponse>, StatusCode> {
    let show_hidden = user.is_some_and(|u| matches!(u.role, Role::Host));
    get_or_init_questions(&state.config, show_hidden)
        .await
        .get(question)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmissionBody {
    language: String,
    solution: String,
}

#[utoipa::path(
    post,
    path = "/{question_index}/submissions", tag = "questions",
    request_body = SubmissionBody,
    responses(
        (status=OK, body=String, content_type="text/plain", description="The ID of the submission", headers(("Location"))),
        (status=400, description="Competition is paused"),
        (status=404, description="Question or language not found"),
    )
)]
#[axum::debug_handler]
pub async fn create_submission(
    user: User,
    axum::extract::Path(question_index): axum::extract::Path<usize>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubmissionBody>,
) -> Result<(StatusCode, HeaderMap, Json<CreatedSubmission>), StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Some(created) = crate::server::tester::run_test(
        state,
        body.language,
        question_index,
        body.solution,
        false,
        user.id,
    ) {
        let location = HeaderValue::from_str(&format!(
            "/questions/{}/submissions/{}",
            question_index, created.id
        ))
        .expect("String is visible ASCII codes");
        Ok((
            StatusCode::CREATED,
            HeaderMap::from_iter([(axum::http::header::LOCATION, location)]),
            Json(created),
        ))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    post,
    path = "/{question_index}/tests", tag = "questions",
    request_body = SubmissionBody,
    responses(
        (status=OK, body=String, content_type="text/plain", description="The ID of the submission", headers(("Location"))),
        (status=400, description="Competition is paused"),
        (status=404, description="Question or language not found"),
    )
)]
#[axum::debug_handler]
pub async fn create_test(
    user: User,
    axum::extract::Path(question_index): axum::extract::Path<usize>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubmissionBody>,
) -> Result<(StatusCode, HeaderMap, Json<CreatedSubmission>), StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::BAD_REQUEST);
    }

    if let Some(created) = crate::server::tester::run_test(
        state,
        body.language,
        question_index,
        body.solution,
        true,
        user.id,
    ) {
        let location = HeaderValue::from_str(&format!(
            "/questions/{}/tests/{}",
            question_index, created.id
        ))
        .expect("String is visible ASCII codes");
        Ok((
            StatusCode::CREATED,
            HeaderMap::from_iter([(axum::http::header::LOCATION, location)]),
            Json(created),
        ))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionState {
    #[serde(flatten)]
    submission_history: SubmissionHistory,
    test_results: Vec<TestResults>,
}

#[utoipa::path(
    get,
    path = "/{question_index}/submissions/{submission_id}", tag = "questions",
    responses(
        (status=OK, body=SubmissionState, content_type="application/json"),
        (status=400, description="Competition is paused"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
    )
)]
#[axum::debug_handler]
pub async fn get_submission(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SubmissionState>, StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::BAD_REQUEST);
    }

    let join = tokio::try_join!(
        repositories::submissions::get_submission(&state.db, id),
        repositories::submissions::get_test_results(&state.db, id),
    );

    let (submission, test_results) = match join {
        Ok(x) => x,
        Err(error) => {
            error!(?error, "Error getting submission state");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if let Some(submission) = submission {
        if user.id != submission.submitter && user.role != Role::Host {
            return Err(StatusCode::FORBIDDEN);
        }

        if submission.test_only {
            return Err(StatusCode::NOT_FOUND);
        }

        Ok(Json(SubmissionState {
            submission_history: submission,
            test_results,
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    get,
    path = "/{question_index}/tests/{test_id}", tag = "questions",
    responses(
        (status=OK, body=SubmissionState, content_type="application/json"),
        (status=400, description="Competition is paused"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
    )
)]
#[axum::debug_handler]
pub async fn get_test(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SubmissionState>, StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::BAD_REQUEST);
    }

    let join = tokio::try_join!(
        repositories::submissions::get_submission(&state.db, id),
        repositories::submissions::get_test_results(&state.db, id),
    );

    let (submission, test_results) = match join {
        Ok(x) => x,
        Err(error) => {
            error!(?error, "Error getting submission state");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if let Some(submission) = submission {
        if user.id != submission.submitter && user.role != Role::Host {
            return Err(StatusCode::FORBIDDEN);
        }

        if !submission.test_only {
            return Err(StatusCode::NOT_FOUND);
        }

        Ok(Json(SubmissionState {
            submission_history: submission,
            test_results,
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    delete,
    path = "/{question_index}/submissions/{submission_id}", tag = "questions",
    responses(
        (status=OK),
        (status=400, description="Competition is paused"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
    )
)]
#[axum::debug_handler]
pub async fn abort_submission(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<(), StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::BAD_REQUEST);
    }

    let Some(submission) = repositories::submissions::get_submission(&state.db, id)
        .await
        .map_err(|error| {
            error!(?error, ?id, "Error while getting submission");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    if submission.test_only {
        return Err(StatusCode::NOT_FOUND);
    }

    if user.id != submission.submitter && user.role != Role::Host {
        return Err(StatusCode::FORBIDDEN);
    }

    if state.tester.abort(id) {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

#[utoipa::path(
    delete,
    path = "/{question_index}/tests/{test_id}", tag = "questions",
    responses(
        (status=OK),
        (status=400, description="Competition is paused"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
    )
)]
#[axum::debug_handler]
pub async fn abort_test(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<(), StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::BAD_REQUEST);
    }

    let Some(submission) = repositories::submissions::get_submission(&state.db, id)
        .await
        .map_err(|error| {
            error!(?error, ?id, "Error while getting submission");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    else {
        return Err(StatusCode::NOT_FOUND);
    };

    tracing::debug!(?submission, "got submission");
    if !submission.test_only {
        return Err(StatusCode::NOT_FOUND);
    }

    if user.id != submission.submitter && user.role != Role::Host {
        return Err(StatusCode::FORBIDDEN);
    }

    if state.tester.abort(id) {
        repositories::submissions::PartialSubmissionHistory(submission)
            .cancel(&state.db)
            .await
            .map_err(|error| {
                error!(?error, ?id, "Error while marking submission cancelled");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(get_all))
        .routes(routes!(get_specific_question))
        .routes(routes!(create_submission))
        .routes(routes!(create_test))
        .routes(routes!(get_submission))
        .routes(routes!(get_test))
        .routes(routes!(abort_submission))
        .routes(routes!(abort_test))
}

pub fn service() -> axum::Router<Arc<AppState>> {
    router().split_for_parts().0
}
