//! A chat lease's earlier turns, read from a live thread.
//!
//! The unit suite beside `lease/history.rs` proves the filter and the caps on
//! rows it builds; this proves the read itself: the statement's cursor, its
//! fleet scope, and its status predicate. The lease a runner is handed is
//! proven through the plane in `integration_lease_gates/history.rs`, which
//! owns the provider seed an issued lease needs.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::test_util::{EventKind, FleetEventSeed, FleetSeed, TenantSeed, WorkspaceSeed, mint_id};
use afd_events::{Cursor, History};
use afd_fleet::lease::history::turns_before;
use afd_wire::event::EventType;
use afd_wire::lease::{ANSWER_FAILED, ANSWER_FAILED_END};
use jiff::Timestamp;
use std::time::Duration;

use crate::support::Fixtures;

/// The instant the scaffold rows are stamped with.
const SEED: Timestamp = Timestamp::constant(1_700_000_000, 0);

/// `millis` past [`SEED`], the instant one seeded event is stamped with.
fn seeded_at(millis: u64) -> Timestamp {
    afd_core::clock::after(SEED, Duration::from_millis(millis))
}

/// A tenant and workspace with two fleets, each with its own thread.
struct Threads {
    workspace: String,
    fleet: String,
    other_fleet: String,
}

impl Threads {
    /// Seeds the tenant, workspace and two fleets the events hang from.
    async fn seed(database: &Db) -> Self {
        let threads = Self {
            workspace: mint_id(),
            fleet: mint_id(),
            other_fleet: mint_id(),
        };
        let tenant = mint_id();
        let mut connection = database.acquire().await.expect("a connection");
        TenantSeed::new(&tenant)
            .at(SEED)
            .insert(&mut *connection)
            .await
            .expect("seeding a tenant");
        WorkspaceSeed::new(&threads.workspace, &tenant)
            .at(SEED)
            .insert(&mut *connection)
            .await
            .expect("seeding a workspace");
        for fleet in [&threads.fleet, &threads.other_fleet] {
            FleetSeed::new(
                fleet,
                &threads.workspace,
                &tenant,
                afd_state::sql::FLEET_STATUS_ACTIVE,
            )
            .name(fleet)
            .at(SEED)
            .insert(&mut *connection)
            .await
            .expect("seeding a fleet");
        }
        threads
    }

    /// One chat event in `fleet`, asked `message`, ended `status` with `answer`.
    async fn event(
        &self,
        database: &Db,
        fleet: &str,
        event_id: &str,
        at: Timestamp,
        (message, status, answer): (&str, &str, Option<&str>),
    ) {
        let mut connection = database.acquire().await.expect("a connection");
        let kind = EventKind {
            actor: "steer:api",
            event_type: EventType::Chat.as_str(),
            status,
        };
        let row = FleetEventSeed::new(fleet, &self.workspace, event_id, kind)
            .at(at)
            .insert(&mut *connection)
            .await
            .expect("seeding a fleet event");
        sqlx::query(
            "UPDATE fleet.events SET request_json = $2::jsonb, response_text = $3 \
             WHERE id = $1::uuid",
        )
        .bind(row)
        .bind(serde_json::json!({ "message": message }).to_string())
        .bind(answer)
        .execute(&mut *connection)
        .await
        .expect("the seeded event carries its turn");
    }

    /// Records `label` as the failure `fleet`'s event `event_id` ended on.
    async fn label(&self, database: &Db, fleet: &str, event_id: &str, label: &str) {
        let mut connection = database.acquire().await.expect("a connection");
        sqlx::query(
            "UPDATE fleet.events SET failure_reason = $3
             WHERE fleet_id = $1::uuid AND event_id = $2",
        )
        .bind(fleet)
        .bind(event_id)
        .bind(label)
        .execute(&mut *connection)
        .await
        .expect("labelling a failed event");
    }

    /// The turns a chat lease on `fleet`'s event at `at` carries.
    async fn turns_at(
        &self,
        database: &Db,
        fleet: &str,
        event_id: &str,
        at: Timestamp,
    ) -> Vec<(String, String)> {
        let workspace = Uuid7::parse(&self.workspace).expect("a minted id");
        let fleet = Uuid7::parse(fleet).expect("a minted id");
        let cursor = Cursor {
            created_at: at,
            event_id: event_id.to_owned(),
        };
        turns_before(
            &History::new(database.clone()),
            &workspace,
            &fleet,
            &cursor,
            EventType::Chat,
        )
        .await
        .into_iter()
        .map(|turn| (turn.message.into_owned(), turn.answer.into_owned()))
        .collect()
    }
}

/// A read before a fleet's second chat message returns the first message and
/// the fleet's answer to it; the second itself, and anything after, is not a
/// turn.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_history_read_carries_the_previous_turn() {
    let fixtures = Fixtures::create().await;
    let database = &fixtures.database;
    let threads = Threads::seed(database).await;
    let fleet = threads.fleet.clone();
    threads
        .event(
            database,
            &fleet,
            "e1",
            seeded_at(1),
            (
                "which tests failed?",
                status::PROCESSED,
                Some("two: a and b"),
            ),
        )
        .await;
    threads
        .event(
            database,
            &fleet,
            "e2",
            seeded_at(2),
            ("fix the second one", "queued", None),
        )
        .await;

    let turns = threads.turns_at(database, &fleet, "e2", seeded_at(2)).await;

    assert_eq!(
        turns,
        [("which tests failed?".to_owned(), "two: a and b".to_owned())]
    );
    fixtures.cleanup().await;
}

/// Another fleet's rows never appear in a lease's turns.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_history_stays_in_its_fleet() {
    let fixtures = Fixtures::create().await;
    let database = &fixtures.database;
    let threads = Threads::seed(database).await;
    let (fleet, other) = (threads.fleet.clone(), threads.other_fleet.clone());
    threads
        .event(
            database,
            &other,
            "o1",
            seeded_at(1),
            ("someone else's", status::PROCESSED, Some("theirs")),
        )
        .await;
    threads
        .event(
            database,
            &fleet,
            "e1",
            seeded_at(2),
            ("mine", status::PROCESSED, Some("ours")),
        )
        .await;

    let turns = threads.turns_at(database, &fleet, "e2", seeded_at(3)).await;

    assert_eq!(turns, [("mine".to_owned(), "ours".to_owned())]);
    fixtures.cleanup().await;
}

/// Rows still running or refused among the newest never take a turn's place:
/// nine finished rows under three unfinished ones still carry the eight newest
/// finished turns.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_unfinished_rows_never_shrink_the_window() {
    let fixtures = Fixtures::create().await;
    let database = &fixtures.database;
    let threads = Threads::seed(database).await;
    let fleet = threads.fleet.clone();
    let unfinished = [status::RECEIVED, status::GATE_BLOCKED, status::RECEIVED];
    let rows = (1..=9)
        .map(|index| (index, status::PROCESSED, Some("a")))
        .chain(
            (10..)
                .zip(unfinished)
                .map(|(index, ended)| (index, ended, None)),
        );
    for (index, ended, answer) in rows {
        let message = format!("m{index}");
        let at = seeded_at(index);
        let event_id = format!("e{index}");
        threads
            .event(database, &fleet, &event_id, at, (&message, ended, answer))
            .await;
    }

    let turns = threads
        .turns_at(database, &fleet, "e13", seeded_at(13))
        .await;

    let messages: Vec<&str> = turns.iter().map(|(message, _)| message.as_str()).collect();
    assert_eq!(messages, ["m2", "m3", "m4", "m5", "m6", "m7", "m8", "m9"]);
    fixtures.cleanup().await;
}

/// The read stops at its own event: a finished row stamped after it, which a
/// later message's run left, is no turn of this one's.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_history_read_excludes_finished_rows_after_its_event() {
    let fixtures = Fixtures::create().await;
    let database = &fixtures.database;
    let threads = Threads::seed(database).await;
    let fleet = threads.fleet.clone();
    let rows = [
        ("e1", ("first", status::PROCESSED, Some("one"))),
        ("e2", ("second", status::RECEIVED, None)),
        ("e3", ("third", status::PROCESSED, Some("three"))),
    ];
    for (offset, (event_id, row)) in (1..).zip(rows) {
        threads
            .event(database, &fleet, event_id, seeded_at(offset), row)
            .await;
    }

    let turns = threads.turns_at(database, &fleet, "e2", seeded_at(2)).await;

    assert_eq!(turns, [("first".to_owned(), "one".to_owned())]);
    fixtures.cleanup().await;
}

/// A run that ended in failure is still a turn: the read admits the failed
/// status, and the turn answers with the failure the row recorded.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_history_read_carries_a_failed_turn() {
    const LABEL: &str = "timeout_kill";
    let fixtures = Fixtures::create().await;
    let database = &fixtures.database;
    let threads = Threads::seed(database).await;
    let fleet = threads.fleet.clone();
    threads
        .event(
            database,
            &fleet,
            "e1",
            seeded_at(1),
            ("run the suite", status::FLEET_ERROR, None),
        )
        .await;
    threads.label(database, &fleet, "e1", LABEL).await;

    let turns = threads.turns_at(database, &fleet, "e2", seeded_at(2)).await;

    let failed = format!("{ANSWER_FAILED}{LABEL}{ANSWER_FAILED_END}");
    assert_eq!(turns, [("run the suite".to_owned(), failed)]);
    fixtures.cleanup().await;
}
