//! A provider's token endpoint, on a loopback port.
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

/// A token endpoint that answers one body and counts what it was asked.
pub(super) struct Vendor {
    /// Where [`Exchange::pointed_at`] should be aimed.
    url: String,
    /// How many codes have been redeemed here.
    exchanges: Arc<AtomicUsize>,
    /// Aborted by [`Vendor::close`] — see there.
    handle: JoinHandle<()>,
}

impl Vendor {
    /// Serves `body` as JSON to every request, counting each one.
    ///
    /// Through `axum::serve` rather than a hand-written response: this crate
    /// already depends on axum with the `tokio` feature, and framing HTTP by
    /// hand to answer a fixed document is the kind of parser RULE PSR exists to
    /// stop. It also keeps the server serving indefinitely, which a reconnect
    /// needs — one that answered a single request would fail that test as a
    /// transport error rather than as whatever it was proving.
    pub(super) async fn answering(body: &str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let url = format!(
            "http://{}{TOKEN_PATH}",
            listener.local_addr().expect("a bound address")
        );

        let exchanges = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&exchanges);
        let body = body.to_owned();
        let router = Router::new().route(
            TOKEN_PATH,
            post(move || {
                counted.fetch_add(1, Ordering::SeqCst);
                let answer = body.clone();
                async move { axum::Json(serde_json::from_str::<serde_json::Value>(&answer).expect("the fixture answer is JSON")) }
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

    pub(super) fn url(&self) -> String {
        self.url.clone()
    }

    /// How many codes have been redeemed here.
    pub(super) fn exchanges(&self) -> usize {
        self.exchanges.load(Ordering::SeqCst)
    }

    /// Stops the server.
    ///
    /// Called rather than left to the drop: the spawned task owns the listener,
    /// so a fixture that only dropped its handle would leave the port bound for
    /// as long as the test binary runs.
    pub(super) fn close(self) {
        self.handle.abort();
    }
}
