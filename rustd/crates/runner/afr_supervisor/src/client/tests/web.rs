//! Single-send calls through the production HTTP transport.
use std::time::Duration;

use af_core::id::Uuid7;
use axum::body::{Body, Bytes};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse as _, Response};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::super::{ControlPlane, HttpRunnerApi, Verb};
use crate::config::{CALL_TIMEOUT_MIN_SECONDS, Config, ENV_API_URL, ENV_RUNNER_TOKEN, Tuning};
use crate::test_support::{Answer, RUN_ID, plane_with_http};

#[derive(Clone, Copy)]
enum AnswerAs {
    Success,
    Stalled,
    LostBody,
    Problem,
}

async fn fake(
    answer: AnswerAs,
) -> (
    ControlPlane,
    mpsc::UnboundedReceiver<(String, Value)>,
    tokio::task::JoinHandle<()>,
) {
    let (sent, seen) = mpsc::unbounded_channel();
    let app = axum::Router::new().fallback(move |method: Method, uri: Uri, bytes: Bytes| {
        let sent = sent.clone();
        async move {
            assert_eq!(method, Method::POST);
            sent.send((
                uri.path().to_owned(),
                serde_json::from_slice(&bytes).unwrap(),
            ))
            .unwrap();
            match answer {
                AnswerAs::Success => {
                    let url = "https://example.org/guide";
                    let reply = if uri.path().ends_with("/searches") {
                        json!({"vendor":"exa","results":[{"url":url,"excerpts":["the guide"]}]})
                    } else {
                        json!({"vendor":"parallel","url":url,"content":"# Guide"})
                    };
                    axum::Json(reply).into_response()
                }
                AnswerAs::Stalled => std::future::pending::<Response>().await,
                AnswerAs::LostBody => {
                    let broken = futures_util::stream::once(async {
                        Err::<Bytes, _>(std::io::Error::from(std::io::ErrorKind::ConnectionReset))
                    });
                    Body::from_stream(broken).into_response()
                }
                AnswerAs::Problem => (
                    StatusCode::BAD_GATEWAY,
                    axum::Json(
                        json!({"error_code":af_core::error_code::WEB_EXTRACT_UNAVAILABLE,
                        "local_read_permitted":true}),
                    ),
                )
                    .into_response(),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (plane(&base), seen, worker)
}

fn plane(base: &str) -> ControlPlane {
    let env = af_core::env::MapEnv::from_pairs([
        (ENV_API_URL, base),
        (ENV_RUNNER_TOKEN, "agt_r_web_test"),
        (crate::config::ENV_STREAM_URL, "https://localhost:4443"),
    ]);
    let tuning = Tuning {
        call_timeout: Duration::from_secs(CALL_TIMEOUT_MIN_SECONDS),
        ..Tuning::DEFAULT
    };
    let config = Config::from_env(&env).unwrap().with_tuning(tuning);
    let http = HttpRunnerApi::new(&config).unwrap();
    let (plane, _native) = plane_with_http(Box::new(http), |_| {
        Answer::Fail(crate::error::config("web verb used native stream"))
    });
    plane.with_tuning(tuning)
}

#[tokio::test]
async fn a_stalled_vendor_is_dispatched_once_and_answers_inside_the_deadline() {
    let (plane, mut seen, worker) = fake(AnswerAs::Stalled).await;
    let timeout = plane.tuning.call_timeout;
    assert!(af_wire::web::WEB_DEADLINE < timeout);
    let call = tokio::spawn(async move {
        plane
            .web_search(&Uuid7::parse(RUN_ID).unwrap(), 7, 19, "rust")
            .await
    });
    let (path, body) = seen.recv().await.unwrap();
    assert_eq!(path, format!("/v1/runners/me/runs/{RUN_ID}/searches"));
    assert_eq!(
        body,
        json!({"fencing_token":7,"call_number":19,"query":"rust"})
    );
    tokio::time::pause();
    tokio::time::advance(timeout).await;
    let timeout = call.await.unwrap().unwrap_err();
    assert!(timeout.is_retryable());
    assert!(!timeout.local_read_permitted());
    assert!(seen.try_recv().is_err());
    worker.abort();
}

#[tokio::test]
async fn a_lost_extract_reply_and_a_502_each_send_only_once() {
    let id = Uuid7::parse(RUN_ID).unwrap();
    let url = "https://example.org/guide";
    for answer in [AnswerAs::LostBody, AnswerAs::Problem] {
        let (plane, mut seen, worker) = fake(answer).await;
        let failure = plane.web_extract(&id, 7, 20, url).await.unwrap_err();
        assert_eq!(
            failure.local_read_permitted(),
            matches!(answer, AnswerAs::Problem)
        );
        let (path, body) = seen.try_recv().unwrap();
        assert_eq!(path, format!("/v1/runners/me/runs/{RUN_ID}/extracts"));
        assert_eq!(body, json!({"fencing_token":7,"call_number":20,"url":url}));
        assert!(seen.try_recv().is_err());
        worker.abort();
    }
}

#[test]
fn only_an_explicit_extract_problem_authorizes_one_local_read() {
    let permitted = af_core::error_code::WEB_EXTRACT_UNAVAILABLE;
    for (verb, status, code, flag) in [
        (Verb::WebSearch, 502, permitted, json!(true)),
        (Verb::WebExtract, 504, permitted, json!(true)),
        (Verb::WebExtract, 502, permitted, json!(false)),
        (Verb::WebExtract, 502, permitted, json!("true")),
        (
            Verb::WebExtract,
            503,
            af_core::error_code::WEB_RESERVATION_UNAVAILABLE,
            json!(true),
        ),
    ] {
        let body =
            serde_json::to_vec(&json!({"error_code":code,"local_read_permitted":flag})).unwrap();
        assert!(!crate::error::unavailable_with_body(verb, status, &body).local_read_permitted());
    }
    for body in [&b"Bad Gateway"[..], br#"{"error_code":"UZ-WEB-009"}"#] {
        assert!(
            !crate::error::unavailable_with_body(Verb::WebExtract, 503, body)
                .local_read_permitted()
        );
    }
}

#[tokio::test]
async fn one_search_and_one_extract_return_typed_replies() {
    let (plane, mut seen, worker) = fake(AnswerAs::Success).await;
    let id = Uuid7::parse(RUN_ID).unwrap();
    let search = plane.web_search(&id, 7, 1, "rust").await.unwrap();
    assert_eq!(search.vendor, "exa");
    assert_eq!(search.results.len(), 1);
    let page = search.results.first().unwrap();
    let extract = plane.web_extract(&id, 7, 2, &page.url).await.unwrap();
    assert_eq!(extract.vendor, "parallel");
    assert_eq!(extract.content, "# Guide");
    assert!(page.title.is_none() && extract.title.is_none());
    assert!(seen.try_recv().is_ok());
    assert!(seen.try_recv().is_ok());
    assert!(seen.try_recv().is_err());
    worker.abort();
}
