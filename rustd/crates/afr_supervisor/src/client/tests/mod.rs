//! A shared daemon seam, with routing, retry and real transport behavior separate.
#![expect(clippy::unwrap_used, reason = "test fixtures must fail loudly")]

mod authoring;
mod retry;
mod routing;
mod transport;

use super::{Call, HttpRunnerApi, RunnerApi, Verb};
use crate::config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN};
use axum::body::Bytes as AxumBytes;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use bytes::Bytes;
use tokio::sync::mpsc;

/// What the test daemon saw of one request.
#[derive(Debug)]
struct Seen {
    method: Method,
    path: String,
    authorization: Option<String>,
    content_type: Option<String>,
    body: AxumBytes,
}

/// A daemon that answers `/status/{code}` with that status and a problem body,
/// and everything else with 200 and the body it was sent.
async fn daemon() -> (String, mpsc::UnboundedReceiver<Seen>) {
    let (seen, received) = mpsc::unbounded_channel();
    let app = axum::Router::new().fallback(
        move |method: Method, uri: Uri, headers: HeaderMap, body: AxumBytes| {
            let seen = seen.clone();
            async move {
                let header = |name| {
                    headers
                        .get(name)
                        .map(|value: &axum::http::HeaderValue| value.to_str().unwrap().to_owned())
                };
                let path = uri.path().to_owned();
                let status = path
                    .strip_prefix("/status/")
                    .map_or(StatusCode::OK, |code| {
                        StatusCode::from_u16(code.parse().unwrap()).unwrap()
                    });
                let reply = if status.is_success() {
                    body.clone()
                } else {
                    AxumBytes::from_static(br#"{"error_code":"UZ-RUN-011"}"#)
                };
                seen.send(Seen {
                    method,
                    path,
                    authorization: header("authorization"),
                    content_type: header("content-type"),
                    body,
                })
                .unwrap();
                (status, reply)
            }
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}/"), received)
}

fn api(base: &str, token: &str) -> crate::Result<HttpRunnerApi> {
    let env = afd_core::env::MapEnv::from_pairs([(ENV_API_URL, base), (ENV_RUNNER_TOKEN, token)]);
    HttpRunnerApi::new(&Config::from_env(&env)?)
}

fn call(verb: Verb, path: &str, body: Option<&'static [u8]>) -> Call {
    Call {
        verb,
        path: path.to_owned().into(),
        body: body.map(Bytes::from_static),
    }
}
