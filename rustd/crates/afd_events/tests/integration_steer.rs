//! Dimension 5.3 — the id a steer answers with IS the id the runner leases.
//!
//! The refusal matrix in front of this append is proven without a datastore —
//! thirty-four cases across `afd_api`'s message suites decide who may steer,
//! what bodies are taken, and which fleet states refuse. None of them can prove
//! the one claim the surface actually makes to a client, because it is a claim
//! about Redis: the `event_id` returned in the 202 is the stream entry id, and
//! it is therefore the id the CLI filters SSE frames by and the id the runner
//! sees when it leases the work.
//!
//! Proving that needs BOTH ends of the same append — the value the caller was
//! handed, and the entry a consumer reads back — so it needs a live stream.
//!
//! # The idempotency clause, and why this file does not assert it
//!
//! Dimension 5.3 was written as "duplicate → idempotent per the documented
//! dedup". There is no such dedup, in either daemon or in the contract:
//!
//! - the retired daemon's `http/handlers/fleets/messages.zig:119` called
//!   `xaddFleetEvent` unconditionally — one append per request, no key
//!   consulted, no prior request remembered;
//! - `public/openapi/paths/fleet-messages.yaml` documents a 202 carrying a
//!   `<MILLISECONDS>-<SEQUENCE>` `event_id` and defines no idempotency key,
//!   request or header;
//! - `FleetStreams::append_once` — the workspace's ONE deduplicating append —
//!   exists for the repair path's crash-shaped retries and is not on this
//!   route in either implementation.
//!
//! So the honest reading is that two identical steers are two messages a person
//! sent twice, and the daemon delivers both. That is asserted here POSITIVELY
//! rather than left unsaid: a future dedup would break this test, which is
//! exactly the notice such a change should get. The divergence from the
//! dimension's wording is recorded in the spec's Declared divergences.
//!
//! Marked `#[ignore]` so `make test-unit-all` still COMPILES and lints this
//! without a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_events::{ACTOR_MACHINE, Steer};
use afd_redis::ReadyIndex;
use afd_redis::ready::READY_INDEX_KEY;
use afd_redis::streams::FleetStreams;
use afd_wire::event::{EventType, field};

use crate::support::EventsLane;

/// The consumer name the test reads the stream back under.
///
/// Any name works — the group is what orders delivery — but a distinctive one
/// makes a stuck pending entry legible in `XPENDING` when a run goes wrong.
const CONSUMER: &str = "steer-integration-reader";

/// The payload a caller's steer carries, already serialized by the handler.
const REQUEST_JSON: &str = r#"{"message":"redeploy staging"}"#;

/// Dimension 5.3 — a steer's answer is the stream entry's own id, and the
/// fleet is marked ready so the message is leasable now rather than at the next
/// poll.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_steer_append_event_id() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.queue.clone());

    // The group has to exist before the append, because `read_new` delivers
    // through it. A consumer created afterwards with `$` would see nothing and
    // this test would report "the entry is missing" for a reason that is the
    // test's own.
    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("the consumer group is created");

    let answered = steer
        .append(&lane.fleet, &lane.workspace, ACTOR_MACHINE, REQUEST_JSON)
        .await
        .expect("the append reaches the queue");

    let leased = streams
        .read_new(&lane.fleet, CONSUMER)
        .await
        .expect("the read reaches the queue")
        .expect("the append left an entry to lease");

    assert_eq!(
        leased.id.as_str(),
        answered,
        "the id answered to the client must BE the stream entry id — a client \
         filters its SSE frames by this value and a runner leases the entry \
         under it, so two spellings would be two events"
    );

    // The envelope the runner reads is the one the handler wrote. Asserted
    // field by field rather than as a blob: a producer that dropped
    // `workspace_id` would still lease and still run, against no workspace.
    assert_eq!(leased.field(field::ACTOR), Some(ACTOR_MACHINE));
    assert_eq!(
        leased.field(field::EVENT_TYPE),
        Some(EventType::Chat.as_str())
    );
    assert_eq!(
        leased.field(field::WORKSPACE_ID),
        Some(lane.workspace.as_str())
    );
    assert_eq!(leased.field(field::REQUEST_JSON), Some(REQUEST_JSON));

    // The readiness mark is what makes the message PROMPTLY leasable rather
    // than waiting for the next poll. It is written after the append and its
    // failure is logged rather than raised, so a test that only checked the
    // append would never notice the mark going missing.
    //
    // Read by exact field rather than through `ReadyIndex::peek`: that samples
    // with `HRANDFIELD`, and the index is shared by every suite in the lane, so
    // a window smaller than the marked set would drop this fleet at random.
    // `HGET` also lets the TOKEN be asserted, which `peek` would have made
    // incidental — and the token is load-bearing. `Steer` writes the fleet id
    // as its own token so the poll-site clear can compare it; a mark written
    // under any other value is one `clear_if_unchanged` can never remove, and
    // the fleet would be polled forever.
    assert_eq!(
        ready_mark(&lane, &lane.fleet).await.as_deref(),
        Some(lane.fleet.as_str()),
        "the steered fleet must be marked ready, under itself as the token; \
         without the mark the message waits for a poll it was appended to skip"
    );

    ReadyIndex::new(lane.queue.clone())
        .force_clear(&lane.fleet)
        .await
        .expect("the readiness mark is cleared");
    streams
        .forget(&lane.fleet)
        .await
        .expect("the fixture stream is removed");
    lane.cleanup().await;
}

/// Dimension 5.3's second half — two identical steers are two messages.
///
/// The positive assertion of the absence documented in this file's header. A
/// person who sends the same sentence twice meant it twice, and the daemon has
/// no key with which to decide otherwise; both appends therefore mint their own
/// entry and both are leasable. Should a dedup ever be introduced, this is the
/// test that fails and says so.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn test_steer_repeats_are_two_messages_not_one() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.queue.clone());

    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("the consumer group is created");

    let first = steer
        .append(&lane.fleet, &lane.workspace, ACTOR_MACHINE, REQUEST_JSON)
        .await
        .expect("the first append reaches the queue");
    let second = steer
        .append(&lane.fleet, &lane.workspace, ACTOR_MACHINE, REQUEST_JSON)
        .await
        .expect("the second append reaches the queue");

    assert_ne!(
        first, second,
        "each steer mints its own entry: there is no idempotency key on this \
         route in either daemon, and the OpenAPI defines none"
    );

    // And both are really there — distinct ids could otherwise be two names for
    // one entry, which is what an assertion on the answers alone would allow.
    let leased_first = streams
        .read_new(&lane.fleet, CONSUMER)
        .await
        .expect("the read reaches the queue")
        .expect("the first entry is deliverable");
    let leased_second = streams
        .read_new(&lane.fleet, CONSUMER)
        .await
        .expect("the read reaches the queue")
        .expect("the second entry is deliverable");

    assert_eq!(leased_first.id.as_str(), first);
    assert_eq!(leased_second.id.as_str(), second);

    ReadyIndex::new(lane.queue.clone())
        .force_clear(&lane.fleet)
        .await
        .expect("the readiness mark is cleared");
    streams
        .forget(&lane.fleet)
        .await
        .expect("the fixture stream is removed");
    lane.cleanup().await;
}

/// The readiness mark held for one fleet, or `None` when it carries none.
///
/// Straight `HGET` against the index key `afd_redis` publishes, because the
/// crate's own reader samples at random by design and this assertion needs the
/// one field.
async fn ready_mark(lane: &EventsLane, fleet: &str) -> Option<String> {
    let mut cmd = redis::cmd("HGET");
    cmd.arg(READY_INDEX_KEY).arg(fleet);
    lane.queue
        .command("HGET", READY_INDEX_KEY, &cmd)
        .await
        .expect("the readiness index answers")
}
