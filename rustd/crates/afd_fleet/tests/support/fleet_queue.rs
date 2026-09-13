//! The lane's Redis, and the two things a lease test has to put in it.
//!
//! Separate from `fleet_fixtures.rs` because the two harnesses have opposite
//! lifetimes. That file creates a DATABASE PER TEST and drops it, which is what
//! keeps row assertions independent. Redis has no such equivalent: the
//! readiness index is one hash at a fixed key and the streams are keyed by
//! fleet, so isolation here comes from every test declaring its own fleet ids
//! rather than from tearing anything down.
//!
//! That difference is why a leaked readiness mark is harmless: the candidate
//! query joins `core.fleets` in the test's OWN database, so another test's
//! fleet id cannot survive the filter even when its mark is still in the index.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicI64, Ordering};

use afd_datastore::{FleetStreams, ReadyIndex, Redis, RedisConfig, RedisRole};
use afd_wire::event::Entry;

/// The lane's Redis URL.
const URL_KNOB: &str = "TEST_REDIS_URL";

/// The lane's Redis certificate authority, when it serves TLS.
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// The configuration the lane hands these suites.
pub(crate) fn config() -> RedisConfig {
    let url = std::env::var(URL_KNOB).unwrap_or_else(|_error| {
        panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
    });
    RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(CA_KNOB).ok().map(Into::into))
}

/// A Redis nobody is listening on.
///
/// Port 1 is reserved and unbound on every platform this builds for, so a
/// command fails on connection refusal rather than waiting out a timeout — the
/// difference between a suite that runs in milliseconds and one that runs in
/// request budgets. Plain `redis://` rather than the lane's `rediss://`: no
/// socket is ever opened, so a certificate authority would be configuration
/// nothing reads.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// A handle over a Redis that will not answer, for the drop paths.
///
/// `Redis::unreachable` skips the ping `connect` performs, which is the only
/// way to hold this: the lane's Redis is SHARED by every test binary running in
/// parallel, so pausing the container or killing the server would fail
/// unrelated suites at the same instant. A handle one test owns fails only that
/// test's commands.
pub(crate) fn unreachable() -> Redis {
    Redis::unreachable(&RedisConfig::from_url(
        RedisRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail")
}

/// Connects to the lane's Redis.
pub(crate) async fn connect() -> Redis {
    afd_datastore::test_util::connect_live(&config())
        .await
        .expect("the lane's Redis must be reachable")
}

/// Puts one event on a fleet's stream and marks the fleet ready.
///
/// Both halves, because either alone is a state the daemon never produces:
/// ingress appends and marks in one path, and a mark with no entry would make
/// the assignment pass look broken when it is the fixture that is.
///
/// The field names are `event_envelope.zig`'s `encodeForXAdd` argv — the
/// producer's side of the contract `assign.rs` reads.
pub(crate) async fn enqueue(
    queue: &Redis,
    fleet: &str,
    workspace: &str,
    actor: &str,
    event_type: &str,
    request_json: &str,
    created_at: i64,
) -> String {
    let streams = FleetStreams::new(queue.clone());
    streams
        .ensure_group(fleet)
        .await
        .expect("the consumer group must exist before a read");
    let created = created_at.to_string();
    let logical = mint_logical_id(created_at);
    // `Entry::queued_pairs` and NOT a hand-written field list. The list this
    // fixture used to carry went stale the moment the ledger added `event_id`:
    // every
    // entry it wrote was refused by `lease::envelope` as "appended by something
    // that did not admit it", dropped, and the fleet looked empty — which is
    // how one fixture failed twenty-six integration tests at once. Routing the
    // shape through the producer's own helper is what makes that impossible to
    // repeat: a field added there arrives here with it.
    let entry = Entry {
        actor,
        event_type,
        workspace_id: workspace,
        request_json,
        created_at: &created,
    };
    let _receipt = streams
        .append(fleet, &entry.queued_pairs(&logical))
        .await
        .expect("the append must land");
    mark_ready(queue, fleet).await;
    logical
}

/// A logical event id in the ledger's shape, unique within this process.
///
/// `<millis>-<seq>` is what `afd_admission::logical_id` spells and what
/// `logical_parts` reads back, and the lease path now carries it as the event's
/// identity. The sequence is a process counter rather than the append's entry
/// id, because the two are deliberately different things now that the ledger
/// owns identity: the entry id
/// is a RECEIPT, and a fixture handing one back as an identity would re-teach
/// the confusion the ledger exists to end.
fn mint_logical_id(created_at: i64) -> String {
    static LOGICAL_SEQUENCE: AtomicI64 = AtomicI64::new(1);
    let seq = LOGICAL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    afd_admission::logical_id(created_at, seq)
}

/// Appends an entry in the shape the Rust producers wrote before the wire fix.
///
/// `event_type` and `request_json` rather than `type` and `request`, and no
/// `created_at` at all. Entries in exactly this shape are sitting on real
/// streams, which is why the reader has to survive one rather than wedge the
/// fleet behind it. The field names are LITERALS on purpose: routing them
/// through `afd_wire`'s constants would make the fixture move with the code it
/// exists to contradict.
pub(crate) async fn enqueue_cutover_era(
    queue: &Redis,
    fleet: &str,
    workspace: &str,
    actor: &str,
    event_type: &str,
    request_json: &str,
) -> String {
    let streams = FleetStreams::new(queue.clone());
    streams
        .ensure_group(fleet)
        .await
        .expect("the consumer group must exist before a read");
    let id = streams
        .append(
            fleet,
            &[
                ("event_type", event_type),
                ("actor", actor),
                ("workspace_id", workspace),
                ("request_json", request_json),
            ],
        )
        .await
        .expect("the append must land");
    mark_ready(queue, fleet).await;
    id.as_str().to_owned()
}

/// Marks a fleet ready so the readiness peek can surface it.
/// Every entry on one fleet's stream, as `(receipt, logical event id)`.
///
/// Over `afd_datastore::test_util::fleet_entries`, which is an `XRANGE` and not
/// a group read — `XREADGROUP` would move `last-delivered-id` and with it the
/// retention floor, changing the thing the caller is about to assert on.
///
/// Both halves are returned because after a replay one logical event
/// legitimately sits on two entries: the receipt is the physical copy and the
/// field is the identity.
pub(crate) async fn entries_on(queue: &Redis, fleet: &str) -> Vec<(String, String)> {
    afd_datastore::test_util::fleet_entries(queue, fleet)
        .await
        .expect("the lane's stream answers a range read")
        .iter()
        .map(|entry| {
            (
                entry.receipt.as_str().to_owned(),
                entry
                    .field(afd_wire::event::field::EVENT_ID)
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

pub(crate) async fn mark_ready(queue: &Redis, fleet: &str) {
    ReadyIndex::new(queue.clone())
        .mark(fleet, fleet)
        .await
        .expect("the readiness mark must land");
}

/// Removes a fleet's readiness mark, so one test's fleet does not crowd the
/// bounded peek another test depends on.
pub(crate) async fn clear_ready(queue: &Redis, fleet: &str) {
    let index = ReadyIndex::new(queue.clone());
    let token = index
        .mark(fleet, fleet)
        .await
        .expect("re-marking to obtain the token must succeed");
    let _cleared = index.clear_if_unchanged(fleet, &token).await;
}
