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

#[derive(Debug, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
pub struct TestResponse {
    pub input: String,
    pub output: String,
    pub visible: bool,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
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

#[derive(Debug, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
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
        (status=400, description="Invalid data provided"),
        (status=404, description="Question or language not found"),
        (status=409, description="Competition is paused"),
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
        return Err(StatusCode::CONFLICT);
    }

    if let Some(created) = crate::server::tester::run_test(
        state,
        body.language,
        question_index,
        body.solution,
        false,
        user.id,
    )
    .await
    {
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
        (status=400, description="Invalid data provided"),
        (status=404, description="Question or language not found"),
        (status=409, description="Competition is paused"),
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
        return Err(StatusCode::CONFLICT);
    }

    if let Some(created) = crate::server::tester::run_test(
        state,
        body.language,
        question_index,
        body.solution,
        true,
        user.id,
    )
    .await
    {
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
        (status=400, description="Invalid data provided"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
        (status=409, description="Competition is paused"),
    )
)]
#[axum::debug_handler]
pub async fn get_submission(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SubmissionState>, StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::CONFLICT);
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
        (status=400, description="Invalid data provided"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
        (status=409, description="Competition is paused"),
    )
)]
#[axum::debug_handler]
pub async fn get_test(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<SubmissionState>, StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::CONFLICT);
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
        (status=400, description="Invalid data provided"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
        (status=409, description="Competition is paused"),
    )
)]
#[axum::debug_handler]
pub async fn abort_submission(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<(), StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::CONFLICT);
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

#[utoipa::path(
    delete,
    path = "/{question_index}/tests/{test_id}", tag = "questions",
    responses(
        (status=OK),
        (status=400, description="Invalid data provided"),
        (status=403, description="Requesting user is not the creator of the submission or a host"),
        (status=404, description="Submission not found"),
        (status=409, description="Competition is paused"),
    )
)]
#[axum::debug_handler]
pub async fn abort_test(
    user: User,
    axum::extract::Path((_, id)): axum::extract::Path<(usize, SubmissionId)>,
    State(state): State<Arc<AppState>>,
) -> Result<(), StatusCode> {
    if state.is_paused().await {
        return Err(StatusCode::CONFLICT);
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

#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::{
        db_user, mock_state,
        repositories::submissions::{NewTestResults, SubmissionState, TestResultState},
        testing::setup_test_logger,
        user,
    };
    use bedrock::{
        language::{BuiltInLanguage, Version},
        packet::Packet,
    };

    use super::*;

    macro_rules! double_problem_packet {
        () => {
            Packet {
                title: "".into(),
                preamble: None,
                problems: vec![
                    Problem {
                        languages: None,
                        title: "problem1".into(),
                        description: None,
                        tests: vec![
                            Test {
                                input: "input1a".into(),
                                output: "output1a".into(),
                                visible: true,
                            },
                            Test {
                                input: "input1b".into(),
                                output: "output1b".into(),
                                visible: false,
                            },
                        ],
                        points: Some(10),
                    }
                    .into(),
                    Problem {
                        languages: None,
                        title: "problem2".into(),
                        description: None,
                        tests: vec![
                            Test {
                                input: "input2a".into(),
                                output: "output2a".into(),
                                visible: false,
                            },
                            Test {
                                input: "input2b".into(),
                                output: "output2b".into(),
                                visible: true,
                            },
                        ],
                        points: Some(10),
                    }
                    .into(),
                ],
            }
            .into()
        };
    }

    macro_rules! rust_language_set {
        () => {{
            let mut set = LanguageSet::new();
            set.insert(Language::BuiltIn {
                language: BuiltInLanguage::Rust,
                version: Version::Latest,
            });
            set.into()
        }};
    }
    macro_rules! sleep_language_set {
        () => {{
            let mut set = LanguageSet::new();
            set.insert(Language::Custom {
                raw_name: "sleep".into(),
                name: "sleep".into(),
                build: None,
                run: "sleep 10s".into(),
                source_file: "foo.sleep".into(),
                syntax: Default::default(),
            });
            set.into()
        }};
    }

    macro_rules! create_submission {
        ($db: expr, $submitter: expr, $test_only: literal) => {{
            let history = repositories::submissions::create_submission_history(
                $db,
                repositories::submissions::NewSubmissionHistory {
                    id: SubmissionId::new(),
                    submitter: $submitter,
                    code: "fn main() {}",
                    question_index: 0,
                    language: "rust",
                    compile_result: None,
                    test_only: $test_only,
                },
            )
            .await
            .unwrap()
            .finish($db, 10., true, 2, 2, Duration::from_secs(1))
            .await
            .unwrap();

            repositories::submissions::create_test_results(
                $db,
                &history.id,
                0,
                NewTestResults {
                    result: TestResultState::Pass,
                    stdout: "stdout".into(),
                    stderr: "stderr".into(),
                    exit_status: 0,
                    time_taken: Duration::from_secs(1).into(),
                },
            )
            .await
            .unwrap();
            history
        }};
    }

    #[tokio::test]
    async fn get_all_questions_competitor() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                ..Config::default()
            }
        );

        let Json(value) = get_all(user!("foobar", Competitor), State(state)).await;

        assert_eq!(
            value,
            vec![
                QuestionResponse {
                    languages: vec![],
                    title: "problem1".into(),
                    description: None,
                    tests: vec![TestResponse {
                        input: "input1a".into(),
                        output: "output1a".into(),
                        visible: true,
                    }],
                    points: Some(10)
                },
                QuestionResponse {
                    languages: vec![],
                    title: "problem2".into(),
                    description: None,
                    tests: vec![TestResponse {
                        input: "input2b".into(),
                        output: "output2b".into(),
                        visible: true,
                    }],
                    points: Some(10)
                }
            ]
        );
    }

    #[tokio::test]
    async fn get_all_questions_host() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                ..Config::default()
            }
        );
        let Json(value) = get_all(user!("foobar", Host), State(state)).await;

        assert_eq!(
            value,
            vec![
                QuestionResponse {
                    languages: vec![],
                    title: "problem1".into(),
                    description: None,
                    tests: vec![
                        TestResponse {
                            input: "input1a".into(),
                            output: "output1a".into(),
                            visible: true,
                        },
                        TestResponse {
                            input: "input1b".into(),
                            output: "output1b".into(),
                            visible: false,
                        },
                    ],
                    points: Some(10)
                },
                QuestionResponse {
                    languages: vec![],
                    title: "problem2".into(),
                    description: None,
                    tests: vec![
                        TestResponse {
                            input: "input2a".into(),
                            output: "output2a".into(),
                            visible: false,
                        },
                        TestResponse {
                            input: "input2b".into(),
                            output: "output2b".into(),
                            visible: true,
                        },
                    ],
                    points: Some(10)
                }
            ]
        );
    }

    #[tokio::test]
    async fn get_specific_question_host() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                ..Config::default()
            }
        );

        let Json(value) =
            get_specific_question(State(state), user!("foobar", Host), axum::extract::Path(1))
                .await
                .unwrap();

        assert_eq!(
            *value,
            QuestionResponse {
                languages: vec![],
                title: "problem2".into(),
                description: None,
                tests: vec![
                    TestResponse {
                        input: "input2a".into(),
                        output: "output2a".into(),
                        visible: false,
                    },
                    TestResponse {
                        input: "input2b".into(),
                        output: "output2b".into(),
                        visible: true,
                    },
                ],
                points: Some(10)
            }
        );
    }

    #[tokio::test]
    async fn get_specific_question_competitor() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                ..Config::default()
            }
        );

        let Json(value) = get_specific_question(
            State(state),
            user!("foobar", Competitor),
            axum::extract::Path(1),
        )
        .await
        .unwrap();

        assert_eq!(
            *value,
            QuestionResponse {
                languages: vec![],
                title: "problem2".into(),
                description: None,
                tests: vec![TestResponse {
                    input: "input2b".into(),
                    output: "output2b".into(),
                    visible: true,
                }],
                points: Some(10)
            }
        );
    }

    #[tokio::test]
    async fn get_specific_question_404() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                ..Config::default()
            }
        );

        let code = get_specific_question(
            State(state),
            user!("foobar", Competitor),
            axum::extract::Path(42),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_submission_valid() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);

        state.clock.write().await.unpause();

        const SOLUTION: &str = r#"
            fn main() {}
        "#;

        let (status, header, Json(created)) = create_submission(
            user,
            axum::extract::Path(0),
            State(state),
            Json(SubmissionBody {
                language: "rust".into(),
                solution: SOLUTION.into(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(status, StatusCode::CREATED);
        assert!(header.contains_key(axum::http::header::LOCATION));
        assert_eq!(
            header[axum::http::header::LOCATION].to_str().unwrap(),
            format!("/questions/0/submissions/{}", created.id)
        );
        assert!(matches!(created, CreatedSubmission { id: _, cases: 2 }));
    }

    #[tokio::test]
    async fn create_submission_404() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        state.clock.write().await.unpause();

        const SOLUTION: &str = r#"
            fn main() {}
        "#;

        let code = create_submission(
            user!("foobar", Competitor),
            axum::extract::Path(42),
            State(state),
            Json(SubmissionBody {
                language: "rust".into(),
                solution: SOLUTION.into(),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_submission_paused() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        state.clock.write().await.pause();

        const SOLUTION: &str = r#"
            fn main() {}
        "#;

        let code = create_submission(
            user!("foobar", Competitor),
            axum::extract::Path(0),
            State(state),
            Json(SubmissionBody {
                language: "rust".into(),
                solution: SOLUTION.into(),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn create_test_valid() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);

        state.clock.write().await.unpause();

        const SOLUTION: &str = r#"
            fn main() {}
        "#;

        let (status, header, Json(created)) = create_test(
            user,
            axum::extract::Path(0),
            State(state),
            Json(SubmissionBody {
                language: "rust".into(),
                solution: SOLUTION.into(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(status, StatusCode::CREATED);
        assert!(header.contains_key(axum::http::header::LOCATION));
        assert_eq!(
            header[axum::http::header::LOCATION].to_str().unwrap(),
            format!("/questions/0/tests/{}", created.id)
        );
        assert!(matches!(created, CreatedSubmission { id: _, cases: 1 }));
    }

    #[tokio::test]
    async fn create_test_404() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        state.clock.write().await.unpause();

        const SOLUTION: &str = r#"
            fn main() {}
        "#;

        let code = create_test(
            user!("foobar", Competitor),
            axum::extract::Path(42),
            State(state),
            Json(SubmissionBody {
                language: "rust".into(),
                solution: SOLUTION.into(),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn create_test_paused() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        state.clock.write().await.pause();

        const SOLUTION: &str = r#"
            fn main() {}
        "#;

        let code = create_test(
            user!("foobar", Competitor),
            axum::extract::Path(0),
            State(state),
            Json(SubmissionBody {
                language: "rust".into(),
                solution: SOLUTION.into(),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn get_submission_valid() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, false);
        state.clock.write().await.unpause();

        let Json(state) = get_submission(user, axum::extract::Path((0, history.id)), State(state))
            .await
            .unwrap();

        assert_eq!(state.submission_history, history);
        assert_eq!(
            state.test_results,
            vec![TestResults {
                submission: history.id,
                test_index: 0,
                result: TestResultState::Pass,
                stdout: "stdout".into(),
                stderr: "stderr".into(),
                exit_status: 0,
                time_taken: Duration::from_secs(1).into(),
            }]
        );
    }

    #[tokio::test]
    async fn get_submission_404() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let submission_id = SubmissionId::new();
        state.clock.write().await.unpause();

        let code = get_submission(
            user!("foobar", Competitor),
            axum::extract::Path((0, submission_id)),
            State(state),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_submission_test() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, true);

        state.clock.write().await.unpause();

        let code = get_submission(user, axum::extract::Path((0, history.id)), State(state))
            .await
            .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_submission_other_user() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, false);

        state.clock.write().await.unpause();

        let code = get_submission(
            user!("not_foobar", Competitor),
            axum::extract::Path((0, history.id)),
            State(state),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_submission_other_user_host() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, false);

        state.clock.write().await.unpause();

        let Json(state) = get_submission(
            user!("not_foobar", Host),
            axum::extract::Path((0, history.id)),
            State(state),
        )
        .await
        .unwrap();

        assert_eq!(state.submission_history, history);
        assert_eq!(
            state.test_results,
            vec![TestResults {
                submission: history.id,
                test_index: 0,
                result: TestResultState::Pass,
                stdout: "stdout".into(),
                stderr: "stderr".into(),
                exit_status: 0,
                time_taken: Duration::from_secs(1).into(),
            }]
        );
    }

    #[tokio::test]
    async fn get_test_valid() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, true);

        state.clock.write().await.unpause();

        let Json(state) = get_test(user, axum::extract::Path((0, history.id)), State(state))
            .await
            .unwrap();

        assert_eq!(state.submission_history, history);
        assert_eq!(
            state.test_results,
            vec![TestResults {
                submission: history.id,
                test_index: 0,
                result: TestResultState::Pass,
                stdout: "stdout".into(),
                stderr: "stderr".into(),
                exit_status: 0,
                time_taken: Duration::from_secs(1).into(),
            }]
        );
    }

    #[tokio::test]
    async fn get_test_404() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let submission_id = SubmissionId::new();
        state.clock.write().await.unpause();

        let code = get_test(
            user!("foobar", Competitor),
            axum::extract::Path((0, submission_id)),
            State(state),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_test_submission() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, false);

        state.clock.write().await.unpause();

        let code = get_test(user, axum::extract::Path((0, history.id)), State(state))
            .await
            .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_test_other_user() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, true);

        state.clock.write().await.unpause();

        let code = get_test(
            user!("not_foobar", Competitor),
            axum::extract::Path((0, history.id)),
            State(state),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_test_other_user_host() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: rust_language_set!(),
                ..Config::default()
            }
        );

        let user = db_user!(&state.db, "foobar", Competitor);
        let history = create_submission!(&state.db, user.id, true);

        state.clock.write().await.unpause();

        let Json(state) = get_test(
            user!("not_foobar", Host),
            axum::extract::Path((0, history.id)),
            State(state),
        )
        .await
        .unwrap();

        assert_eq!(state.submission_history, history);
        assert_eq!(
            state.test_results,
            vec![TestResults {
                submission: history.id,
                test_index: 0,
                result: TestResultState::Pass,
                stdout: "stdout".into(),
                stderr: "stderr".into(),
                exit_status: 0,
                time_taken: Duration::from_secs(1).into(),
            }]
        );
    }

    #[tokio::test]
    async fn abort_submission_valid() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_submission(
            user.clone(),
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        abort_submission(
            user,
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap();

        // sleep to ensure the abort gets completed
        tokio::time::sleep(Duration::from_secs(1)).await;

        let submission = repositories::submissions::get_submission(&state.db, submission.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(submission.state, SubmissionState::Cancelled);
    }

    #[tokio::test]
    async fn abort_submission_404() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let code = abort_submission(
            user,
            axum::extract::Path((0, SubmissionId::new())),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn abort_submission_test() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_test(
            user.clone(),
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        let code = abort_submission(
            user,
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn abort_submission_other_user() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_submission(
            user.clone(),
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        let code = abort_submission(
            user!("not_foobar", Competitor),
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn abort_submission_other_user_host() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_submission(
            user.clone(),
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        abort_submission(
            user!("not_foobar", Host),
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap();

        // sleep to ensure the abort gets completed
        tokio::time::sleep(Duration::from_secs(1)).await;

        let submission = repositories::submissions::get_submission(&state.db, submission.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(submission.state, SubmissionState::Cancelled);
    }

    #[tokio::test]
    async fn abort_test_valid() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_test(
            user.clone(),
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        abort_test(
            user,
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap();

        // sleep to ensure the abort gets completed
        tokio::time::sleep(Duration::from_secs(1)).await;

        let submission = repositories::submissions::get_submission(&state.db, submission.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(submission.state, SubmissionState::Cancelled);
    }

    #[tokio::test]
    async fn abort_test_404() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let code = abort_test(
            user,
            axum::extract::Path((0, SubmissionId::new())),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn abort_test_submission() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_submission(
            user.clone(),
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        let code = abort_test(
            user,
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn abort_test_other_user() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_test(
            user,
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        let code = abort_test(
            user!("not_foobar", Competitor),
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap_err();

        assert_eq!(code, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn abort_test_other_user_host() {
        setup_test_logger();

        mock_state!(
            let state;
            Config {
                packet: double_problem_packet!(),
                languages: sleep_language_set!(),
                ..Config::default()
            }
        );
        state.clock.write().await.unpause();

        let user = db_user!(&state.db, "foobar", Competitor);

        let (_, _, Json(submission)) = create_test(
            user,
            axum::extract::Path(0),
            State(Arc::clone(&state)),
            Json(SubmissionBody {
                language: "sleep".into(),
                solution: "".into(),
            }),
        )
        .await
        .unwrap();

        abort_test(
            user!("not_foobar", Host),
            axum::extract::Path((0, submission.id)),
            State(Arc::clone(&state)),
        )
        .await
        .unwrap();

        // sleep to ensure the abort gets completed
        tokio::time::sleep(Duration::from_secs(1)).await;

        let submission = repositories::submissions::get_submission(&state.db, submission.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(submission.state, SubmissionState::Cancelled);
    }
}
