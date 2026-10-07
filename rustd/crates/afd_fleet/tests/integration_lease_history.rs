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
use afd_db::test_util::mint_id;
use afd_events::{Cursor, History};
use afd_fleet::lease::history::turns_before;
use afd_wire::event::EventType;
use afd_wire::lease::{ANSWER_FAILED, ANSWER_FAILED_END};

use crate::support::Fixtures;

/// The instant the scaffold rows are stamped with.
const SEED_MS: i64 = 1_700_000_000_000;

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
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, 'history-fixture', $2, $2)",
        )
        .bind(tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
             VALUES ($1::uuid, $2::uuid, $1::text, 'history-fixture', $3)",
        )
        .bind(threads.workspace.as_str())
        .bind(tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a workspace");
        for fleet in [&threads.fleet, &threads.other_fleet] {
            sqlx::query(
                "INSERT INTO core.fleets
                   (id, workspace_id, tenant_id, name, source_markdown, config_json,
                    status, created_at, updated_at)
                 VALUES ($1::uuid, $2::uuid, $3::uuid, $1::text, '# fixture', '{}'::jsonb,
                         'active', $4, $4)",
            )
            .bind(fleet.as_str())
            .bind(threads.workspace.as_str())
            .bind(tenant.as_str())
            .bind(SEED_MS)
            .execute(&mut *connection)
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
        at: i64,
        (message, status, answer): (&str, &str, Option<&str>),
    ) {
        let mut connection = database.acquire().await.expect("a connection");
        sqlx::query(
            "INSERT INTO core.fleet_events
               (fleet_id, workspace_id, event_id, actor, event_type, status,
                request_json, response_text, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3, 'steer:api', 'chat', $4, $5::jsonb, $6, $7, $7)",
        )
        .bind(fleet)
        .bind(self.workspace.as_str())
        .bind(event_id)
        .bind(status)
        .bind(serde_json::json!({ "message": message }).to_string())
        .bind(answer)
        .bind(at)
        .execute(&mut *connection)
        .await
        .expect("seeding a fleet event");
    }

    /// Records `label` as the failure `fleet`'s event `event_id` ended on.
    async fn label(&self, database: &Db, fleet: &str, event_id: &str, label: &str) {
        let mut connection = database.acquire().await.expect("a connection");
        sqlx::query(
            "UPDATE core.fleet_events SET failure_label = $3
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
        at: i64,
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
            SEED_MS + 1,
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
            SEED_MS + 2,
            ("fix the second one", "queued", None),
        )
        .await;

    let turns = threads.turns_at(database, &fleet, "e2", SEED_MS + 2).await;

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
            SEED_MS + 1,
            ("someone else's", status::PROCESSED, Some("theirs")),
        )
        .await;
    threads
        .event(
            database,
            &fleet,
            "e1",
            SEED_MS + 2,
            ("mine", status::PROCESSED, Some("ours")),
        )
        .await;

    let turns = threads.turns_at(database, &fleet, "e2", SEED_MS + 3).await;

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
        let at = SEED_MS + index;
        let event_id = format!("e{index}");
        threads
            .event(database, &fleet, &event_id, at, (&message, ended, answer))
            .await;
    }

    let turns = threads
        .turns_at(database, &fleet, "e13", SEED_MS + 13)
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
            .event(database, &fleet, event_id, SEED_MS + offset, row)
            .await;
    }

    let turns = threads.turns_at(database, &fleet, "e2", SEED_MS + 2).await;

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
            SEED_MS + 1,
            ("run the suite", status::FLEET_ERROR, None),
        )
        .await;
    threads.label(database, &fleet, "e1", LABEL).await;

    let turns = threads.turns_at(database, &fleet, "e2", SEED_MS + 2).await;

    let failed = format!("{ANSWER_FAILED}{LABEL}{ANSWER_FAILED_END}");
    assert_eq!(turns, [("run the suite".to_owned(), failed)]);
    fixtures.cleanup().await;
}
