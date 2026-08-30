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
use axum::routing::post;
use tokio::task::JoinHandle;

/// The path this stands the token endpoint at.
///
/// Any path serves: the daemon posts wherever `Exchange::pointed_at` aims it,
/// and what a provider calls its own endpoint is the registry's business.
const TOKEN_PATH: &str = "/oauth/access";

/// A token endpoint that answers a sequence of bodies and counts its callers.
pub(crate) struct FakeProvider {
    /// Where [`Exchange::pointed_at`] should be aimed.
    url: String,
    /// How many codes have been redeemed here.
    exchanges: Arc<AtomicUsize>,
    /// Aborted by [`FakeProvider::close`] — see there.
    handle: JoinHandle<()>,
}

impl FakeProvider {
    /// Serves `bodies` in order, then repeats the last, counting every call.
    ///
    /// Repeating rather than running out: a test that sent one request too many
    /// should fail on the assertion it was making, not on a transport error
    /// from a server that had nothing left to say.
    ///
    /// Through `axum::serve` rather than a hand-written response: this crate
    /// already depends on axum with the `tokio` feature, and framing HTTP by
    /// hand to answer a fixed document is the kind of parser RULE PSR exists to
    /// stop.
    pub(crate) async fn answering(bodies: &[&str]) -> Self {
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

        let router = Router::new().route(
            TOKEN_PATH,
            post(move || {
                let asked = counted.fetch_add(1, Ordering::SeqCst);
                // `get` then `last` rather than a clamped index: the repeat is the
                // rule being stated, and indexing to express it can panic.
                let answer = answers
                    .get(asked)
                    .or_else(|| answers.last())
                    .cloned()
                    .expect("the fake provider was built with at least one answer");
                async move { axum::Json(answer) }
            }),
        );

        let handle = tokio::spawn(async move {
            let _served = axum::serve(listener, router).await;
        });

        Self {
            url,
            exchanges,
            handle,
        }
    }

    pub(crate) fn url(&self) -> String {
        self.url.clone()
    }

    /// How many codes have been redeemed here.
    pub(crate) fn exchanges(&self) -> usize {
        self.exchanges.load(Ordering::SeqCst)
    }

    /// Stops the server.
    ///
    /// Called rather than left to the drop: the spawned task owns the listener,
    /// so a fixture that only dropped its handle would leave the port bound for
    /// as long as the test binary runs.
    pub(crate) fn close(self) {
        self.handle.abort();
    }
}
