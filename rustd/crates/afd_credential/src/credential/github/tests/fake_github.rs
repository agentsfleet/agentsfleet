//! A loopback GitHub, answering the installation read and the token request
//! each from a script.
//!
//! On axum rather than a raw socket, so each call is routed by a real HTTP
//! server — a request to any other path is a 404 and fails the case — and the
//! token request's body is recorded as parsed JSON. Shared by [`super::transport`]
//! and [`super::signed`], which drive it unsigned and signed.
#![expect(
    clippy::expect_used,
    reason = "the loopback server and client are transport fixture preconditions"
)]

use std::sync::{Arc, Mutex};

use afd_fleet_runtime::config::{Reach, RepositoryBinding};
use axum::Router;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use jiff::Timestamp;
use serde_json::Value;
use tokio::task::JoinHandle;
use url::Url;
use zeroize::Zeroizing;

use super::super::api::AppApi;
use super::super::exchange::request_token;
use crate::credential::outcome::Outcome;

pub(super) const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);
/// An hour past [`NOW`], the installation token's local ceiling.
pub(super) const IN_AN_HOUR: Timestamp = Timestamp::constant(1_760_003_600, 0);
pub(super) const INSTALLATION_ID: u64 = 42;
const INSTALLATION: &str = "/app/installations/42";
const ACCESS_TOKENS: &str = "/app/installations/42/access_tokens";
const MEDIA_JSON: &str = "application/json";

/// An installation holding every evidence read.
pub(super) const HOLDS_EVERYTHING: &str =
    r#"{"permissions":{"contents":"read","actions":"read","checks":"read","metadata":"read"}}"#;
/// The platform App as registered today: no Checks permission.
pub(super) const HOLDS_NO_CHECKS: &str = r#"{"permissions":{"contents":"read","actions":"read","deployments":"read","metadata":"read","pull_requests":"write"}}"#;

/// One scripted answer: a status and its JSON body.
pub(super) type Answer = (u16, &'static str);

pub(super) const OK: u16 = 200;
pub(super) const CREATED: u16 = 201;
/// GitHub's answer when the installation is gone.
pub(super) const GONE: &str = r#"{"message":"gone"}"#;
/// GitHub's answer when it is briefly unavailable.
pub(super) const LATER: &str = r#"{"message":"later"}"#;

/// A loopback GitHub answering the installation read and the token request.
pub(super) struct FakeGithub {
    pub(super) base: String,
    asked: Arc<Mutex<Vec<Value>>>,
    /// The `Authorization` header of every call, in order.
    bearers: Arc<Mutex<Vec<String>>>,
    http: reqwest::Client,
    handle: JoinHandle<()>,
}

impl FakeGithub {
    pub(super) async fn answering(installation: Answer, token: Answer) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("the listener is bound")
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        let bearers = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&asked);
        let (read_by, posted_by) = (Arc::clone(&bearers), Arc::clone(&bearers));
        let router = Router::new()
            .route(
                INSTALLATION,
                get(move |headers: HeaderMap| {
                    record(&read_by, authorization(&headers));
                    async move { reply(installation) }
                }),
            )
            .route(
                ACCESS_TOKENS,
                post(move |headers: HeaderMap, sent: String| {
                    record(&posted_by, authorization(&headers));
                    record(
                        &recorded,
                        serde_json::from_str(&sent).unwrap_or(Value::Null),
                    );
                    async move { reply(token) }
                }),
            );
        let handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("the fake GitHub serves until aborted");
        });
        Self {
            base,
            asked,
            bearers,
            http: loopback_client(),
            handle,
        }
    }

    pub(super) async fn mint(&self) -> Outcome {
        let api = AppApi::new(
            &self.http,
            Url::parse(&self.base).expect("the loopback base is a URL"),
            Zeroizing::new(UNSIGNED.to_owned()),
        );
        request_token(&api, INSTALLATION_ID, &binding(), NOW).await
    }

    /// The `Authorization` header of every call, in order.
    pub(super) fn bearers(&self) -> Vec<String> {
        self.bearers
            .lock()
            .expect("no test holds this lock across a panic")
            .clone()
    }

    /// The permissions of every token request, in order. Empty when the mint
    /// stopped at the installation read.
    pub(super) fn asked_permissions(&self) -> Vec<Value> {
        self.asked
            .lock()
            .expect("no test holds this lock across a panic")
            .iter()
            .map(|body| body["permissions"].clone())
            .collect()
    }
}

impl Drop for FakeGithub {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// The bearer an unsigned fixture client presents.
const UNSIGNED: &str = "unsigned";

/// A client for the loopback GitHub each test stands up.
#[expect(
    clippy::disallowed_methods,
    reason = "the client dials only the loopback GitHub each test stands up"
)]
pub(super) fn loopback_client() -> reqwest::Client {
    reqwest::Client::new()
}

fn record<T>(into: &Mutex<Vec<T>>, value: T) {
    into.lock()
        .expect("no test holds this lock across a panic")
        .push(value);
}

fn authorization(headers: &HeaderMap) -> String {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

fn reply((status, body): Answer) -> Response {
    let status = StatusCode::from_u16(status).expect("a scripted status is valid");
    (status, [(CONTENT_TYPE, MEDIA_JSON)], body).into_response()
}

pub(super) fn binding() -> RepositoryBinding {
    RepositoryBinding::from_parts(vec!["acme/widgets".into()], Reach::Read)
}
