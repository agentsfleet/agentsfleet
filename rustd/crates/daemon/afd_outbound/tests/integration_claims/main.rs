//! Outbound delivery claimed from Postgres (M244 §4), on a private database
//! per test: a claim reads every lane, so a shared schema's other rows would
//! become this test's work.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod poster;

use std::time::Duration;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_connector::Provider;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, Migrator};
use afd_outbound::obligation::{self, Delivery};
use afd_outbound::{Posters, Worker};
use tokio_util::sync::CancellationToken;

use self::poster::Recorder;

/// How long a test waits for the workers before it fails.
const PATIENCE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);
const DESTINATION: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// A migrated database of this test's own, and its fixture parents.
struct Ledger {
    lane: TestDatabase,
    database: Db,
    tenant: Uuid7,
}

/// One workspace and its fleet: one (slack, workspace) lane.
struct Lane {
    workspace: Uuid7,
    fleet: Uuid7,
}

impl Ledger {
    async fn create() -> Self {
        af_core::test_util::trace::install_subscriber();
        let lane = TestDatabase::create().await;
        let migrator = lane.open(DbRole::Migrator, &[]).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the private database migrates");
        let database = lane.open(DbRole::Api, &[]).await;
        let tenant = mint_id();
        sql(
            &database,
            "INSERT INTO identity.tenants (id, name, created_at, updated_at) \
             VALUES ($1, 'outbound-claims', now(), now())",
            &[&tenant],
        )
        .await;
        Self {
            lane,
            database,
            tenant,
        }
    }

    async fn lane(&self) -> Lane {
        let workspace = mint_id();
        let fleet = mint_id();
        sql(
            &self.database,
            "INSERT INTO identity.workspaces (id, tenant_id, name, created_at) \
             VALUES ($1, $2, $1::text, now())",
            &[&workspace, &self.tenant],
        )
        .await;
        sql(
            &self.database,
            "INSERT INTO fleet.fleets (id, workspace_id, tenant_id, name, status, required_tags, \
             memory_reads_workspace, memory_publishes_workspace, resource_version, created_at, updated_at) \
             VALUES ($1, $2, $3, 'outbound-claims', 'active', '{}'::text[], false, false, 1, now(), now())",
            &[&fleet, &workspace, &self.tenant],
        )
        .await;
        Lane { workspace, fleet }
    }

    /// Owes one answer on `lane`, as a committed report would; answers its event.
    async fn owe(&self, lane: &Lane, answer: &str) -> Uuid7 {
        let event = mint_id();
        let mut connection = self.database.acquire().await.expect("a connection");
        let written = obligation::owe(
            &mut connection,
            &mint_id(),
            mint_id().as_str(),
            Delivery {
                fleet_id: &lane.fleet,
                workspace_id: &lane.workspace,
                provider: Provider::Slack,
                destination: DESTINATION,
                event_id: &event,
                answer,
            },
        )
        .await
        .expect("the owe lands");
        assert!(written, "a fresh event owes a row");
        event
    }

    async fn delivered(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query_scalar("SELECT count(*) FROM fleet.obligations WHERE delivered_at IS NOT NULL")
            .fetch_one(&mut *connection)
            .await
            .expect("the count reads")
    }

    async fn until_delivered(&self, rows: i64) {
        tokio::time::timeout(PATIENCE, async {
            while self.delivered().await < rows {
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .expect("the workers deliver every owed row");
    }

    fn worker(&self, poster: &Recorder, pass: Duration) -> Worker<Recorder> {
        let (bus, _server) = afd_bus::test_util::detached().expect("a detached bus");
        Worker::new(
            self.database.clone(),
            bus,
            Posters {
                slack: poster.clone(),
            },
            Entropy::new(),
        )
        .with_claim_pass(pass)
    }

    async fn cleanup(self) {
        self.lane.cleanup().await;
    }
}

/// Runs one statement on the runtime role; fixtures only.
async fn sql(database: &Db, statement: &'static str, binds: &[&Uuid7]) {
    let mut connection = database.acquire().await.expect("a connection");
    let mut query = sqlx::query(statement);
    for bind in binds {
        query = query.bind(*bind);
    }
    query
        .execute(&mut *connection)
        .await
        .expect("the fixture statement runs");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_workers_keep_one_lane_in_order() {
    let ledger = Ledger::create().await;
    let (a, b) = (ledger.lane().await, ledger.lane().await);
    let mut expected_a = Vec::new();
    for n in 1..=10 {
        expected_a.push(ledger.owe(&a, &format!("a{n}")).await);
        ledger.owe(&b, &format!("b{n}")).await;
    }
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let first = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));
    let second = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));

    ledger.until_delivered(20).await;
    token.cancel();
    first.await.expect("the first worker stops");
    second.await.expect("the second worker stops");

    assert_eq!(
        poster.delivered_to(&a.workspace),
        expected_a,
        "lane A in order"
    );
    assert_eq!(
        poster.most_in_flight(&a.workspace),
        1,
        "never two rows held in lane A"
    );
    assert_eq!(
        poster.most_in_flight(&b.workspace),
        1,
        "never two rows held in lane B"
    );
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn failing_head_blocks_only_its_lane() {
    let ledger = Ledger::create().await;
    let (a, b) = (ledger.lane().await, ledger.lane().await);
    let head = ledger.owe(&a, "a1").await;
    let behind = ledger.owe(&a, "a2").await;
    for n in 1..=5 {
        ledger.owe(&b, &format!("b{n}")).await;
    }
    let poster = Recorder::failing(head.clone());
    let token = CancellationToken::new();
    let worker = tokio::spawn(ledger.worker(&poster, POLL).run(token.clone()));

    ledger.until_delivered(5).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(poster.delivered_to(&b.workspace).len(), 5, "lane B drained");
    assert!(
        !poster.attempted().contains(&behind),
        "A's second row is never claimed before its head"
    );
    assert!(poster.attempted().contains(&head), "A's head was tried");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn lapsed_claim_is_retaken_and_the_late_stamp_refused() -> Result<(), &'static str> {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    ledger.owe(&lane, "only").await;

    let first = obligation::claim(&ledger.database, 8, "holder-one")
        .await
        .expect("the first claim");
    let [first] = first.as_slice() else {
        return Err("one row is claimed");
    };
    sql(
        &ledger.database,
        "UPDATE fleet.obligations SET claim_expires_at = now() - interval '1 second' WHERE id = $1",
        &[&first.id],
    )
    .await;
    let second = obligation::claim(&ledger.database, 8, "holder-two")
        .await
        .expect("the retake");
    let [second] = second.as_slice() else {
        return Err("a lapsed claim is retaken");
    };
    assert_eq!(second.id, first.id);

    let stale_stamp = obligation::stamp_delivered(&ledger.database, first)
        .await
        .expect("the late stamp runs");
    assert!(!stale_stamp, "the first holder's late stamp updates 0 rows");
    let current = obligation::stamp_delivered(&ledger.database, second)
        .await
        .expect("the holder's stamp runs");
    assert!(current, "the current holder stamps");
    assert_eq!(ledger.delivered().await, 1, "one delivery in total");
    ledger.cleanup().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn lost_ping_is_caught_by_the_claim_pass() {
    let metrics = af_observability::test_util::Capture::install();
    let scans = || metrics.sum("agentsfleet_outbound_claims_total", &[("via", "scan")]);
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let poster = Recorder::default();
    let token = CancellationToken::new();
    let worker = tokio::spawn(
        ledger
            .worker(&poster, Duration::from_millis(200))
            .run(token.clone()),
    );
    // Let the worker's opening claim find nothing before the row exists.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let before = scans();

    let event = ledger.owe(&lane, "unpinged").await;
    ledger.until_delivered(1).await;
    token.cancel();
    worker.await.expect("the worker stops");

    assert_eq!(poster.delivered_to(&lane.workspace), vec![event]);
    // At least one: the capture is process-wide and sibling tests also scan,
    // but none of them pings, so any rise here is a pass.
    assert!(
        scans() - before >= 1,
        "the pass, not a ping, claimed the row"
    );
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn claim_reads_the_open_lane_index() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let seed = "INSERT INTO fleet.obligations (id, fleet_id, workspace_id, provider, destination, \
        event_id, answer, receipt, claim_expires_at, delivered_at, attempt_count, created_at, updated_at) \
        SELECT ('0195b4ba-8d3a-7a11-8abc-' || lpad(to_hex(n), 12, '0'))::uuid, $1, $2, 'slack', 'd', \
        ('0195b4ba-8d3a-7a12-8abc-' || lpad(to_hex(n), 12, '0'))::uuid, 'a', 't', now(), \
        CASE WHEN n <= 10000 THEN now() END, 0, now(), now() FROM generate_series(1, 10100) n";
    sql(&ledger.database, seed, &[&lane.fleet, &lane.workspace]).await;
    let mut connection = ledger.database.acquire().await.expect("a connection");
    sqlx::query("ANALYZE fleet.obligations")
        .execute(&mut *connection)
        .await
        .expect("statistics refresh");
    let mut transaction = sqlx::Transaction::begin(connection, None)
        .await
        .expect("a transaction to roll back");
    let explain = format!(
        "EXPLAIN (ANALYZE, BUFFERS) {}",
        afd_outbound::test_util::CLAIM_HEADS
    );
    let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(explain))
        .bind(8_i64)
        .bind("plan-holder")
        .bind(Duration::from_secs(30))
        .fetch_all(&mut *transaction)
        .await
        .expect("the claim plans");
    transaction
        .rollback()
        .await
        .expect("the plan's claims roll back");
    let plan = plan.join("\n");
    assert!(plan.contains("idx_obligations_open_lane"), "{plan}");
    ledger.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_empty_answer_owes_nothing_and_a_repeat_owes_once() {
    let ledger = Ledger::create().await;
    let lane = ledger.lane().await;
    let event = mint_id();
    let mut connection = ledger.database.acquire().await.expect("a connection");
    let delivery = |answer| Delivery {
        fleet_id: &lane.fleet,
        workspace_id: &lane.workspace,
        provider: Provider::Slack,
        destination: DESTINATION,
        event_id: &event,
        answer,
    };
    assert!(
        !obligation::owe(&mut connection, &mint_id(), "t", delivery(""))
            .await
            .expect("owe")
    );
    assert!(
        obligation::owe(&mut connection, &mint_id(), "t", delivery("x"))
            .await
            .expect("owe")
    );
    assert!(
        !obligation::owe(&mut connection, &mint_id(), "t", delivery("x"))
            .await
            .expect("owe")
    );
    drop(connection);
    ledger.cleanup().await;
}
