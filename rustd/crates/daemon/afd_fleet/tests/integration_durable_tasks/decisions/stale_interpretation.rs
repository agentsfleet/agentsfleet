//! An interpretation binds only under the fleet revision it was made against.

use af_core::{error_code, id::Uuid7};
use af_wire::durable_task::{DurableTaskBindingsInput, DurableTaskInputAction};
use afd_fleet::durable_task::DurableTaskLifecycle as _;

use super::{
    super::fixture::{Case, exercise, id},
    existing_inputs::proposed,
    fixture::Decisions,
};

/// The task's stored status, generation and input cursor.
async fn stored(case: &Case, task: &Uuid7) -> (String, i64, i64) {
    sqlx::query_as("SELECT status,generation,input_sequence FROM fleet.durable_tasks WHERE id=$1")
        .bind(task)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("task observer"),
        )
        .await
        .expect("the task row")
}

fn valid(input: DurableTaskBindingsInput) -> garde::Valid<DurableTaskBindingsInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded interpretation")
}

// Value: protects=an interpretation made against a superseded fleet revision
// binds nothing; fails_when=the revision check is dropped and a steer read under
// old setup is accepted; why_new=the revision arm of the interpretation source
// had no case; seam=the production paid interpretation over live Postgres, the
// input re-stamped with another revision and re-digested so only the revision
// differs.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_interpretation_against_another_revision_is_refused_as_stale() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let durable_task_id = id(&task.durable_task_id);
        let source = case.assigned_source().await;
        let mut input = proposed(
            &fixture,
            &source,
            &task.durable_task_id,
            DurableTaskInputAction::Steer,
        )
        .await;
        input.accepted_revision_id = afd_db::test_util::mint_id().to_string();
        let digest = af_core::etag::sha256_hex(
            &serde_json::to_vec(&input.interpretation()).expect("complete interpretation"),
        );
        input.plan_digest = digest;
        let before = case.counts().await;

        let refused = fixture
            .tasks
            .interpret(case.run(), &valid(input))
            .await
            .expect_err("another revision's interpretation is stale");

        assert_eq!(refused.code(), error_code::DURABLE_TASK_STALE_STATE);
        assert_eq!(
            stored(case, &durable_task_id).await,
            ("running".to_owned(), 1, 0)
        );
        assert_eq!(case.counts().await, before, "no item or turn is written");
        fixture.model.finish().await.expect("model fixture cleanup");
    })
    .await;
}
