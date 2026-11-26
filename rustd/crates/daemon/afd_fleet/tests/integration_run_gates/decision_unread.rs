//! A selected decision profile this build cannot read refuses the issue as a
//! decision, never a fault that would end the runner's stream.

use af_core::test_util::trace::Capture;
use afd_admin::test_util::DecisionModelFixture;
use afd_db::config::DbRole;
use afd_fleet::run::{Billed, Refused};

use super::{assign, decision::select, ready};
use crate::{seed, support::Fixtures};

// Value: protects=an unreadable stored profile answers each issue with a typed
// refusal, logged once for its revision; fails_when=the decode fails the issue
// as a fault, or every refused issue logs again; seam=the issue's profile read
// over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unreadable_decision_profile_refuses_the_issue_without_a_fault() {
    let fixtures = Fixtures::create_isolated().await;
    let model = DecisionModelFixture::create(&fixtures.database)
        .await
        .expect("reviewed decision model fixture");
    let case = ready(&fixtures).await;
    select(&fixtures, &case.runner, Some(&model.model.id)).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    // Revisions are append-only; only the owner, in this test's own database,
    // writes one the build cannot read.
    let mut connection = owner.acquire().await.expect("an owner connection");
    let trigger = |toggle| {
        format!(
            "ALTER TABLE model.decision_revisions {toggle} TRIGGER trg_decision_revisions_refuse_update"
        )
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(trigger("DISABLE")))
        .execute(&mut *connection)
        .await
        .expect("the owner lifts the append-only guard");
    sqlx::query(
        "UPDATE model.decision_revisions SET definition = '{\"unread\": true}'::jsonb \
         WHERE platform_model_id = $1",
    )
    .bind(&model.model.id)
    .execute(&mut *connection)
    .await
    .expect("the stored definition stops reading");
    sqlx::raw_sql(sqlx::AssertSqlSafe(trigger("ENABLE")))
        .execute(&mut *connection)
        .await
        .expect("the owner restores the append-only guard");
    drop(connection);
    let runs = fixtures.runs();
    let billed = Billed {
        tenant_id: &case.tenant,
        posture: seed::POSTURE,
        provider: seed::PROVIDER,
        model: seed::MODEL,
    };

    let log = Capture::install();
    let mut answers = Vec::new();
    for _ in 0..2 {
        let acquired = assign(&fixtures, &case).await;
        let issued = runs
            .issue(&case.runner, &acquired, billed, fixtures.enrolled_at())
            .await
            .expect("an unreadable profile is a decision, not a fault");
        answers.push(issued.err());
        runs.release(&acquired.fleet_id, acquired.fence)
            .await
            .expect("the refused assignment lets go");
    }
    let line = log.only("runner_decision_profile_unreadable");
    drop(log);

    assert_eq!(
        answers,
        vec![Some(Refused::Profile), Some(Refused::Profile)]
    );
    assert_eq!(line.field("runner_id"), Some(case.runner.as_str()));
    owner.close().await;
    fixtures.cleanup().await;
}
