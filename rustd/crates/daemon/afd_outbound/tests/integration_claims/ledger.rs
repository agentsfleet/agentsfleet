//! The claims suite's fixture: a migrated database of a test's own, its
//! lanes, the rows a test reads back, and the waits it polls with.

use std::time::Duration;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_connector::Provider;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, Migrator};
use afd_outbound::obligation::{self, Delivery};
use afd_outbound::{Posters, Worker};

use super::poster::Recorder;

/// How long a test waits for the workers before it fails.
pub(super) const PATIENCE: Duration = Duration::from_secs(60);
pub(super) const POLL: Duration = Duration::from_millis(20);
pub(super) const DESTINATION: &str =
    r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// A migrated database of this test's own, and its fixture parents.
pub(super) struct Ledger {
    pub(super) lane: TestDatabase,
    pub(super) database: Db,
    pub(super) tenant: Uuid7,
}

/// One workspace and its fleet: one (slack, workspace) lane.
pub(super) struct Lane {
    pub(super) workspace: Uuid7,
    pub(super) fleet: Uuid7,
}

impl Ledger {
    pub(super) async fn create() -> Self {
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

    pub(super) async fn lane(&self) -> Lane {
        let workspace = mint_id();
        let fleet = mint_id();
        sql(
            &self.database,
            "INSERT INTO identity.workspaces (id, tenant_id, name, created_at) \
             VALUES ($1, $2, 'outbound-claims-' || substr(md5(random()::text), 1, 12), now())",
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
    pub(super) async fn owe(&self, lane: &Lane, answer: &str) -> Uuid7 {
        let event = mint_id();
        let mut connection = self.database.acquire().await.expect("a connection");
        let written = obligation::owe(
            &mut connection,
            &mint_id(),
            &mint_id(),
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

    pub(super) async fn delivered(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query_scalar("SELECT count(*) FROM fleet.obligations WHERE delivered_at IS NOT NULL")
            .fetch_one(&mut *connection)
            .await
            .expect("the count reads")
    }

    pub(super) async fn until_delivered(&self, rows: i64) {
        tokio::time::timeout(PATIENCE, async {
            while self.delivered().await < rows {
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .expect("the workers deliver every owed row");
    }

    /// One row's delivery state, by its event.
    pub(super) async fn row(&self, event: &Uuid7) -> Row {
        let mut connection = self.database.acquire().await.expect("a connection");
        sqlx::query_as(
            "SELECT delivered_at IS NOT NULL AS delivered, abandoned_at IS NOT NULL AS abandoned, \
             abandon_reason, attempt_count, receipt, \
             COALESCE(next_attempt_at > now() + interval '20 seconds', false) AS due_later, \
             extract(epoch FROM claim_expires_at)::float8 AS claim_expires \
             FROM fleet.obligations WHERE event_id = $1",
        )
        .bind(event)
        .fetch_one(&mut *connection)
        .await
        .expect("the row reads")
    }

    /// Polls `event`'s row until `done` holds.
    pub(super) async fn until(&self, event: &Uuid7, done: impl Fn(&Row) -> bool) -> Row {
        tokio::time::timeout(PATIENCE, async {
            loop {
                let row = self.row(event).await;
                if done(&row) {
                    return row;
                }
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .expect("the row reaches the state the test waits for")
    }

    /// Takes `event`'s row from its holder, as a retaking replica would.
    pub(super) async fn steal(&self, event: &Uuid7) {
        sql(
            &self.database,
            "UPDATE fleet.obligations SET receipt = $2 WHERE event_id = $1",
            &[event, &thief()],
        )
        .await;
    }

    pub(super) fn worker(&self, poster: &Recorder, pass: Duration) -> Worker<Recorder> {
        let (bus, _server) = afd_bus::test_util::detached().expect("a detached bus");
        self.worker_on(bus, poster, pass)
    }

    /// [`Ledger::worker`], pinged over `bus`.
    pub(super) fn worker_on(
        &self,
        bus: afd_bus::Handle,
        poster: &Recorder,
        pass: Duration,
    ) -> Worker<Recorder> {
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

    pub(super) async fn cleanup(self) {
        self.lane.cleanup().await;
    }
}

/// One row's delivery state, as a test reads it.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct Row {
    pub(super) delivered: bool,
    pub(super) abandoned: bool,
    pub(super) abandon_reason: Option<String>,
    pub(super) attempt_count: i64,
    pub(super) receipt: Uuid7,
    /// Released with the cycle backoff still ahead of it.
    pub(super) due_later: bool,
    pub(super) claim_expires: f64,
}

/// Polls until `done` holds.
pub(super) async fn until(done: impl Fn() -> bool) {
    tokio::time::timeout(PATIENCE, async {
        while !done() {
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .expect("the condition the test waits for holds");
}

/// Runs one statement on the runtime role; fixtures only.
pub(super) async fn sql(database: &Db, statement: &'static str, binds: &[&Uuid7]) {
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

/// The token a retaking replica holds after [`Ledger::steal`].
pub(super) fn thief() -> Uuid7 {
    Uuid7::parse("0195b4ba-8d3a-7000-8abc-0000000000ee").expect("a fixture token")
}
