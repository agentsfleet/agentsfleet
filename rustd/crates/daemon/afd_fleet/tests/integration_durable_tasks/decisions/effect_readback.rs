//! Recovery listings of persisted uncertain intents never claim remote success.

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::durable_task::{DurableTaskEffectState, DurableTaskEffectsQuery};
use afd_db::test_util::mint_id;
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTaskRefusal};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::Decisions,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn recovery_pages_keep_uncertainty_and_respect_watermark_durable_task_and_fence() {
    exercise(readback).await;
}

async fn readback(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let durable_task_id = id(&task.durable_task_id);
    let first = seed(case, &durable_task_id, 1, "refused").await;
    let second = seed(case, &durable_task_id, 2, "unknown").await;
    let third = seed(case, &durable_task_id, 3, "issued").await;
    pages(&fixture, &task.durable_task_id, &first, &second, &third).await;
    refusals(&fixture, &task.durable_task_id).await;
    receipts(&fixture, &task.durable_task_id, &first, &second).await;
    fixture.model.finish().await.expect("fixture cleanup");
}

fn query(
    case: &Case,
    durable_task_id: &str,
    after: u64,
    cursor: Option<&str>,
) -> garde::Valid<DurableTaskEffectsQuery> {
    garde::Unvalidated::new(DurableTaskEffectsQuery {
        fencing_token: case.run().fencing_token,
        durable_task_id: durable_task_id.to_owned(),
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
) -> af_wire::durable_task::DurableTaskEffectsPage {
    fixture
        .tasks
        .effects(
            fixture.case.run(),
            &query(fixture.case, task, after, cursor),
        )
        .await
        .expect("recovery page")
}

async fn pages(fixture: &Decisions<'_>, task: &str, first: &Uuid7, second: &Uuid7, third: &Uuid7) {
    let page = recovery_page(fixture, task, 0, None).await;
    assert_eq!(page.effects.len(), 1);
    assert_eq!(
        page.effects.first().expect("one effect on page").effect_id,
        *first
    );
    assert_eq!(
        page.effects.first().expect("one effect on page").state,
        DurableTaskEffectState::Refused
    );
    assert_eq!(page.next_cursor.as_deref(), Some("1"));
    let page = recovery_page(fixture, task, 0, page.next_cursor.as_deref()).await;
    assert_eq!(
        page.effects.first().expect("one effect on page").effect_id,
        *second
    );
    assert_eq!(
        page.effects.first().expect("one effect on page").state,
        DurableTaskEffectState::Unknown
    );
    assert!(
        page.effects
            .first()
            .expect("one effect on page")
            .commit
            .is_none()
    );
    assert_eq!(page.next_cursor.as_deref(), Some("2"));
    // A stale pagination cursor must not reintroduce effects before a saved checkpoint watermark.
    let page = recovery_page(fixture, task, 2, Some("1")).await;
    assert_eq!(
        page.effects.first().expect("one effect on page").effect_id,
        *third
    );
    assert_eq!(
        page.effects.first().expect("one effect on page").state,
        DurableTaskEffectState::Issued
    );
    assert!(
        page.effects
            .first()
            .expect("one effect on page")
            .commit
            .is_none()
    );
    assert!(page.next_cursor.is_none());
    let page = recovery_page(fixture, task, 3, None).await;
    assert_eq!(page.effects.len(), 0);
    assert!(page.next_cursor.is_none());
}

async fn refusals(fixture: &Decisions<'_>, task: &str) {
    let case = fixture.case;
    for cursor in ["-1", "not-an-ordinal", "9223372036854775808"] {
        assert_eq!(
            fixture
                .tasks
                .effects(case.run(), &query(case, task, 0, Some(cursor)))
                .await
                .expect_err("malformed cursor")
                .code(),
            error_code::INVALID_REQUEST
        );
    }
    assert_eq!(
        fixture
            .tasks
            .effects(case.run(), &query(case, task, u64::MAX, None))
            .await
            .expect_err("overflowed watermark")
            .code(),
        error_code::INVALID_REQUEST
    );
    assert_eq!(
        fixture
            .tasks
            .effects(case.run(), &query(case, mint_id().as_str(), 0, None))
            .await
            .expect_err("foreign task")
            .code(),
        DurableTaskRefusal::NotFound.code()
    );
    let mut stale = case.run();
    stale.fencing_token += 1;
    assert_eq!(
        fixture
            .tasks
            .effects(stale, &query(case, task, 0, None))
            .await
            .expect_err("stale run fence")
            .code(),
        error_code::RUN_STALE_FENCING_TOKEN
    );
}

async fn receipts(fixture: &Decisions<'_>, task: &str, first: &Uuid7, second: &Uuid7) {
    let case = fixture.case;
    let refused = fixture
        .tasks
        .effect(case.run(), first)
        .await
        .expect("terminal refused receipt");
    assert_eq!(refused.state, DurableTaskEffectState::Refused);
    assert!(refused.commit.is_none());
    assert_eq!(
        fixture
            .tasks
            .effect(case.run(), &mint_id())
            .await
            .expect_err("missing receipt")
            .code(),
        DurableTaskRefusal::NotFound.code()
    );
    // No configured native Github dependency can turn unknown into confirmed on a read.
    assert_eq!(
        fixture
            .tasks
            .effect(case.run(), second)
            .await
            .expect_err("unconfigured GitHub cannot resolve unknown effect")
            .code(),
        DurableTaskRefusal::InvalidTarget.code()
    );
    let page = fixture
        .tasks
        .effects(case.run(), &query(case, task, 1, None))
        .await
        .expect("uncertainty retained");
    assert_eq!(
        page.effects.first().expect("one effect on page").state,
        DurableTaskEffectState::Unknown
    );
    assert!(
        page.effects
            .first()
            .expect("one effect on page")
            .commit
            .is_none()
    );
}

pub(super) async fn seed(case: &Case, task: &Uuid7, ordinal: i64, state: &str) -> Uuid7 {
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
    sqlx::query("UPDATE fleet.durable_tasks SET effect_cursor=GREATEST(effect_cursor,$2),used_publications=used_publications+CASE WHEN $3 IN ('issued','unknown') THEN 1 ELSE 0 END WHERE id=$1")
        .bind(task).bind(ordinal).bind(state).execute(&mut *transaction)
        .await.expect("retain allocation and any issued attempt");
    // The request, its unconfirmed state and its private recovery target, in one row.
    sqlx::query("INSERT INTO fleet.durable_task_effects (id,durable_task_id,fleet_id,workspace_id,source_item_id,original_run_id,original_runner_id,original_fence,durable_task_generation,stop_generation,ordinal,operation_id,covered_input_sequence,expected_head,tested_tree,request_digest,github_repository_id,github_installation_id,issuance_expires_at,status,target,created_at,updated_at) SELECT $1,id,fleet_id,workspace_id,originating_durable_task_item_id,$3,$4,$5,generation,stop_generation,$6,$7,0,$8,$9,$10,(target->>'repository_id')::bigint,(target->>'installation_id')::bigint,now()+interval '1 minute',$11,target,now(),now() FROM fleet.durable_tasks WHERE id=$2")
        .bind(&effect).bind(task).bind(case.run().run_id).bind(case.run().runner_id)
        .bind(i64::try_from(case.run().fencing_token).expect("fixture fence")).bind(ordinal).bind(mint_id())
        .bind("0".repeat(40)).bind("1".repeat(40)).bind("a".repeat(64)).bind(state)
        .execute(&mut *transaction).await.expect("preexisting publication intent");
    transaction
        .commit()
        .await
        .expect("complete persisted recovery fixture");
    effect
}
