//! A replacement run abandons only an unissued effect whose original owner is fenced.
use super::{
    super::fixture::{Case, exercise, id},
    effect_readback::seed,
    fixture::Decisions,
};
use af_core::id::Uuid7;
use af_wire::task::TaskEffectState;
use afd_fleet::task::TaskLifecycle as _;
use serde_json::Value;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reassigned_owner_refuses_prepared_effect_without_spending_publication() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let task_id = id(&task.task_id);
        let effect = seed(case, &task_id, 1, "prepared").await;
        let before = state(case, &effect).await;
        let prepared = fixture
            .tasks
            .effect(case.run(), &effect)
            .await
            .expect("live original issuer retains preparation");
        assert_eq!(prepared.state, TaskEffectState::Prepared);
        assert_eq!(prepared.effect_id, effect);
        assert!(prepared.commit.is_none());
        assert_eq!(state(case, &effect).await, before);
        reassign_and_abandon(&fixture, &effect).await;
        assert_unspent(case, &task_id).await;
        assert_eq!(fixture.counts().await, (0, 0, 0));
        fixture.model.finish().await.expect("model fixture cleanup");
    })
    .await;
}

#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn reassign_and_abandon(fixture: &Decisions<'_>, effect: &Uuid7) {
    let case = fixture.case;
    let original = intent(case, effect).await;
    let default = super::run_recovery::support::provider(fixture).await;
    super::run_recovery::support::model(fixture, Some(&fixture.model.model.id)).await;
    let (run_id, fence) = replacement(case).await;
    let current = afd_fleet::task::TaskRun {
        runner_id: &case.held.runner,
        run_id: &run_id,
        fencing_token: fence,
    };
    let refused = fixture
        .tasks
        .effect(current, effect)
        .await
        .expect("fenced original preparation is safely abandoned");
    assert_eq!(refused.state, TaskEffectState::Refused);
    assert_eq!(refused.effect_id, *effect);
    assert_eq!(refused.ordinal, 1);
    assert!(refused.commit.is_none());
    assert_replay(fixture, current, &refused).await;
    assert_eq!(intent(case, effect).await, original);
    super::run_recovery::support::model(fixture, None).await;
    drop(default);
}

async fn intent(case: &Case, effect: &Uuid7) -> Value {
    let row: (Value, bool) = sqlx::query_as("SELECT to_jsonb(e),issuance_expires_at>clock_timestamp() FROM fleet.task_effects e WHERE id=$1")
        .bind(effect).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("immutable intent observer"))
        .await.expect("original future issuance capability");
    assert!(
        row.1,
        "abandonment must prove original-owner fencing before capability expiry"
    );
    row.0
}

async fn state(case: &Case, effect: &Uuid7) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(s) FROM fleet.task_effect_states s WHERE task_effect_id=$1")
        .bind(effect)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("intent state observer"),
        )
        .await
        .expect("retained effect state")
}

async fn assert_unspent(case: &Case, task: &Uuid7) {
    let row: (i32, i64, i64, bool) = sqlx::query_as("SELECT used_publications,effect_cursor,(SELECT count(*) FROM fleet.task_effects WHERE task_id=$1),(SELECT issued_at IS NULL FROM fleet.task_effect_states s JOIN fleet.task_effects e ON e.id=s.task_effect_id WHERE e.task_id=$1) FROM fleet.tasks WHERE id=$1")
        .bind(task).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("publication accounting observer"))
        .await.expect("unissued original allocation");
    assert_eq!(row, (0, 1, 1, true));
    assert_eq!(case.counts().await, (1, 1, 1));
}

async fn assert_replay(
    fixture: &Decisions<'_>,
    current: afd_fleet::task::TaskRun<'_>,
    refused: &af_wire::task::TaskEffect,
) {
    let case = fixture.case;
    let effect = id(&refused.effect_id);
    let terminal = state(case, &effect).await;
    let replay = fixture
        .tasks
        .effect(current, &effect)
        .await
        .expect("terminal abandonment replay");
    assert_eq!(replay.state, TaskEffectState::Refused);
    assert_eq!(replay.effect_id, refused.effect_id);
    assert!(replay.commit.is_none());
    assert_eq!(state(case, &effect).await, terminal);
}

#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn replacement(case: &Case) -> (Uuid7, u64) {
    super::run_recovery::support::expire(case).await;
    let acquired = super::run_recovery::support::assign(case).await;
    let run = super::run_recovery::reassigned(case, acquired).await;
    let run_id = id(run
        .get("run_id")
        .and_then(Value::as_str)
        .expect("replacement run identifier"));
    let fence = run
        .get("fencing_token")
        .and_then(Value::as_u64)
        .expect("native replacement fence");
    assert_ne!(run_id, case.held.run_key);
    assert!(fence > case.run().fencing_token);
    (run_id, fence)
}
