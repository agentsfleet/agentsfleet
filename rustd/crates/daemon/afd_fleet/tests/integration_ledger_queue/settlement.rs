//! Failed and superseded turn settlement cannot charge or end a run.

use crate::{
    report_commit::{RESPONSE_ACCEPTED, assert_nothing_landed, report},
    report_seed::{Held, held_in},
    rows::Row,
    support::Fixtures,
};
use afd_db::config::DbRole;
use afd_fleet::run::Committed;

const FAIL_SETTLE: &str = "CREATE FUNCTION fleet.refuse_turn_settlement() RETURNS trigger \
    LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected settlement failure'; END $$; \
    CREATE TRIGGER refuse_turn_settlement BEFORE UPDATE OF settled_at ON thread.turns \
    FOR EACH ROW EXECUTE FUNCTION fleet.refuse_turn_settlement()";
const RESTORE_SETTLE: &str = "DROP TRIGGER refuse_turn_settlement ON thread.turns; \
    DROP FUNCTION fleet.refuse_turn_settlement()";

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn failed_turn_settlement_rolls_back_the_report_and_retry_settles_once() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    sqlx::raw_sql(FAIL_SETTLE)
        .execute(&mut *connection)
        .await
        .expect("inject fault");
    commit(&held)
        .await
        .expect_err("the settlement trigger refuses the report");
    assert_nothing_landed(&held, "turn settlement failed").await;
    assert!(
        !held.fixtures.turn_settled(&held.turn_id).await,
        "a rollback cannot settle the turn"
    );
    assert_eq!(
        held.fixtures.text(Row::Run(&held.run_key), "status").await,
        Some(afd_state::sql::RUN_STATUS_ACTIVE.to_owned())
    );
    sqlx::raw_sql(RESTORE_SETTLE)
        .execute(&mut *connection)
        .await
        .expect("remove fault");
    drop(connection);
    assert!(matches!(
        commit(&held).await.expect("retry"),
        Committed::Settled { .. }
    ));
    assert!(held.fixtures.turn_settled(&held.turn_id).await);
    assert!(matches!(
        commit(&held).await.expect("repeat"),
        Committed::AlreadySettled
    ));
    assert_eq!(held.fixtures.ledger_rows(&held.turn_id).await, 1);
    owner.close().await;
    held.fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_report_cannot_settle_a_turn_with_a_different_fence() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let mut connection = held.fixtures.database.acquire().await.expect("database");
    sqlx::query("UPDATE thread.turns SET run_fence = run_fence + 1 WHERE id = $1")
        .bind(&held.turn_id)
        .execute(&mut *connection)
        .await
        .expect("supersede turn");
    assert!(matches!(
        commit(&held).await.expect("fenced report"),
        Committed::Fenced { .. }
    ));
    assert_nothing_landed(&held, "the turn fence changed").await;
    assert!(
        !held.fixtures.turn_settled(&held.turn_id).await,
        "a superseded report cannot settle"
    );
    assert_eq!(
        held.fixtures.text(Row::Run(&held.run_key), "status").await,
        Some(afd_state::sql::RUN_STATUS_ACTIVE.to_owned())
    );
    sqlx::query("UPDATE thread.turns SET run_fence = $2 WHERE id = $1")
        .bind(&held.turn_id)
        .bind(held.fence.as_i64())
        .execute(&mut *connection)
        .await
        .expect("restore fixture fence");
    drop(connection);
    assert!(matches!(
        commit(&held).await.expect("retry"),
        Committed::Settled { .. }
    ));
    assert!(held.fixtures.turn_settled(&held.turn_id).await);
    held.fixtures.cleanup().await;
}

async fn commit(held: &Held) -> afd_fleet::Result<Committed> {
    let runs = held.fixtures.runs_with_dead_bus();
    let run = runs
        .load_for_report(&held.run_key, &held.runner)
        .await?
        .expect("issued run");
    runs.commit_report(report(
        &held.run_key,
        &held.runner,
        &run,
        (&held.run_key, &held.pricing),
        RESPONSE_ACCEPTED,
        held.now,
    ))
    .await
}
