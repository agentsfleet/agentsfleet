//! An admitted message costs one top-level statement, and a repeat costs one
//! and writes nothing (`M242_001` Dimensions 2.1 and 2.3).
//!
//! A private database, because `pg_stat_statements` counts per database and a
//! parallel suite's calls on the shared lane would land in the count.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, WorkspaceSeed, mint_id};
use afd_events::{ACTOR_MACHINE, Steer};

use crate::integration_list_plans::private_database;
use crate::integration_steer_retry::REQUEST_JSON;
use crate::support::EventsLane;

/// Top-level statements the API login ran in this database. The deployment
/// ceiling's own sample (`select_backlog.sql`, the only statement naming
/// `oldest_age`) runs on a timer, not per message, so it is left out.
const API_CALLS: &str = "SELECT COALESCE(sum(calls), 0)::bigint FROM pg_stat_statements \
    WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
      AND userid = (SELECT oid FROM pg_roles WHERE rolname = 'agentsfleet_api') \
      AND toplevel AND query NOT ILIKE '%oldest_age%'";

/// Transaction control the API login sent in this database.
const TRANSACTION_CONTROL: &str = "SELECT COALESCE(sum(calls), 0)::bigint FROM pg_stat_statements \
    WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
      AND userid = (SELECT oid FROM pg_roles WHERE rolname = 'agentsfleet_api') \
      AND query ~* '^\\s*(BEGIN|COMMIT|START TRANSACTION)'";

/// Sends counted after the warm-up.
const SENDS: i64 = 10;

/// One operation id, sent twice.
const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0c01";

/// A private database holding one active fleet with a current setup, its
/// owner pool, and a steer over the API login.
struct Private {
    lane: afd_db::test_util::TestDatabase,
    owner: afd_db::Db,
    runtime: afd_db::Db,
    steer: Steer,
    workspace: String,
    fleet: String,
}

impl Private {
    async fn open() -> Self {
        let (lane, owner, tenant) = private_database().await;
        let (workspace, fleet) = (mint_id(), mint_id());
        let mut connection = owner.acquire().await.expect("private owner");
        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .execute(&mut *connection)
            .await
            .expect("the statement counter");
        WorkspaceSeed::new(&workspace, &tenant)
            .insert(&mut *connection)
            .await
            .expect("a workspace");
        FleetSeed::new(&fleet, &workspace, &tenant, "active")
            .native_setup()
            .insert(&mut *connection)
            .await
            .expect("an active fleet with a current setup");
        drop(connection);
        let runtime = lane.open(DbRole::Api, &[]).await;
        let queue = afd_dragonfly::test_util::connect_live(&EventsLane::redis())
            .await
            .expect("the lane's Dragonfly");
        let steer = Steer::new(afd_admission::Admissions::for_tests(runtime.clone(), queue));
        Self {
            lane,
            owner,
            runtime,
            steer,
            workspace,
            fleet,
        }
    }

    async fn send(&self, operation: Option<&str>) -> String {
        self.steer
            .append(
                &self.fleet,
                &self.workspace,
                ACTOR_MACHINE,
                REQUEST_JSON,
                operation,
            )
            .await
            .expect("admitted")
            .event_id
    }

    async fn scalar(&self, statement: &'static str) -> i64 {
        sqlx::query_scalar(statement)
            .fetch_one(&mut *self.owner.acquire().await.expect("owner read"))
            .await
            .expect("an owner read")
    }

    async fn rows(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1::uuid")
            .bind(&self.fleet)
            .fetch_one(&mut *self.owner.acquire().await.expect("owner read"))
            .await
            .expect("the fleet's rows")
    }

    async fn close(self) {
        drop(self.steer);
        self.runtime.close().await;
        self.owner.close().await;
        self.lane.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_accepted_message_costs_one_statement() {
    let private = Private::open().await;
    private.send(None).await;
    let (calls, control) = (
        private.scalar(API_CALLS).await,
        private.scalar(TRANSACTION_CONTROL).await,
    );
    for _ in 0..SENDS {
        private.send(None).await;
    }
    assert_eq!(private.scalar(API_CALLS).await - calls, SENDS);
    assert_eq!(private.scalar(TRANSACTION_CONTROL).await, control);
    assert_eq!(control, 0, "no send opened a transaction");
    private.close().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_repeat_costs_one_statement_and_writes_nothing() {
    let private = Private::open().await;
    let first = private.send(Some(OPERATION)).await;
    let (calls, rows) = (private.scalar(API_CALLS).await, private.rows().await);
    let repeat = private.send(Some(OPERATION)).await;
    assert_eq!(repeat, first, "the repeat names the first event");
    assert_eq!(private.scalar(API_CALLS).await - calls, 1);
    assert_eq!(private.rows().await, rows);
    private.close().await;
}
