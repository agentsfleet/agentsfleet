//! Actual catalogue lock contention, without sleeps as ordering assertions.

use afd_admin::test_util::DecisionModelFixture;
use std::{borrow::Cow, time::Duration};

use afd_core::{clock, id::Uuid7};
use afd_db::{Db, test_util::mint_id};
use afd_runner::RunnerEdit;
use sqlx::{Acquire as _, PgConnection};

use super::{Fixture, enrolment};

#[derive(Clone, Copy)]
enum CatalogueChange {
    Disable,
    Revise,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn assignment_observes_committed_catalogue_changes_after_lock_contention() {
    for change in [CatalogueChange::Disable, CatalogueChange::Revise] {
        let mut fixture = Fixture::create().await;
        let model = DecisionModelFixture::create(&fixture.database)
            .await
            .expect("reviewed decision model fixture");
        let runner = fixture.enrol().await;
        let mut connection = fixture.owner.acquire().await.expect("catalogue writer");
        let mut transaction = connection.begin().await.expect("hold model lock");
        let blocker: i32 = sqlx::query_scalar(
            "SELECT pg_backend_pid() FROM model.platform_models WHERE id=$1::uuid FOR UPDATE",
        )
        .bind(model.model.id.as_str())
        .fetch_one(&mut *transaction)
        .await
        .expect("locked model");
        let waiting = start_assignment(&fixture, &runner, &model.model.id);
        wait_for_blocker(&fixture.owner, blocker).await;
        stage_change(&mut transaction, &model.model.id, change).await;
        transaction
            .commit()
            .await
            .expect("publish catalogue change");
        let result = tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .expect("bounded assignment wait")
            .expect("assignment task");
        assert_outcome(&fixture, &runner, &model.model.id, change, result).await;
        drop(connection);
        finish_case(fixture, model, runner).await;
    }
}

async fn finish_case(fixture: Fixture, model: DecisionModelFixture, runner: Uuid7) {
    let clear = enrolment().assigned_policy;
    fixture
        .runners
        .edit(
            &runner,
            RunnerEdit {
                assigned_policy: Some(&clear),
                labels: None,
            },
            clock::now(),
        )
        .await
        .expect("release selected catalogue identity");
    model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
    fixture.cleanup().await;
}

async fn stage_change(connection: &mut PgConnection, model: &Uuid7, change: CatalogueChange) {
    match change {
        CatalogueChange::Disable => {
            sqlx::query(
                "UPDATE model.platform_models SET decision_enabled=false WHERE id=$1::uuid",
            )
            .bind(model.as_str())
            .execute(connection)
            .await
            .expect("disable while assignment waits");
        }
        CatalogueChange::Revise => {
            let next = mint_id();
            sqlx::query("INSERT INTO model.decision_revisions (id,platform_model_id,revision,definition,created_at) SELECT $2::uuid,m.id,$3,jsonb_set(d.definition,'{revision}',to_jsonb($3::text)),now() FROM model.platform_models m JOIN model.decision_revisions d ON d.id=m.decision_revision_id WHERE m.id=$1::uuid")
                .bind(model.as_str()).bind(&next).bind("reviewed-2")
                .execute(&mut *connection).await.expect("new immutable revision in writer transaction");
            sqlx::query(
                "UPDATE model.platform_models SET decision_revision_id=$2::uuid WHERE id=$1::uuid",
            )
            .bind(model.as_str())
            .bind(&next)
            .execute(connection)
            .await
            .expect("select new reviewed revision");
        }
    }
}

async fn assert_outcome(
    fixture: &Fixture,
    runner: &Uuid7,
    model: &Uuid7,
    change: CatalogueChange,
    result: afd_runner::Result<afd_runner::RunnerEdited<'_>>,
) {
    let detail = fixture
        .runners
        .runner_detail(runner)
        .await
        .expect("committed runner state");
    match change {
        CatalogueChange::Disable => {
            assert!(
                result
                    .expect_err("fresh disabled model refused")
                    .is_rejected()
            );
            assert_eq!(fixture.history(runner).await.len(), 1);
            assert_eq!(detail.item().labels(), ["plane"]);
            assert!(detail.item().assigned_policy().decision_model_id.is_none());
        }
        CatalogueChange::Revise => {
            let stored =
                result.expect("enabled model remains assignable across revision publication");
            assert_eq!(
                stored
                    .assigned_policy
                    .as_ref()
                    .and_then(|policy| policy.decision_model_id.as_deref()),
                Some(model.as_str())
            );
            assert_eq!(fixture.history(runner).await.len(), 2);
            assert_eq!(detail.item().labels(), ["changed-with-policy"]);
        }
    }
}

fn start_assignment(
    fixture: &Fixture,
    runner: &Uuid7,
    model: &Uuid7,
) -> tokio_util::task::AbortOnDropHandle<afd_runner::Result<afd_runner::RunnerEdited<'static>>> {
    let runners = fixture.runners.clone();
    let target = runner.clone();
    let selected = model.clone();
    tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
        let mut policy = enrolment().assigned_policy;
        policy.decision_model_id = Some(Cow::Owned(selected.as_str().to_owned()));
        let labels = [Cow::Borrowed("changed-with-policy")];
        runners
            .edit(
                &target,
                RunnerEdit {
                    assigned_policy: Some(&policy),
                    labels: Some(&labels),
                },
                clock::now(),
            )
            .await
    }))
}

async fn wait_for_blocker(database: &Db, blocker: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut connection = database.acquire().await.expect("observe lock contention");
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity a WHERE $1=ANY(pg_blocking_pids(a.pid)))")
                .bind(blocker).fetch_one(&mut *connection).await.expect("observe actual waiter");
            if waiting { break; }
            tokio::task::yield_now().await;
        }
    }).await.expect("assignment reaches the locked catalogue row");
}
