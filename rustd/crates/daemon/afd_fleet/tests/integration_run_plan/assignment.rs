//! An assignment finds a prior narrative by its fleet and logical identity.

use super::{Fixtures, relation_scans, run};
use crate::report_seed::held_in;
use af_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_fleet::run::sql::queue::ASSIGN_HEAD;
use sqlx::{Connection as _, PgConnection};

const RETAINED: &str = "WITH events AS (INSERT INTO thread.turns \
    (thread_id,workspace_id,id,actor,kind,status,request_json,opened_at,settled_at, \
    created_at,updated_at) \
    SELECT l.fleet_id,l.workspace_id,uuidv7(),'fixture','chat','processed','{}',now(),now(), \
    now(),now() \
    FROM thread.runs l,generate_series(1,4000) n WHERE l.id=$1 RETURNING id AS turn_id) \
    INSERT INTO thread.runs (id,runner_id,fleet_id,workspace_id,tenant_id,turn_id, \
    turn_created_at,posture,provider,model,last_metered_at,fencing_token,run_expires_at, \
    status,posted_message_count,created_at,updated_at) \
    SELECT uuidv7(),l.runner_id,l.fleet_id,l.workspace_id,l.tenant_id,e.turn_id,l.turn_created_at, \
    l.posture,l.provider,l.model,l.last_metered_at,0,l.run_expires_at,$2,0,l.created_at,now() \
    FROM thread.runs l CROSS JOIN events e WHERE l.id=$1";
const FRESH: &str = "WITH ended AS (UPDATE thread.turns SET settled_at=now() \
    WHERE thread_id=$1 AND settled_at IS NULL RETURNING thread_id,workspace_id) \
    INSERT INTO thread.turns (id,thread_id,workspace_id,producer,producer_key,payload_digest, \
    actor,kind,status,request_json,created_at,updated_at) \
    SELECT uuidv7(),thread_id,workspace_id,'steer','fresh-plan','fixture','fixture','chat', \
    'queued','{}',now(),now() FROM ended";

// Value: protects=indexed run-transition lookups; fails_when=identity lookup
// scans a backlog; why_new=small histories hide unbounded index keys; seam=none;
// no_claim=full assignment or discovery cost is constant in fleet population.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn prior_run_lookup_probes_the_run_event_index_for_fresh_and_reassigned_heads() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("plan owner");
    let mut tx = connection.begin().await.expect("plan fixture");
    super::meter::assert_point_lookups(&mut tx, &held).await;
    sqlx::query(RETAINED)
        .bind(&held.run_key)
        .bind(afd_state::sql::RUN_STATUS_REPORTED)
        .execute(&mut *tx)
        .await
        .expect("retained history");
    for table in ["thread.turns", "thread.runs"] {
        run(&mut tx, &format!("ANALYZE {table}")).await;
    }
    sqlx::query(
        "UPDATE thread.threads SET run_expires_at=now()-interval '1 second' WHERE fleet_id=$1",
    )
    .bind(&held.fleet)
    .execute(&mut *tx)
    .await
    .expect("expired thread");
    run(&mut tx, super::FORCE_GENERIC).await;
    for fresh in [false, true] {
        run(&mut tx, "SAVEPOINT assignment_probe").await;
        if fresh {
            sqlx::query(FRESH)
                .bind(&held.fleet)
                .execute(&mut *tx)
                .await
                .expect("fresh head");
        }
        let plan = assignment_plan(&mut tx, &held.fleet, &held.runner).await;
        // Runs carry the event they ran, so the prior-run lookup is one
        // index probe on `thread.runs` by `turn_id`. The assignment reads
        // `thread.turns` for its head and opens it in the same update.
        let prior = relation_scans(&plan, "runs");
        assert!(
            prior.iter().any(|scan| scan["Index Cond"]
                .as_str()
                .is_some_and(|condition| condition.contains("turn_id"))),
            "fresh={fresh}: {plan}"
        );
        run(&mut tx, "ROLLBACK TO SAVEPOINT assignment_probe").await;
    }
    tx.rollback().await.expect("discard plan fixture");
    drop(connection);
    owner.close().await;
    held.fixtures.cleanup().await;
}

async fn assignment_plan(
    connection: &mut PgConnection,
    fleet: &Uuid7,
    runner: &Uuid7,
) -> serde_json::Value {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, FORMAT JSON) {ASSIGN_HEAD}"
    )))
    .bind(fleet)
    .bind(runner)
    .bind(af_core::timing::RUN_TTL)
    .bind(af_core::timing::RUNNER_OFFLINE_AFTER)
    .bind(afd_state::sql::RUNNER_STATUS_ACTIVE)
    .bind(afd_state::sql::RUN_STATUS_ACTIVE)
    .bind(afd_state::sql::RUN_STATUS_EXPIRED)
    .bind(af_wire::task::CAPABILITY)
    .bind(af_wire::task::TaskState::Ready.as_str())
    .bind(af_wire::task::TaskState::Running.as_str())
    .bind(afd_fleet::task::DECISION_POLICY_PATH)
    .bind(afd_state::sql::FLEET_STATUS_ACTIVE)
    .bind(af_core::turn::status::RECEIVED)
    .bind(true)
    .bind(afd_credential::provider::PLATFORM_MODEL_REF)
    .fetch_one(connection)
    .await
    .expect("prepared production assignment plan")
}
