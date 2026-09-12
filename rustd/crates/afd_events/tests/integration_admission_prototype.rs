//! Dimension 0.3 — a ledger-first acceptance replayed after queue loss yields
//! one logical event per producer identity, whatever order receipts arrive in.
//!
//! The shape under measurement is the one §2 adopts, prototyped here before
//! the boundary exists: the durable ledger row is committed FIRST, and the
//! queue append is a consequence of a row that is already safe. The logical
//! event id belongs to the ledger, not to the stream, so a producer that is
//! replayed re-appends the identifier it was already given rather than minting
//! a second one. That single inversion is what makes queue loss survivable —
//! the queue holds a derived copy, and the recoverable original is in Postgres.
//!
//! What this refuses is the shape the crate ships today, where the stream entry
//! id IS the identity: lose the stream and the identity is gone with it, and a
//! retry after loss is indistinguishable from a second request.
//!
//! # What the assertions are, and what they deliberately are not
//!
//! One logical event per identity, and one settlement per identity. NOT one
//! physical entry per identity: after a replay the stream may legitimately hold
//! a second physical copy of a logical event whose first copy was lost, and the
//! architecture page says as much. Settlement is idempotent BECAUSE the ledger
//! is unique on identity, not because delivery is; that is the invariant the
//! spec states ("no exactly-once promise for external side effects") and
//! asserting a stronger one here would encode a promise the design never made.
//!
//! # Why a temporary table
//!
//! The ledger this measures is not a schema change — §2 decides the real
//! table's name, keys and migration. A `TEMP TABLE` gives the prototype the
//! uniqueness and transaction semantics it is actually testing without
//! proposing a column anyone has to live with. Temporary tables are scoped to
//! one session, so the whole test holds ONE pooled connection; taking a second
//! from the pool would reach a session where the table does not exist.
//!
//! Marked `#[ignore]` so `make test-unit-all` still compiles and lints this
//! without datastores, and `make test-integration-rustd` is the only lane that
//! runs it.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use redis::cluster::ClusterClientBuilder;
use redis::cluster_async::ClusterConnection;
use redis::{ProtocolVersion, Value};
use sqlx::Row as _;

use crate::support::EventsLane;

/// The knob `make test-integration-rustd` exports the cluster's seed under —
/// the same name `afd_redis`'s cluster harness reads.
const URL_KNOB: &str = "TEST_DRAGONFLY_URL";

/// How long one reply may take, and how long a dial may.
const RESPONSE_BUDGET: Duration = Duration::from_secs(5);
const CONNECT_BUDGET: Duration = Duration::from_secs(5);

/// How many times the driver follows a redirect before giving up.
const REDIRECT_RETRIES: u32 = 8;

/// How many producers the run admits. Enough that an ordering bug shows as
/// several wrong rows rather than one that could be a fluke.
const PRODUCERS: usize = 12;

/// Commands this suite issues by name, once each per RULE UFS.
const CMD_XADD: &str = "XADD";
const CMD_XLEN: &str = "XLEN";
const CMD_DEL: &str = "DEL";

/// The field every stream entry carries the ledger's logical id in.
const FIELD_EVENT: &str = "event_id";

/// Distinguishes one admission ATTEMPT from the next.
///
/// Every call to [`admit`] proposes a different candidate id, which is what
/// makes the replay assertion load-bearing: if the ledger ever inserted afresh
/// instead of returning the row it already holds, the replayed id would differ
/// from the original and the test would say so. A candidate derived from the
/// identity alone would be equal either way, and would prove nothing.
static ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// Where an admission was interrupted. The three stops are the three
/// boundaries a crash can fall on once the ledger is written first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// The ledger row committed; the queue never heard about it.
    AfterCommit,
    /// The entry was appended; the receipt was never recorded.
    AfterAppend,
    /// Nothing was interrupted.
    None,
}

impl Stop {
    /// The stop for producer `index`, cycling so each stop is exercised by
    /// four of the twelve producers.
    fn for_index(index: usize) -> Self {
        match index % 3 {
            0 => Self::AfterCommit,
            1 => Self::AfterAppend,
            _ => Self::None,
        }
    }
}

/// Dimension 0.3 — one logical event per identity across queue loss and
/// receipts replayed in reverse.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live datastores: make test-integration-rustd"]
async fn test_durable_identity_survives_replay() {
    let lane = EventsLane::open().await;
    let mut queue = connect_cluster().await;
    // One slot for the whole prototype: the hash tag is the fleet, which is
    // the co-location the target layout relies on, and `DEL` below is then a
    // single-key command rather than a cross-slot one.
    let stream = format!("{{{}}}:admission", lane.fleet);
    let mut ledger = lane.connection().await;

    create_ledger(&mut ledger).await;

    // First pass: every producer commits its ledger row, and then stops where
    // its schedule says. The identity is the producer's own, stable across
    // both passes, and is all a replay has to go on.
    let identities: Vec<String> = (0..PRODUCERS)
        .map(|index| format!("{}-producer-{index}", lane.fleet))
        .collect();
    for (index, identity) in identities.iter().enumerate() {
        let stop = Stop::for_index(index);
        let event = admit(&mut ledger, identity).await;
        if stop == Stop::AfterCommit {
            continue;
        }
        append(&mut queue, &stream, &event).await;
        if stop == Stop::AfterAppend {
            continue;
        }
        settle(&mut ledger, identity).await;
    }

    let before_loss = admitted(&mut ledger).await;
    assert_eq!(
        before_loss.len(),
        PRODUCERS,
        "the ledger holds one row per identity before anything is lost"
    );

    // Queue loss: the stream this test owns is deleted outright. The ledger is
    // untouched, which is the whole point of committing it first.
    let dropped = del(&mut queue, &stream).await;
    assert_eq!(dropped, 1, "the test's own stream existed and was deleted");
    assert_eq!(
        xlen(&mut queue, &stream).await,
        0,
        "queue loss leaves nothing behind to recover from"
    );

    // Replay, in reverse: recovery reads the ledger and re-drives each
    // admission. Reverse order is the point — receipts arrive in whatever order
    // recovery happens to walk, and the outcome may not depend on it.
    for identity in identities.iter().rev() {
        let replayed = admit(&mut ledger, identity).await;
        let original = before_loss
            .get(identity)
            .expect("every identity was admitted in the first pass");
        assert_eq!(
            &replayed, original,
            "a replayed admission returns the identity's EXISTING logical event, never a new one"
        );
        append(&mut queue, &stream, &replayed).await;
        settle(&mut ledger, identity).await;
    }

    // One logical event per identity, unchanged by the replay.
    let after_replay = admitted(&mut ledger).await;
    assert_eq!(
        after_replay, before_loss,
        "replay mints no identity and changes no logical event id"
    );

    // Settlement happened once per identity, though four producers were
    // settled in the first pass and every one of the twelve in the second.
    let settled = settlements(&mut ledger).await;
    assert_eq!(
        settled.len(),
        PRODUCERS,
        "every identity is settled after recovery"
    );
    for (identity, count) in &settled {
        assert_eq!(
            *count, 1,
            "identity {identity} settled more than once, which is a duplicate debit"
        );
    }

    // Receipts restored: the rebuilt stream carries one entry per identity.
    // Physical count equals logical count here only because the stream was
    // emptied first — this is the recovered state, not a dedup claim.
    assert_eq!(
        xlen(&mut queue, &stream).await,
        PRODUCERS,
        "recovery rebuilt one physical entry for each logical event"
    );

    println!(
        "evidence: {PRODUCERS} identities survived queue loss and reverse replay with {} logical events and one settlement each",
        after_replay.len()
    );

    del(&mut queue, &stream).await;
    lane.cleanup().await;
}

/// The prototype ledger: unique on the producer's identity, which is what makes
/// a replayed admission a lookup rather than an insert.
async fn create_ledger(connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>) {
    sqlx::query(
        "CREATE TEMP TABLE admission_ledger (
           identity    text PRIMARY KEY,
           event_id    text NOT NULL,
           settlements integer NOT NULL DEFAULT 0
         ) ON COMMIT PRESERVE ROWS",
    )
    .execute(&mut **connection)
    .await
    .expect("the prototype ledger must be creatable in this session");
}

/// Admits `identity`, returning the logical event id it now owns.
///
/// The insert is the acceptance. `ON CONFLICT DO NOTHING` plus the follow-up
/// read is what makes this idempotent: a second call for an identity already
/// admitted returns the id the first call minted, and writes nothing — even
/// though this call proposed a DIFFERENT candidate, which is how the caller's
/// replay assertion can tell the two apart.
async fn admit(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    identity: &str,
) -> String {
    let attempt = ATTEMPT.fetch_add(1, Ordering::Relaxed);
    let minted = format!("{identity}-event-{attempt}");
    sqlx::query(
        "INSERT INTO admission_ledger (identity, event_id)
         VALUES ($1, $2)
         ON CONFLICT (identity) DO NOTHING",
    )
    .bind(identity)
    .bind(&minted)
    .execute(&mut **connection)
    .await
    .expect("an admission must commit before anything is queued");

    sqlx::query("SELECT event_id FROM admission_ledger WHERE identity = $1")
        .bind(identity)
        .fetch_one(&mut **connection)
        .await
        .expect("the row just committed must be readable")
        .get::<String, _>("event_id")
}

/// Records a settlement for `identity`, once and only once.
///
/// The guard is in the predicate, not in the caller: settling an identity that
/// is already settled writes nothing, which is how a replay that re-drives
/// every admission still debits each one a single time.
async fn settle(connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>, identity: &str) {
    sqlx::query(
        "UPDATE admission_ledger
            SET settlements = settlements + 1
          WHERE identity = $1 AND settlements = 0",
    )
    .bind(identity)
    .execute(&mut **connection)
    .await
    .expect("settlement must be recordable");
}

/// Every admitted identity and the logical event it owns.
async fn admitted(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
) -> BTreeMap<String, String> {
    sqlx::query("SELECT identity, event_id FROM admission_ledger")
        .fetch_all(&mut **connection)
        .await
        .expect("the ledger must be readable")
        .into_iter()
        .map(|row| (row.get("identity"), row.get("event_id")))
        .collect()
}

/// Every identity's settlement count.
async fn settlements(
    connection: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
) -> BTreeMap<String, i32> {
    sqlx::query("SELECT identity, settlements FROM admission_ledger")
        .fetch_all(&mut **connection)
        .await
        .expect("the ledger must be readable")
        .into_iter()
        .map(|row| (row.get("identity"), row.get("settlements")))
        .collect()
}

/// Appends the ledger's logical id to the stream, as the queue copy.
async fn append(queue: &mut ClusterConnection, stream: &str, event: &str) {
    let mut xadd = redis::cmd(CMD_XADD);
    xadd.arg(stream).arg("*").arg(FIELD_EVENT).arg(event);
    let appended: Value = xadd
        .query_async(queue)
        .await
        .expect("the queue copy must append");
    assert!(
        !matches!(appended, Value::Nil),
        "an append must answer with an entry id"
    );
}

/// The stream's length, or zero when it does not exist.
///
/// Decoded as `usize` because a length is a count: it is compared against
/// [`PRODUCERS`] and never against a signed quantity, so no conversion exists
/// here to get wrong.
async fn xlen(queue: &mut ClusterConnection, stream: &str) -> usize {
    let mut xlen = redis::cmd(CMD_XLEN);
    xlen.arg(stream);
    xlen.query_async(queue).await.expect("XLEN answers")
}

/// Deletes the stream, returning how many keys went.
async fn del(queue: &mut ClusterConnection, stream: &str) -> usize {
    let mut del = redis::cmd(CMD_DEL);
    del.arg(stream);
    del.query_async(queue).await.expect("DEL answers")
}

/// The lane's cluster over its seed: RESP3, bounded dials and replies, and a
/// redirect allowance — the same builder shape `afd_redis`'s harness uses.
async fn connect_cluster() -> ClusterConnection {
    let seed = std::env::var(URL_KNOB)
        .unwrap_or_else(|_| panic!("{URL_KNOB} must name the lane's Dragonfly cluster"));
    ClusterClientBuilder::new([seed])
        .use_protocol(ProtocolVersion::RESP3)
        .connection_timeout(CONNECT_BUDGET)
        .response_timeout(RESPONSE_BUDGET)
        .retries(REDIRECT_RETRIES)
        .build()
        .expect("the seed URL parses")
        .get_async_connection()
        .await
        .expect("the lane's cluster must be reachable")
}
