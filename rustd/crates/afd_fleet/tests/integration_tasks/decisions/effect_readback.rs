//! Recovery listings of persisted uncertain intents never claim remote success.

use afd_db::test_util::mint_id;
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{TaskEffectState, TaskEffectsQuery};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::Decisions,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn recovery_pages_keep_uncertainty_and_respect_watermark_task_and_fence() {
    exercise(readback).await;
}

async fn readback(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let first = seed(case, &task.task_id, 1, "refused").await;
    let second = seed(case, &task.task_id, 2, "unknown").await;
    let third = seed(case, &task.task_id, 3, "issued").await;
    pages(&fixture, &task.task_id, &first, &second, &third).await;
    refusals(&fixture, &task.task_id).await;
    receipts(&fixture, &task.task_id, &first, &second).await;
    fixture.model.finish().await.expect("fixture cleanup");
}

fn query(
    case: &Case,
    task_id: &str,
    after: u64,
    cursor: Option<&str>,
) -> garde::Valid<TaskEffectsQuery> {
    garde::Unvalidated::new(TaskEffectsQuery {
        fencing_token: case.lease().fencing_token,
        task_id: task_id.to_owned(),
        ordinal_after: after,
        starting_after: cursor.map(str::to_owned),
        limit: 1,
    })
    .validate()
    .expect("bounded effect page")
}

async fn recovery_page(
    fixture: &Decisions<'_>,
    task: &str,
    after: u64,
    cursor: Option<&str>,
) -> afd_wire::task::TaskEffectsPage {
    fixture
        .tasks
        .effects(
            fixture.case.lease(),
            &query(fixture.case, task, after, cursor),
        )
        .await
        .expect("recovery page")
}

async fn pages(fixture: &Decisions<'_>, task: &str, first: &str, second: &str, third: &str) {
    let page = recovery_page(fixture, task, 0, None).await;
    assert_eq!(page.effects.len(), 1);
    assert_eq!(page.effects[0].effect_id, first);
    assert_eq!(page.effects[0].state, TaskEffectState::Refused);
    assert_eq!(page.next_cursor.as_deref(), Some("1"));
    let page = recovery_page(fixture, task, 0, page.next_cursor.as_deref()).await;
    assert_eq!(page.effects[0].effect_id, second);
    assert_eq!(page.effects[0].state, TaskEffectState::Unknown);
    assert!(page.effects[0].commit.is_none());
    assert_eq!(page.next_cursor.as_deref(), Some("2"));
    // A stale pagination cursor must not reintroduce effects before a saved checkpoint watermark.
    let page = recovery_page(fixture, task, 2, Some("1")).await;
    assert_eq!(page.effects[0].effect_id, third);
    assert_eq!(page.effects[0].state, TaskEffectState::Issued);
    assert!(page.effects[0].commit.is_none());
    assert!(page.next_cursor.is_none());
    let page = recovery_page(fixture, task, 3, None).await;
    assert!(page.effects.is_empty());
    assert!(page.next_cursor.is_none());
}

async fn refusals(fixture: &Decisions<'_>, task: &str) {
    let case = fixture.case;
    for cursor in ["-1", "not-an-ordinal", "9223372036854775808"] {
        assert!(
            fixture
                .tasks
                .effects(case.lease(), &query(case, task, 0, Some(cursor)))
                .await
                .is_err()
        );
    }
    assert!(
        fixture
            .tasks
            .effects(case.lease(), &query(case, task, u64::MAX, None))
            .await
            .is_err()
    );
    assert!(
        fixture
            .tasks
            .effects(case.lease(), &query(case, &mint_id(), 0, None))
            .await
            .is_err()
    );
    let mut stale = case.lease();
    stale.fencing_token += 1;
    assert!(
        fixture
            .tasks
            .effects(stale, &query(case, task, 0, None))
            .await
            .is_err()
    );
}

async fn receipts(fixture: &Decisions<'_>, task: &str, first: &str, second: &str) {
    let case = fixture.case;
    let refused = fixture
        .tasks
        .effect(case.lease(), &id(first))
        .await
        .expect("terminal refused receipt");
    assert_eq!(refused.state, TaskEffectState::Refused);
    assert!(refused.commit.is_none());
    assert!(
        fixture
            .tasks
            .effect(case.lease(), &id(&mint_id()))
            .await
            .is_err()
    );
    // No configured native Github dependency can turn unknown into confirmed on a read.
    assert!(
        fixture
            .tasks
            .effect(case.lease(), &id(second))
            .await
            .is_err()
    );
    let page = fixture
        .tasks
        .effects(case.lease(), &query(case, task, 1, None))
        .await
        .expect("uncertainty retained");
    assert_eq!(page.effects[0].state, TaskEffectState::Unknown);
    assert!(page.effects[0].commit.is_none());
}

pub(super) async fn seed(case: &Case, task: &str, ordinal: i64, state: &str) -> String {
    let effect = mint_id();
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("persisted unresolved effect fixture");
    let mut transaction = sqlx::Connection::begin(&mut *connection)
        .await
        .expect("atomic crash-recovery fixture");
    // A persisted intent owns an allocated ordinal. Unknown/issued fixtures also
    // retain the spent publication attempt, as after a lost remote acknowledgement.
    sqlx::query("UPDATE fleet.tasks SET effect_cursor=GREATEST(effect_cursor,$2),used_publications=used_publications+CASE WHEN $3 IN ('issued','unknown') THEN 1 ELSE 0 END WHERE id=$1::uuid")
        .bind(task).bind(ordinal).bind(state).execute(&mut *transaction)
        .await.expect("retain allocation and any issued attempt");
    sqlx::query("INSERT INTO fleet.task_effects (id,task_id,fleet_id,workspace_id,source_item_id,original_lease_id,original_runner_id,original_fence,task_generation,stop_generation,ordinal,operation_id,covered_input_sequence,expected_head,tested_tree,request_digest,repository_id,installation_id,issuance_expires_at,created_at) SELECT $1::uuid,id,fleet_id,workspace_id,originating_task_item_id,$3::uuid,$4::uuid,$5,generation,stop_generation,$6,$7::uuid,0,$8,$9,$10,(target->>'repository_id')::bigint,(target->>'installation_id')::bigint,now()+interval '1 minute',now() FROM fleet.tasks WHERE id=$2::uuid")
        .bind(&effect).bind(task).bind(case.lease().lease_id.as_str()).bind(case.lease().runner_id.as_str())
        .bind(i64::try_from(case.lease().fencing_token).expect("fixture fence")).bind(ordinal).bind(mint_id())
        .bind("0".repeat(40)).bind("1".repeat(40)).bind("a".repeat(64))
        .execute(&mut *transaction).await.expect("preexisting immutable publication intent");
    sqlx::query("INSERT INTO fleet.task_effect_states (task_effect_id,status,issued_at,created_at,updated_at) VALUES ($1::uuid,$2,CASE WHEN $2 IN ('issued','unknown') THEN now() END,now(),now())")
        .bind(&effect).bind(state).execute(&mut *transaction).await.expect("unconfirmed state");
    sqlx::query("INSERT INTO fleet.task_effect_payloads (task_effect_id,task_id,fleet_id,workspace_id,target,request,created_at) SELECT $1::uuid,id,fleet_id,workspace_id,target,'{}'::jsonb,now() FROM fleet.tasks WHERE id=$2::uuid")
        .bind(&effect).bind(task).execute(&mut *transaction).await.expect("private recovery target");
    transaction
        .commit()
        .await
        .expect("complete persisted recovery fixture");
    effect
}
