//! What the cache and the flight guard decide, with a counting exchange in
//! place of a vendor.
//!
//! The exchange itself is proven in `github` and `oauth`; what these prove is
//! everything around it — that one upstream call serves N concurrent callers,
//! that a refusal is never cached, that a rotation reaches exactly the caller
//! that caused it, and that two asks are the same entry only when serving one
//! the other's token would be correct.
//!
//! Every case runs on the MULTI-THREADED runtime: a single-threaded one cannot
//! interleave the contenders the flight guard exists for, so the race would
//! pass by never happening (`dispatch/write_rust.md`).
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_fleet_runtime::config::{Access, RepositoryBinding};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::{Ask, Broker, Exchanger, stored};
use crate::credential::outcome::{Minted, Outcome, Retry};
use crate::secrets::connector::{Connector, Registry};

/// The instant every decision here is measured from.
const NOW_MS: i64 = 1_760_000_000_000;

/// An hour past [`NOW_MS`] — comfortably outside the re-mint skew.
const IN_AN_HOUR_MS: i64 = NOW_MS + 3_600_000;

/// The panic a malformed fixture identifier earns.
///
/// Declared once (RULE UFS) because two spellings of the same precondition read
/// as two different preconditions in a failure log.
const V7_EXPECTED: &str = "a canonical v7 identifier";

/// The connector every ask below names, and the fields its stored body carries.
///
/// One spelling each: a fixture whose seeded field name drifted from the one
/// the assertion reads would prove the rotation reached a body nothing stored.
const INTEGRATION: &str = "integration";
const ZOHO: &str = "zoho";
const REFRESH_TOKEN: &str = "refresh_token";
const CONNECTED_AT_MS: &str = "connected_at_ms";

/// The tokens the counting exchange mints, seeds and rotates.
///
/// Three distinct values, named because every assertion below compares against
/// one of them and a literal repeated at the seed and at the assertion is two
/// places for a rotation test to agree with itself while proving nothing.
const MINTED_ACCESS_TOKEN: &str = "at_minted";
const SEEDED_REFRESH_TOKEN: &str = "rt_seeded";
const ROTATED_REFRESH_TOKEN: &str = "rt_rotated";

/// What a contender's join answers when the flight guard did its job.
const CONTENDER_COMPLETED: &str = "a contender ran to completion";

/// The workspace every ask below belongs to.
fn workspace() -> Uuid7 {
    Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011").expect(V7_EXPECTED)
}

/// A second workspace, for the scope proof.
fn other_workspace() -> Uuid7 {
    Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1012").expect(V7_EXPECTED)
}

/// A binding over `acme/widgets` at `access`.
fn binding(access: Access) -> RepositoryBinding {
    let base = (access == Access::Write).then(|| "main".into());
    RepositoryBinding::from_parts(vec!["acme/widgets".into()], access, base)
}

/// An exchange that counts its calls and answers whatever it was told to.
///
/// The stand-in for a vendor: what a broker test needs from GitHub or Zoho is
/// how MANY times it was asked and what it said, and both are parameters here.
#[derive(Debug)]
struct Counting {
    /// How many exchanges actually ran.
    calls: AtomicUsize,
    /// What each one answers.
    answer: Answer,
    /// How long one takes, which is what makes a race a race.
    latency: Duration,
}

/// What a counting exchange hands back.
#[derive(Debug, Clone)]
enum Answer {
    /// A token that lives an hour, optionally rotating a refresh token.
    Token { rotates: bool },
    /// A refusal.
    Refused(Outcome),
}

impl Counting {
    /// A vendor answering `answer`, taking `latency` to do it.
    fn new(answer: Answer, latency: Duration) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            answer,
            latency,
        })
    }

    /// How many exchanges this vendor actually performed.
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Exchanger for Counting {
    fn exchange<'a>(
        &'a self,
        _connector: &'a dyn Connector,
        _ask: Ask<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.latency).await;
            match &self.answer {
                Answer::Token { rotates } => Outcome::Ok(Minted {
                    token: Zeroizing::new(MINTED_ACCESS_TOKEN.to_owned()),
                    expires_at_ms: IN_AN_HOUR_MS,
                    rotated_refresh_token: rotates
                        .then(|| Zeroizing::new(ROTATED_REFRESH_TOKEN.to_owned())),
                }),
                Answer::Refused(refusal) => refusal.clone(),
            }
        })
    }
}

/// A broker over the shipped registry and a counting vendor.
fn broker(vendor: &Arc<Counting>) -> Broker {
    Broker::new(Arc::new(Registry), Arc::clone(vendor) as Arc<dyn Exchanger>)
}

/// A Zoho handle, which mints through the refresh exchange.
fn zoho_handle() -> Value {
    json!({INTEGRATION: ZOHO, REFRESH_TOKEN: SEEDED_REFRESH_TOKEN, CONNECTED_AT_MS: 1})
}

/// The ask `handle` makes in `workspace`, under `binding`.
fn ask<'a>(
    workspace_id: &'a Uuid7,
    handle: &'a Value,
    binding: Option<&'a RepositoryBinding>,
) -> Ask<'a> {
    Ask {
        workspace_id,
        handle,
        binding,
        now_ms: NOW_MS,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_cold_callers_cost_exactly_one_upstream_mint() {
    // Dimension 4.5, and the reason the Zig hand-rolls a flight guard: two cold
    // misses on a ROTATING provider both post the same refresh token, and a
    // provider with reuse detection then revokes the whole family. The latency
    // is what guarantees the contenders overlap rather than queueing.
    let vendor = Counting::new(Answer::Token { rotates: false }, Duration::from_millis(50));
    let broker = Arc::new(broker(&vendor));
    let workspace = workspace();
    let handle = zoho_handle();

    let contenders: Vec<_> = (0..16)
        .map(|_| {
            let broker = Arc::clone(&broker);
            let workspace = workspace.clone();
            let handle = handle.clone();
            tokio::spawn(async move { broker.mint(ask(&workspace, &handle, None)).await })
        })
        .collect();

    for contender in contenders {
        let outcome = contender.await.expect(CONTENDER_COMPLETED);
        assert_eq!(
            outcome.minted().map(|minted| minted.token.as_str()),
            Some(MINTED_ACCESS_TOKEN),
            "every caller is served the same live token"
        );
    }
    assert_eq!(vendor.calls(), 1, "the vendor was asked more than once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_ask_is_served_from_the_cache() {
    let vendor = Counting::new(Answer::Token { rotates: false }, Duration::ZERO);
    let broker = broker(&vendor);
    let workspace = workspace();
    let handle = zoho_handle();

    for _ in 0..3 {
        let outcome = broker.mint(ask(&workspace, &handle, None)).await;
        assert!(outcome.minted().is_some());
    }
    assert_eq!(vendor.calls(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_is_handed_to_every_waiter_and_cached_by_none() {
    // The negative half of Dimension 4.5: a bad minute at a vendor must not
    // become a cached minute of refusals, and the retry that follows must
    // actually reach the vendor.
    for (refusal, expected) in [
        (Outcome::MintFailed(Retry::Transient), "a transient failure"),
        (Outcome::ReconnectRequired, "a dead connection"),
        (Outcome::UnknownIntegration, "an unknown integration"),
        (Outcome::Unconfigured, "an unconfigured integration"),
    ] {
        let vendor = Counting::new(Answer::Refused(refusal), Duration::from_millis(20));
        let broker = Arc::new(broker(&vendor));
        let workspace = workspace();
        let handle = zoho_handle();

        let contenders: Vec<_> = (0..4)
            .map(|_| {
                let broker = Arc::clone(&broker);
                let workspace = workspace.clone();
                let handle = handle.clone();
                tokio::spawn(async move { broker.mint(ask(&workspace, &handle, None)).await })
            })
            .collect();
        for contender in contenders {
            let outcome = contender.await.expect(CONTENDER_COMPLETED);
            assert!(outcome.minted().is_none(), "{expected} carried a token");
        }
        // Concurrent refusals still single-flight — one call served them all.
        assert_eq!(vendor.calls(), 1, "{expected}");

        // And nothing was cached, so the next ask mints again.
        broker.mint(ask(&workspace, &handle, None)).await;
        assert_eq!(vendor.calls(), 2, "{expected} was cached");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rotation_reaches_the_caller_that_caused_it_and_nobody_else() {
    // The write-back obligation: these providers invalidate the posted refresh
    // token the moment they issue a successor, so exactly one caller may — and
    // must — persist the replacement. A cache HIT performed no exchange and
    // rotated nothing; reporting one would have a second caller write a value
    // the vault already holds, and a third write it back over a NEWER one.
    let vendor = Counting::new(Answer::Token { rotates: true }, Duration::ZERO);
    let broker = broker(&vendor);
    let workspace = workspace();
    let handle = zoho_handle();

    let cold = broker.mint(ask(&workspace, &handle, None)).await;
    let rotated = cold
        .minted()
        .expect("the cold mint produced a token")
        .rotated_refresh_token
        .as_deref()
        .map(String::as_str);
    assert_eq!(rotated, Some(ROTATED_REFRESH_TOKEN));

    let warm = broker.mint(ask(&workspace, &handle, None)).await;
    assert!(
        warm.minted()
            .expect("the warm read produced a token")
            .rotated_refresh_token
            .is_none(),
        "a cache hit reported a rotation it did not perform"
    );
    assert_eq!(vendor.calls(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_asks_share_an_entry_only_when_serving_one_the_others_token_is_correct() {
    // Each pair below differs in exactly one component of the key, and each is
    // a case where a shared entry would hand a caller a credential it must not
    // have — or, for the rotation, would cost a needless mint.
    let vendor = Counting::new(Answer::Token { rotates: false }, Duration::ZERO);
    let broker = broker(&vendor);

    let workspace = workspace();
    let elsewhere = other_workspace();
    let handle = zoho_handle();
    let read = binding(Access::Read);
    let write = binding(Access::Write);

    // The baseline entry.
    broker.mint(ask(&workspace, &handle, Some(&read))).await;
    assert_eq!(vendor.calls(), 1);

    // A different WORKSPACE is another tenant. Invariant 2.
    broker.mint(ask(&elsewhere, &handle, Some(&read))).await;
    assert_eq!(vendor.calls(), 2, "a foreign workspace shared an entry");

    // A different BINDING is a different scope: a read-scoped fleet served the
    // write-scoped token its neighbour cached undoes the narrowing.
    broker.mint(ask(&workspace, &handle, Some(&write))).await;
    assert_eq!(vendor.calls(), 3, "two scopes shared an entry");

    // Declaring NO binding is not the same as declaring one.
    broker.mint(ask(&workspace, &handle, None)).await;
    assert_eq!(vendor.calls(), 4, "an unbound ask shared a bound entry");

    // A different INSTALLATION — a reconnect, which the connect callbacks make
    // visible by stamping a fresh `connected_at_ms`.
    let reconnected =
        json!({INTEGRATION: ZOHO, REFRESH_TOKEN: SEEDED_REFRESH_TOKEN, CONNECTED_AT_MS: 2});
    broker
        .mint(ask(&workspace, &reconnected, Some(&read)))
        .await;
    assert_eq!(vendor.calls(), 5, "a reconnected handle shared an entry");

    // But an ordinary ROTATION of the credential itself is the same
    // installation, and must stay a hit — otherwise every refresh costs a mint.
    let rotated =
        json!({INTEGRATION: ZOHO, REFRESH_TOKEN: ROTATED_REFRESH_TOKEN, CONNECTED_AT_MS: 1});
    broker.mint(ask(&workspace, &rotated, Some(&read))).await;
    assert_eq!(vendor.calls(), 5, "a rotated credential missed the cache");
}

mod edges;
