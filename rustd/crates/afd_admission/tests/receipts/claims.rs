//! A replay pass claims its rows before it appends them: two sweepers racing
//! over one backlog append each row once, and a row another pass claimed waits
//! until that claim lapses.

use std::collections::HashMap;
use std::time::Duration;

use afd_admission::Reply as ReplyTo;
use afd_db::test_util::mint_id;
use jiff::{SignedDuration, Timestamp};
use jiff_sqlx::ToSqlx as _;

use super::lane::Lane;
use super::ledger::{FAKE_RECEIPT, admit, steer};
use super::queue::{over_fake, recording};
use crate::fake_redis::Reply;

/// An append the queue refuses, so each admitted row is left owed.
const XADD_REFUSED: &str = "-ERR the queue refused the append\r\n";

/// A minimum age this suite's own rows are aged past and no row another suite
/// is still writing has reached.
const AN_HOUR: Duration = Duration::from_secs(3_600);

/// Rows each pass may claim: small, so two passes over one backlog race for it.
const RACE_LIMIT: i64 = 2;

/// Rows one pass may take when every owed row in the lane must be reachable.
const REPLAY_ALL: i64 = 10_000;

/// Rows the race is run over.
const RACED_ROWS: usize = 8;

/// Passes either sweeper may take before the backlog must be drained. A sibling
/// suite's pass can briefly hold one of these rows; a few extra rounds absorb it.
const ROUNDS: usize = 40;

/// The events a pass logs for each row it appended.
const APPENDED_EVENTS: [&str; 2] = ["admission_replayed_to_queue", "admission_replay_superseded"];

/// Admits `count` rows on `lane`'s fleet that the queue refused, then ages them
/// on the database's clock past [`AN_HOUR`], so only this suite's passes
/// window them. Answers their ids.
async fn owed_rows(lane: &Lane, count: usize) -> Vec<String> {
    let (_server, admissions) = over_fake(lane, &[("XADD", Reply::Raw(XADD_REFUSED))]).await;
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let admitted = admit(&admissions, steer(lane, &mint_id(), ReplyTo::None)).await;
        ids.push(admitted.stored.id);
    }
    sqlx::query(
        "UPDATE fleet.admissions SET created_at = created_at - interval '2 hours' \
         WHERE fleet_id = $1::uuid",
    )
    .bind(&lane.fleet)
    .execute(&mut *lane.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the rows age");
    ids
}

/// How many of `lane`'s fleet's rows still owe a receipt.
async fn owed(lane: &Lane) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1::uuid AND receipt IS NULL",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the backlog counts")
}

/// Two sweepers, each over its own queue, race passes over one backlog until
/// it drains; no row is appended by both.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_racing_sweepers_append_each_row_once() {
    let lane = Lane::seed().await;
    let ids = owed_rows(&lane, RACED_ROWS).await;
    let (_first_queue, first) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;
    let (_second_queue, second) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;

    let mut appends: HashMap<String, usize> = HashMap::new();
    for _ in 0..ROUNDS {
        let ((left, right), events) = recording(async {
            tokio::join!(
                first.replay(AN_HOUR, RACE_LIMIT),
                second.replay(AN_HOUR, RACE_LIMIT)
            )
        })
        .await;
        left.expect("the first sweeper's pass runs");
        right.expect("the second sweeper's pass runs");
        for fields in events {
            let appended = fields
                .get("event")
                .is_some_and(|event| APPENDED_EVENTS.contains(&event.as_str()));
            if let (true, Some(event_id)) = (appended, fields.get("event_id")) {
                *appends.entry(event_id.clone()).or_default() += 1;
            }
        }
        if owed(&lane).await == 0 {
            break;
        }
    }

    assert_eq!(owed(&lane).await, 0, "the two sweepers drained the backlog");
    for id in &ids {
        assert!(
            appends.get(id).copied().unwrap_or_default() <= 1,
            "row {id} was appended {} times",
            appends.get(id).copied().unwrap_or_default()
        );
    }
    assert!(
        ids.iter().any(|id| appends.contains_key(id)),
        "the race appended this suite's rows, so the check above saw them"
    );
    lane.cleanup().await;
}

/// A row claimed by a pass that has not finished is left alone, and replayed
/// once that claim lapses — the recovery for a pass that died mid-batch.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_claimed_row_waits_until_its_claim_lapses() {
    let lane = Lane::seed().await;
    let _owed = owed_rows(&lane, 1).await;
    let claim = |expires_at: Timestamp| {
        let lane = &lane;
        async move {
            sqlx::query(
                "UPDATE fleet.admissions SET replay_claim_expires_at = $2 \
                 WHERE fleet_id = $1::uuid",
            )
            .bind(&lane.fleet)
            .bind(expires_at.to_sqlx())
            .execute(&mut *lane.database.acquire().await.expect("a pooled connection"))
            .await
            .expect("the claim is stamped");
        }
    };
    let (_queue, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;

    claim(Timestamp::now() + SignedDuration::from_hours(1)).await;
    admissions
        .replay(AN_HOUR, REPLAY_ALL)
        .await
        .expect("a pass runs");
    assert_eq!(
        owed(&lane).await,
        1,
        "a live claim keeps the row from this pass"
    );

    claim(Timestamp::now() - SignedDuration::from_hours(1)).await;
    admissions
        .replay(AN_HOUR, REPLAY_ALL)
        .await
        .expect("a pass runs");
    assert_eq!(
        lane.receipt().await.as_deref(),
        Some(FAKE_RECEIPT),
        "a lapsed claim is replayed"
    );
    lane.cleanup().await;
}
