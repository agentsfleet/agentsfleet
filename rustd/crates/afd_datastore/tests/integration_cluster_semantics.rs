//! Dimension 6.2 — what the cluster must not change about failing.
//!
//! The dimension names four hazards, and each is one a SHARDED datastore
//! introduces or sharpens. They are asserted together because they share the
//! property under test: the answer a caller reads is the same answer a
//! single-node Dragonfly gave, and an action that may happen once still happens
//! once.
//!
//! # Lost replies
//!
//! A reply that never arrives does not tell the caller whether the command
//! ran. On a cluster there is one more way to lose one — a slot moves and the
//! driver re-issues against the new owner — so every write that matters is a
//! compare-and-set, and a retry that lost its race is refused rather than
//! applied twice.
//!
//! # Script cache loss
//!
//! This is the cluster's own hazard and the reason this file exists. Both
//! one-time actions in this crate are Lua: the token-checked readiness clear
//! and the device-flow transition. Both ship as `EVALSHA`, and a node that
//! restarts, is replaced, or is flushed has never seen the body. On one server
//! that is a rare event; on four it is four times as likely, and it is the
//! difference between a script that reloads and one that answers `NOSCRIPT` to
//! a caller who reads it as a failure and retries a redemption.
//!
//! # Session races and expiry
//!
//! In [`sessions`], split out for length. `test_session_transition_atomic`
//! proves the race on a warm cache; it is re-run there against one that was
//! just emptied, because a reload that happened once per caller rather than
//! once per server would serialise nothing. Expiry sits beside it for one
//! reason: `Expired` and `Missing` are different answers to a caller and a
//! cluster must not collapse them into one.
//!
//! Marked `#[ignore]` like the rest of the live-cluster suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "integration_cluster_semantics/sessions.rs"]
mod sessions;

use afd_datastore::ready::{Partition, ReadyIndex};
use redis::cluster_routing::{MultipleNodeRoutingInfo, ResponsePolicy, RoutingInfo};

use crate::cluster::ClusterHarness;
use crate::support::RedisHarness;

/// Emptying every primary's Lua cache. `SCRIPT` names no key, so the routing
/// has to be given rather than derived from one.
const CMD_SCRIPT: &str = "SCRIPT";
const SUB_FLUSH: &str = "FLUSH";

/// The two generations every compare-and-set here is written between.
const FIRST_GENERATION: &str = "generation-one";
const SECOND_GENERATION: &str = "generation-two";

/// Wider than any partition this file puts a single fleet in, so a peek that
/// found nothing means the mark is gone rather than that the sample missed it.
const WHOLE_PARTITION: usize = 64;

/// Empties the Lua cache on EVERY primary.
///
/// `AllMasters` rather than a single node, and that is the whole point: a
/// flush that reached one server would leave the others warm, and a suite
/// asserting a reload would pass without one ever happening.
async fn forget_every_script(cluster: &ClusterHarness) {
    let mut connection = cluster.connect().await;
    let mut cmd = redis::cmd(CMD_SCRIPT);
    cmd.arg(SUB_FLUSH);
    connection
        .route_command(
            cmd,
            RoutingInfo::MultiNode((
                MultipleNodeRoutingInfo::AllMasters,
                Some(ResponsePolicy::AllSucceeded),
            )),
        )
        .await
        .expect("every primary empties its script cache");
}

/// Whether the index still holds a mark for `fleet`.
async fn is_marked(index: &ReadyIndex, fleet: &str) -> bool {
    index
        .peek(Partition::of(fleet), WHOLE_PARTITION)
        .await
        .expect("the partition reads")
        .iter()
        .any(|ready| ready.fleet_id == fleet)
}

/// A retry whose first reply was lost must not erase work that arrived since.
///
/// The compare-and-set is what makes the retry safe, and this is the case it
/// exists for rather than the case it is usually described by: the caller's
/// clear DID run, the reply was lost, and by the time the retry lands ingress
/// has marked the fleet again. An unconditional delete would erase that second
/// mark, and the work behind it would wait for a sweep.
async fn a_lost_reply_retried_does_not_erase_newer_work(harness: &RedisHarness) {
    let index = ReadyIndex::new(harness.redis.clone());
    let fleet = harness.name("lost-reply");

    let first = index
        .mark(&fleet, FIRST_GENERATION)
        .await
        .expect("ingress marks the fleet");
    assert!(
        index
            .clear_if_unchanged(&fleet, &first)
            .await
            .expect("the clear runs"),
        "the poll that saw this generation clears it"
    );

    // The reply to that clear never reached the caller. Before it retries,
    // ingress admits again.
    index
        .mark(&fleet, SECOND_GENERATION)
        .await
        .expect("ingress marks the fleet again");

    assert!(
        !index
            .clear_if_unchanged(&fleet, &first)
            .await
            .expect("the retry runs rather than faulting"),
        "the retry carries the generation it observed, so it declines to clear the newer one"
    );
    assert!(
        is_marked(&index, &fleet).await,
        "and the newer mark is still there for the next poll to find"
    );

    index.force_clear(&fleet).await.expect("cleanup");
}

/// A one-time action survives the server forgetting how to perform it.
///
/// Both halves matter. The clear must still ANSWER — a `NOSCRIPT` surfacing as
/// an error would make an ordinary poll look like an outage — and it must
/// still answer CORRECTLY, because a reload that lost the body would be a
/// script that runs and compares nothing.
async fn a_forgotten_script_reloads_and_still_compares(
    harness: &RedisHarness,
    cluster: &ClusterHarness,
) {
    let index = ReadyIndex::new(harness.redis.clone());
    let fleet = harness.name("forgotten-script");

    let stale = index
        .mark(&fleet, FIRST_GENERATION)
        .await
        .expect("ingress marks the fleet");

    forget_every_script(cluster).await;

    assert!(
        index
            .clear_if_unchanged(&fleet, &stale)
            .await
            .expect("a server that has never seen the body loads it rather than refusing"),
        "and the reloaded body is the one that compares the token, not an empty script"
    );

    // The negative arm, on a cache emptied again: a stale token still fails to
    // match. Without it, a reload that returned a constant would pass above.
    let current = index
        .mark(&fleet, SECOND_GENERATION)
        .await
        .expect("ingress marks the fleet again");
    forget_every_script(cluster).await;
    assert!(
        !index
            .clear_if_unchanged(&fleet, &stale)
            .await
            .expect("the reload answers"),
        "a reloaded script refuses a token that does not match, exactly as a warm one does"
    );
    assert!(
        is_marked(&index, &fleet).await,
        "so the fleet the stale token aimed at is still discoverable"
    );

    assert!(
        index
            .clear_if_unchanged(&fleet, &current)
            .await
            .expect("cleanup"),
        "and the matching token still clears"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live cluster: make test-integration-rustd"]
async fn test_cluster_preserves_failure_and_session_semantics() {
    let harness = RedisHarness::connect().await;
    let cluster = ClusterHarness::from_lane();

    a_lost_reply_retried_does_not_erase_newer_work(&harness).await;
    a_forgotten_script_reloads_and_still_compares(&harness, &cluster).await;
    sessions::a_race_through_a_cold_cache_still_redeems_once(&harness, &cluster).await;
    sessions::an_expired_code_is_gone_rather_than_redeemable(&harness).await;
}
