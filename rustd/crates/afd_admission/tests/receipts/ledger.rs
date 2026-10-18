//! The row itself: the id an event is named by and the clock that stamps it.
//!
//! Each case reads the row back with its own SQL rather than through the
//! crate, because what is under test is what Postgres holds, and a read through
//! the same statements the crate writes with would agree with them by
//! construction.

use std::time::Duration;

use afd_admission::{Admission, Admissions, Admitted, Key, Producer, Reply as ReplyTo};
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_wire::event::EventType;
use jiff::Timestamp;
use sqlx::Connection as _;

use super::{lane::Lane, queue::over_fake};
use crate::fake_redis::Reply;

/// The entry id the fake's `XADD` hands back.
pub(super) const FAKE_RECEIPT: &str = "1790000000000-0";

/// An append the queue refuses, so the row is left without a receipt.
const XADD_REFUSED: &str = "-ERR the queue refused the append\r\n";

/// A connector and a thread, as a producer owning a reply surface states them.
pub(super) const CONNECTOR: &str = "slack";
pub(super) const THREAD: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// A minimum age no fixture row can reach.
const AN_HOUR: Duration = Duration::from_secs(3_600);

/// How many rows one replay pass may take: more than any lane holds.
const REPLAY_LIMIT: i64 = 10_000;

/// The instants and the sequence Postgres stamped one row with.
#[derive(Debug, sqlx::FromRow)]
struct Stamps {
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    event_created_at: Timestamp,
    sequence: i64,
}

/// A steer on the lane's fleet under `key`, replying where `reply` says.
pub(super) fn steer<'a>(lane: &'a Lane, key: &'a str, reply: ReplyTo<'a>) -> Admission<'a> {
    Admission {
        producer: Producer::Steer,
        key: Key::Repeated(key),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: "fixture:ledger",
        event_type: EventType::Chat,
        request_json: r#"{"message":"ledger fixture"}"#,
        reply,
    }
}

pub(super) async fn admit(admissions: &Admissions, admission: Admission<'_>) -> Admitted {
    admissions
        .admit(admission)
        .await
        .expect("the ledger answers")
}

async fn stamps(lane: &Lane, id: &str) -> Stamps {
    sqlx::query_as(
        "SELECT created_at, updated_at, event_created_at, sequence \
         FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(id)
    .fetch_one(&mut *lane.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the admitted row is there")
}

/// The event id an admission answers with is its row's `UUIDv7`, and a retry
/// under the same key answers the same one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_admitted_event_is_named_by_its_row_id() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;
    let key = mint_id();

    let first = admit(&admissions, steer(&lane, &key, ReplyTo::None)).await;
    let retried = admit(&admissions, steer(&lane, &key, ReplyTo::None)).await;

    let row: String =
        sqlx::query_scalar("SELECT id::text FROM fleet.admissions WHERE producer_key = $1")
            .bind(&key)
            .fetch_one(&mut *lane.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("one row holds the key");
    assert_eq!(first.stored.id, row, "the event id is the row id");
    assert!(
        Uuid7::parse(&first.stored.id).is_ok(),
        "{}",
        first.stored.id
    );
    assert!(retried.replayed);
    assert_eq!(
        retried.stored.id, first.stored.id,
        "a retry answers the first id"
    );
    assert_eq!(lane.receipt().await.as_deref(), Some(FAKE_RECEIPT));
    lane.cleanup().await;
}

/// Every instant on the row is the database's: one `now()` for all three, and
/// the one the producer was answered with.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_database_stamps_every_instant_on_the_row() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;

    let admitted = admit(&admissions, steer(&lane, &mint_id(), ReplyTo::None)).await;
    let row = stamps(&lane, &admitted.stored.id).await;

    assert_eq!(row.created_at, row.event_created_at);
    assert!(
        row.updated_at >= row.created_at,
        "the receipt write moves only updated_at"
    );
    assert_eq!(admitted.stored.created_at, row.created_at);
    lane.cleanup().await;
}

/// Two admissions staged in one transaction share its `now()` to the
/// microsecond, and still sort in the order they were inserted: inside one
/// instant, `sequence` decides.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_admissions_in_one_instant_order_by_their_sequence() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let (first_key, second_key) = (mint_id(), mint_id());
    let first = admissions
        .prepare(steer(&lane, &first_key, ReplyTo::None))
        .await
        .expect("budget observation");
    let second = admissions
        .prepare(steer(&lane, &second_key, ReplyTo::None))
        .await
        .expect("budget observation");

    let mut connection = lane.database.acquire().await.expect("a connection");
    let mut transaction = connection.begin().await.expect("begin");
    let first = admissions
        .stage(&mut transaction, first)
        .await
        .expect("stage");
    let second = admissions
        .stage(&mut transaction, second)
        .await
        .expect("stage");
    transaction.commit().await.expect("commit");
    drop(connection);

    let (first_row, second_row) = (
        stamps(&lane, &first.stored.id).await,
        stamps(&lane, &second.stored.id).await,
    );
    assert_eq!(
        first_row.created_at, second_row.created_at,
        "one transaction, one now()"
    );
    assert!(
        first_row.sequence < second_row.sequence,
        "the sequence keeps insertion order"
    );
    let ordered: Vec<String> = sqlx::query_scalar(
        "SELECT id::text FROM fleet.admissions WHERE fleet_id = $1::uuid \
         ORDER BY created_at, sequence",
    )
    .bind(&lane.fleet)
    .fetch_all(&mut *lane.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the fleet's rows read");
    assert_eq!(ordered, [first.stored.id, second.stored.id]);
    lane.cleanup().await;
}

/// The replay window is measured on the database's clock: a row younger than
/// the minimum age is left to its inserter, and one older is taken.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_replay_leaves_rows_younger_than_its_minimum_age() {
    let lane = Lane::seed().await;
    let (server, admissions) = over_fake(&lane, &[("XADD", Reply::Raw(XADD_REFUSED))]).await;
    admit(&admissions, steer(&lane, &mint_id(), ReplyTo::None)).await;
    assert_eq!(
        lane.receipt().await,
        None,
        "the refused append left no receipt"
    );
    let backlog = admissions.backlog().await.expect("the backlog reads");
    assert!(backlog.rows >= 1, "the unreceipted row is counted");
    assert!(backlog.oldest_age.is_some(), "and has an age");

    // The queue takes appends from here on, so only the window decides.
    server.set_reply("XADD", Reply::Bulk(FAKE_RECEIPT));
    admissions
        .replay(AN_HOUR, REPLAY_LIMIT)
        .await
        .expect("a pass runs");
    assert_eq!(
        lane.receipt().await,
        None,
        "a row seconds old is inside an hour's window"
    );

    // Aged on the database's clock rather than replayed with no window, so the
    // pass takes this fleet's row and none a concurrent suite is still writing.
    sqlx::query(
        "UPDATE fleet.admissions SET created_at = created_at - interval '2 hours' \
         WHERE fleet_id = $1::uuid",
    )
    .bind(&lane.fleet)
    .execute(&mut *lane.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the row ages");
    admissions
        .replay(AN_HOUR, REPLAY_LIMIT)
        .await
        .expect("a pass runs");
    assert_eq!(lane.receipt().await.as_deref(), Some(FAKE_RECEIPT));
    lane.cleanup().await;
}
