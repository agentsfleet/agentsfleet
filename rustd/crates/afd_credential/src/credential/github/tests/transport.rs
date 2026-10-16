//! The exchange against a loopback GitHub: the installation read, then the
//! token request, each answered from a script.
//!
//! On axum rather than a raw socket, so each call is routed by a real HTTP
//! server — a request to any other path is a 404 and fails the case — and the
//! token request's body is asserted as parsed JSON.
#![expect(
    clippy::expect_used,
    reason = "the loopback server and client are transport fixture preconditions"
)]

use std::sync::{Arc, Mutex};

use afd_core::test_util::trace::{Capture, CapturedEvent};
use afd_fleet_runtime::config::{Reach, RepositoryBinding};
use axum::Router;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post};
use octocrab::Octocrab;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tracing::Level;

use super::super::exchange::{Exchange, mint_at, request_token};
use crate::credential::outcome::{Outcome, Retry};
use crate::credential::platform::GithubApp;

const NOW_MS: i64 = 1_760_000_000_000;
const INSTALLATION_ID: u64 = 42;
const INSTALLATION: &str = "/app/installations/42";
const ACCESS_TOKENS: &str = "/app/installations/42/access_tokens";
const MEDIA_JSON: &str = "application/json";

/// An installation holding every evidence read.
const HOLDS_EVERYTHING: &str =
    r#"{"permissions":{"contents":"read","actions":"read","checks":"read","metadata":"read"}}"#;
/// The platform App as registered today: no Checks permission.
const HOLDS_NO_CHECKS: &str = r#"{"permissions":{"contents":"read","actions":"read","deployments":"read","metadata":"read","pull_requests":"write"}}"#;

/// One scripted answer: a status and its JSON body.
type Answer = (u16, &'static str);

const OK: u16 = 200;
const CREATED: u16 = 201;
/// GitHub's answer when the installation is gone.
const GONE: &str = r#"{"message":"gone"}"#;
/// GitHub's answer when it is briefly unavailable.
const LATER: &str = r#"{"message":"later"}"#;

/// A loopback GitHub answering the installation read and the token request.
struct FakeGithub {
    base: String,
    asked: Arc<Mutex<Vec<Value>>>,
    handle: JoinHandle<()>,
}

impl FakeGithub {
    async fn answering(installation: Answer, token: Answer) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let base = format!(
            "http://{}",
            listener.local_addr().expect("the listener is bound")
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&asked);
        let router = Router::new()
            .route(
                INSTALLATION,
                get(move || async move { reply(installation) }),
            )
            .route(
                ACCESS_TOKENS,
                post(move |sent: String| {
                    recorded
                        .lock()
                        .expect("no test holds this lock across a panic")
                        .push(serde_json::from_str(&sent).unwrap_or(Value::Null));
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
            handle,
        }
    }

    async fn mint(&self) -> Outcome {
        let client = Octocrab::builder()
            .base_uri(self.base.as_str())
            .expect("the loopback base is a URI")
            .build()
            .expect("the fixture client builds");
        request_token(&client, INSTALLATION_ID, &binding(), NOW_MS).await
    }

    /// The permissions of every token request, in order. Empty when the mint
    /// stopped at the installation read.
    fn asked_permissions(&self) -> Vec<Value> {
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

fn reply((status, body): Answer) -> Response {
    let status = StatusCode::from_u16(status).expect("a scripted status is valid");
    (status, [(CONTENT_TYPE, MEDIA_JSON)], body).into_response()
}

fn binding() -> RepositoryBinding {
    RepositoryBinding::from_parts(vec!["acme/widgets".into()], Reach::Read)
}

#[tokio::test]
async fn a_narrowed_response_is_delivered_with_the_local_expiry_ceiling() {
    let token = r#"{"token":"ghs_fixture","expires_at":"2026-01-01T00:00:00Z","permissions":{"contents":"read","actions":"read","checks":"read","metadata":"read"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, token)).await;
    let outcome = github.mint().await;
    let minted = outcome.minted().expect("the narrow response mints");

    assert_eq!(minted.token.as_str(), "ghs_fixture");
    assert_eq!(minted.expires_at_ms, NOW_MS + 3_600_000);
    assert!(minted.rotated_refresh_token.is_none());
    assert_eq!(
        github.asked_permissions(),
        vec![json!({"actions": "read", "checks": "read", "contents": "read"})]
    );
}

/// Dimension 1.6 — the App as registered today holds no Checks. The mint asks
/// without it and succeeds, where asking for it failed every mint.
#[tokio::test]
async fn an_installation_without_checks_mints_without_asking_for_them() {
    let token = r#"{"token":"ghs_fixture","permissions":{"contents":"read","actions":"read","metadata":"read"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_NO_CHECKS), (CREATED, token)).await;

    assert!(github.mint().await.minted().is_some());
    assert_eq!(
        github.asked_permissions(),
        vec![json!({"actions": "read", "contents": "read"})]
    );
}

#[tokio::test]
async fn a_successful_but_overreaching_response_is_discarded() {
    let token = r#"{"token":"ghs_fixture","permissions":{"contents":"write"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, token)).await;
    let capture = Capture::install();

    assert!(matches!(
        github.mint().await,
        Outcome::MintFailed(Retry::Permanent)
    ));
    // One warning an operator can read the reach from, and never the token.
    let events = capture.events();
    let [warning]: [&CapturedEvent; 1] = events
        .iter()
        .filter(|event| event.field("event") == Some("github_mint_overreach"))
        .collect::<Vec<_>>()
        .try_into()
        .expect("exactly one overreach warning");
    assert_eq!(warning.level, Level::WARN);
    for name in ["overreach", "granted", "requested"] {
        assert!(warning.field(name).is_some(), "{name} in {warning:?}");
    }
    assert!(
        warning
            .fields
            .values()
            .all(|value| !value.contains("ghs_fixture")),
        "{warning:?}"
    );
}

/// Dimension 1.7 — a failed installation read is classified as the token
/// request would be, and no token is asked for after it.
#[tokio::test]
async fn an_installation_read_failure_keeps_its_retry_posture() {
    let never_asked = (CREATED, "{}");
    for (case, installation, reconnects) in [
        ("unauthorised", (401, GONE), true),
        ("uninstalled", (404, GONE), true),
        ("unavailable", (503, LATER), false),
        ("unreadable", (OK, "not-json"), false),
    ] {
        let github = FakeGithub::answering(installation, never_asked).await;
        let outcome = github.mint().await;
        if reconnects {
            assert!(matches!(outcome, Outcome::ReconnectRequired), "{case}");
        } else {
            assert!(
                matches!(outcome, Outcome::MintFailed(Retry::Transient)),
                "{case}"
            );
        }
        assert!(github.asked_permissions().is_empty(), "{case}");
    }
}

#[tokio::test]
async fn token_request_failures_keep_their_retry_posture() {
    // Any other client error is GitHub refusing the request as made, which
    // asking again unchanged will not fix.
    let refused = FakeGithub::answering((OK, HOLDS_EVERYTHING), (422, GONE)).await;
    assert!(matches!(
        refused.mint().await,
        Outcome::MintFailed(Retry::Permanent)
    ));

    for status in [401, 404] {
        let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (status, GONE)).await;
        assert!(matches!(github.mint().await, Outcome::ReconnectRequired));
    }

    let unavailable = FakeGithub::answering((OK, HOLDS_EVERYTHING), (503, LATER)).await;
    assert!(matches!(
        unavailable.mint().await,
        Outcome::MintFailed(Retry::Transient)
    ));

    let malformed = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, "not-json")).await;
    assert!(matches!(
        malformed.mint().await,
        Outcome::MintFailed(Retry::Transient)
    ));
}

/// A freshly generated App key, PEM-encoded the way an operator pastes one.
///
/// Generated per run rather than committed: a private key in the tree is a
/// secret-scanner finding whatever it signs.
fn app_key() -> String {
    rcgen::KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256)
        .expect("the host generates an RSA key")
        .serialize_pem()
}

/// Mints through the App's own signed client, rooted at `base`.
async fn signed_mint(app: &GithubApp, base: &str) -> Outcome {
    let handle = json!({"integration": "github", "installation_id": INSTALLATION_ID});
    let binding = binding();
    mint_at(
        Exchange {
            app,
            handle: &handle,
            binding: Some(&binding),
            now_ms: NOW_MS,
        },
        base,
    )
    .await
}

#[tokio::test]
async fn a_usable_app_key_signs_its_way_to_a_token() {
    let token = r#"{"token":"ghs_signed","permissions":{"contents":"read","actions":"read","checks":"read","metadata":"read"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, token)).await;
    let app = GithubApp {
        app_id: 7,
        private_key_pem: app_key().into(),
    };

    let outcome = signed_mint(&app, &github.base).await;

    let minted = outcome.minted().expect("a signed exchange mints");
    assert_eq!(minted.token.as_str(), "ghs_signed");
    assert_eq!(github.asked_permissions().len(), 1);
}

#[tokio::test]
async fn an_api_root_that_is_not_a_uri_signs_nothing() {
    let app = GithubApp {
        app_id: 7,
        private_key_pem: app_key().into(),
    };

    assert!(matches!(
        signed_mint(&app, "not a uri").await,
        Outcome::MintFailed(Retry::Permanent)
    ));
}
