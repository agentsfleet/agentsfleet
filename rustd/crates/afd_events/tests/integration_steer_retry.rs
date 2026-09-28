//! Dimension 7.5 — a steer retried after a lost response admits once.
//!
//! A timeout does not prove an operation failed. The client that never saw its
//! 202 has no way to tell a message the daemon dropped from one it accepted and
//! could not answer for, so the only safe thing it can do is send again — and
//! the only thing that makes sending again safe is an identity the retry
//! carries. `Steer::append` takes that identity as `operation_id` and hands it
//! to the ledger as `Key::Repeated` (`afd_events::steer`), where
//! `UNIQUE (producer, producer_key)` on `core.fleet_admissions` is what admits
//! it once.
//!
//! The claim is deliberately proven at THREE depths, because any one of them
//! alone is satisfiable by a bug:
//!
//! - the two calls answer the same event id — but so would a second row that
//!   happened to reuse an id;
//! - the ledger holds one row for the key — but a row deduplicated after its
//!   entry was appended still delivers twice;
//! - the stream holds one entry — which is the one a runner leases, and
//!   therefore the one the tenant is charged for and the provider is asked to
//!   run.
//!
//! The companion test holds the other edge. Deduplication that cannot be turned
//! off is worse than none: two DIFFERENT operation ids carrying identical text
//! are a person who meant it twice, and both must land. `integration_steer`'s
//! `test_steer_repeats_are_two_messages_not_one` proves the same for a caller
//! that sends no identity at all.
//!
//! Marked `#[ignore]` so `make test-unit-all` still COMPILES and lints this
//! without a datastore, and `make test-integration-rustd` is the only lane that
//! executes it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_dragonfly::ReadyIndex;
use afd_dragonfly::streams::FleetStreams;
use afd_events::{ACTOR_MACHINE, Steer};
use afd_wire::event::field;
use sqlx::Row as _;

use crate::support::EventsLane;

/// The consumer name the test reads the stream back under.
///
/// Distinct from `integration_steer`'s: the two files share the lane's queue,
/// and a name in common would let one suite's read consume the other's entry.
const CONSUMER: &str = "steer-retry-integration-reader";

/// The payload a caller's steer carries, already serialized by the handler.
pub(crate) const REQUEST_JSON: &str = r#"{"message":"redeploy staging"}"#;

/// The identity a client repeats across its retries.
///
/// A literal is safe now that the steer layer scopes the key to the fleet: the
/// lane mints a fresh fleet per `open`, so this id can meet neither a
/// neighbour in the same binary nor a row an earlier run left behind.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0999";

/// A second, DIFFERENT identity carrying the same text.
///
/// The other edge of the same rule: equal bodies are not equal operations, so
/// this one must admit on its own.
const DISTINCT_OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0998";

/// Joins the fleet to the operation id in the ledger key — `afd_events::steer`'s
/// separator, mirrored so a key shape that moved fails here instead of
/// following.
pub(crate) const KEY_SEPARATOR: &str = ":";

/// How the ledger spells this producer, for the row count below.
pub(crate) const PRODUCER_STEER: &str = "steer";

/// Dimension 7.5 — the same operation id twice admits once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_retry_reuses_its_admission() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.admissions());

    // The group has to exist before the append, because `read_new` delivers
    // through it. Created afterwards with `$` it would see nothing, and this
    // test would report a missing entry for a reason that is its own.
    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("the consumer group is created");

    let answered = append_with(&steer, &lane, Some(OPERATION)).await;
    // The retry: same identity, same text, as a client that never saw its 202
    // would send it.
    let retried = append_with(&steer, &lane, Some(OPERATION)).await;

    assert_eq!(
        answered, retried,
        "a retry carrying the operation id it already sent must be answered \
         with the FIRST admission's event id -- a client polling the id it was \
         handed would otherwise watch an event that never runs"
    );
    assert_eq!(
        admissions_for(&lane, OPERATION).await,
        1,
        "one ledger row for the key: the UNIQUE (producer, producer_key) on \
         core.fleet_admissions is what makes the retry free, and a second row \
         is a second unit of accepted work the replay sweeper will deliver"
    );

    // The entry is what a runner leases, so it is the depth that decides
    // whether the tenant pays twice and the provider runs twice.
    let leased = streams
        .read_new(&lane.fleet, CONSUMER)
        .await
        .expect("the read reaches the queue")
        .expect("the admitted steer left an entry to lease");
    assert_eq!(
        leased.field(field::EVENT_ID),
        Some(answered.as_str()),
        "the entry carries the LOGICAL id the ledger minted, which is what the \
         client was answered with -- `receipt` is the stream's own id for the \
         physical copy, and one logical event legitimately sits on two of those \
         after a replay"
    );
    assert!(
        streams
            .read_new(&lane.fleet, CONSUMER)
            .await
            .expect("the read reaches the queue")
            .is_none(),
        "the retry appended NO second entry -- a ledger that deduplicates \
         after the append still delivers the message twice, which is the \
         failure this dimension exists to refuse"
    );

    clean(&lane, &streams).await;
}

/// Dimension 7.5's other edge — two identities carrying equal text admit twice.
///
/// The dedup must be keyed on what the CLIENT called one operation, never on
/// what the payload happens to say. A person who sends the same sentence under
/// a new operation id meant it twice, and a daemon that collapsed them would
/// silently drop the second with nothing to appeal to.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_two_operation_ids_with_equal_text_admit_twice() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.admissions());

    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("the consumer group is created");

    let first = append_with(&steer, &lane, Some(OPERATION)).await;
    let second = append_with(&steer, &lane, Some(DISTINCT_OPERATION)).await;

    assert_ne!(
        first, second,
        "equal bodies are not equal operations: each identity admits on its \
         own, and collapsing them would drop a message the sender meant"
    );

    // Both really landed. Distinct answers alone would also be satisfied by two
    // names for one entry.
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
    assert_eq!(leased_first.field(field::EVENT_ID), Some(first.as_str()));
    assert_eq!(leased_second.field(field::EVENT_ID), Some(second.as_str()));

    clean(&lane, &streams).await;
}

/// One steer through the production path, answering its event id.
pub(crate) async fn append_with(
    steer: &Steer,
    lane: &EventsLane,
    operation: Option<&str>,
) -> String {
    steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            operation,
        )
        .await
        .expect("the append reaches the datastore")
        .event_id
}

/// Ledger rows the steer producer holds for `operation` on the lane's fleet.
///
/// Counted on `(producer, producer_key)` -- the pair the unique constraint is
/// declared over -- with the key spelled the way the steer layer composes it,
/// so a key shape that drifted counts zero here and fails the assertion.
pub(crate) async fn admissions_for(lane: &EventsLane, operation: &str) -> i64 {
    let key = [lane.fleet.as_str(), operation].join(KEY_SEPARATOR);
    let mut connection = lane.connection().await;
    sqlx::query(
        "SELECT count(*) FROM core.fleet_admissions \
         WHERE producer = $1 AND producer_key = $2",
    )
    .bind(PRODUCER_STEER)
    .bind(key)
    .fetch_one(&mut *connection)
    .await
    .expect("the ledger count must run")
    .try_get(0)
    .expect("count answers a bigint")
}

/// Returns the lane's shared state to where the next suite expects it.
pub(crate) async fn clean(lane: &EventsLane, streams: &FleetStreams) {
    ReadyIndex::new(lane.queue.clone())
        .force_clear(&lane.fleet)
        .await
        .expect("the readiness mark is cleared");
    streams
        .forget(&lane.fleet)
        .await
        .expect("the fixture stream is removed");
}
