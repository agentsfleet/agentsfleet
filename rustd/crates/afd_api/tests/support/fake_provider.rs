//! A FAKE provider token endpoint, on a loopback port.
//!
//! Named for what it is, beside `afd_redis/tests/support/fake_redis.rs`: this
//! serves a fixture's answers, and a reader who took it for a real vendor
//! client would look here for the daemon's own exchange, which lives in
//! `afd_connector::Exchange`.
//!
//! `Exchange::pointed_at` exists because a token endpoint is a `&'static str`
//! in the registry and no test may post to Slack's. Pointing it here is what
//! makes a COMPLETED connect reachable at all: `Connectors::finish` redeems the
//! code before it reads a grant, so everything past the exchange — the parse,
//! the seal under the provider's key, the routing row — is unreachable without
//! something that answers.
//!
//! # Why a real socket rather than a stubbed exchange
//!
//! The seam this suite proves is the one BETWEEN the exchange and the vault,
//! and a stub handing back a parsed grant would jump it. The daemon's own
//! reading of the vendor's JSON — the field names, the `ok` flag, the scope
//! delimiter — is the half that breaks when a provider changes shape, so the
//! fixture answers bytes and lets the daemon do the reading.
//!
//! # It counts, because a count is the only proof of single use
//!
//! A replayed callback that got past the nonce would redeem the code again and
//! seal an identical grant. Nothing in the vault distinguishes that from the
//! first connect, so the assertion carrying the property is that the token
//! endpoint was asked exactly once.
//!
//! # A SEQUENCE of answers, which is the reference implementation's shape
//!
//! `oauth_providers_integration_test.zig`'s `FakeProvider` holds
//! `bodies: []const []const u8` and a cursor, so consecutive requests get
//! consecutive answers — it is how that suite drives Jira's token call and its
//! site listing from one server. The same shape is what lets a reconnect here
//! be one server issuing two different tokens rather than two servers, which
//! keeps the exchange count continuous across both halves of that test.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use tokio::task::JoinHandle;

const TOKEN_PATH: &str = "/oauth/access";
/// What a caller must ask for to be answered in JSON — GitHub's contract, and
/// the reason this fixture reads the request's `Accept` at all. It answered
/// JSON unconditionally until Sep 8, 2026, which made every test here pass
/// while the live GitHub connect refused every code it redeemed: the daemon
/// sent no `Accept`, GitHub answered form-encoded, and `complete` reported
/// `UZ-CONN-006` over a grant that had been issued. A fixture more forgiving
/// than the vendor is a fixture that certifies the bug.
const ACCEPT_JSON: &str = "application/json";
/// How the same answer comes back when nobody asked for JSON.
const FORM_MEDIA_TYPE: &str = "application/x-www-form-urlencoded";
const FIELD_SEPARATOR: &str = "&";
const FIELD_ASSIGNMENT: char = '=';

/// One GET the vendor answers beside the exchange: its path, status and body.
///
/// GitHub's connect asks two more questions after the exchange — which
/// installations the person reaches, and whether a claimed one opens — and
/// both go to the origin the exchange was pinned at (`afd_connector::endpoint`).
/// A read is answered by PATH so a test arranges the vendor's state ("this
/// person reaches one installation") rather than an outcome.
pub(crate) struct Read {
    pub(crate) path: String,
    pub(crate) status: u16,
    pub(crate) body: String,
}

/// The fixture's answer as a vendor renders it for a caller that asked for no
/// JSON: the same fields, form-encoded. Values render as their scalar text — a
/// token answer holds strings and numbers, never a nested object — so a daemon
/// that reads this with `serde_json` fails exactly where the live one did.
fn form_encoded(answer: &serde_json::Value) -> String {
    answer.as_object().map_or_else(String::new, |fields| {
        fields
            .iter()
            .map(|(name, value)| {
                let rendered = value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned);
                format!("{name}{FIELD_ASSIGNMENT}{rendered}")
            })
            .collect::<Vec<_>>()
            .join(FIELD_SEPARATOR)
    })
}

pub(crate) struct FakeProvider {
    url: String,
    exchanges: Arc<AtomicUsize>,
    reads: Arc<AtomicUsize>,
    handle: JoinHandle<()>,
}

impl FakeProvider {
    pub(crate) async fn answering(bodies: &[&str]) -> Self {
        Self::answering_with_reads(bodies, Vec::new()).await
    }

    /// A vendor that answers the exchange from `bodies` and each GET in
    /// `reads` from its own path.
    pub(crate) async fn answering_with_reads(bodies: &[&str], reads: Vec<Read>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let url = format!(
            "http://{}{TOKEN_PATH}",
            listener.local_addr().expect("a bound address")
        );
        let exchanges = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&exchanges);
        let answers: Vec<serde_json::Value> = bodies
            .iter()
            .map(|body| serde_json::from_str(body).expect("a fixture answer is JSON"))
            .collect();
        assert!(
            !answers.is_empty(),
            "a fake provider answers at least one body"
        );
        let mut router = Router::new().route(
            TOKEN_PATH,
            post(move |headers: HeaderMap| {
                let asked = counted.fetch_add(1, Ordering::SeqCst);
                let answer = answers
                    .get(asked)
                    .or_else(|| answers.last())
                    .cloned()
                    .expect("the fake provider was built with at least one answer");
                let asked_for_json = headers
                    .get(ACCEPT)
                    .is_some_and(|value| value.as_bytes() == ACCEPT_JSON.as_bytes());
                async move {
                    if asked_for_json {
                        axum::Json(answer).into_response()
                    } else {
                        ([(CONTENT_TYPE, FORM_MEDIA_TYPE)], form_encoded(&answer)).into_response()
                    }
                }
            }),
        );
        let served = Arc::new(AtomicUsize::new(0));
        for read in reads {
            let status = StatusCode::from_u16(read.status).expect("a fixture status is one");
            let body: serde_json::Value =
                serde_json::from_str(&read.body).expect("a fixture read answer is JSON");
            let counted = Arc::clone(&served);
            router = router.route(
                &read.path,
                get(move || {
                    counted.fetch_add(1, Ordering::SeqCst);
                    let body = body.clone();
                    async move { (status, axum::Json(body)) }
                }),
            );
        }
        let handle = tokio::spawn(async move {
            let _served = axum::serve(listener, router).await;
        });
        Self {
            url,
            exchanges,
            reads: served,
            handle,
        }
    }

    pub(crate) fn url(&self) -> String {
        self.url.clone()
    }

    pub(crate) fn exchanges(&self) -> usize {
        self.exchanges.load(Ordering::SeqCst)
    }

    /// How many of the arranged GETs were asked.
    pub(crate) fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    pub(crate) fn close(self) {
        self.handle.abort();
    }
}
