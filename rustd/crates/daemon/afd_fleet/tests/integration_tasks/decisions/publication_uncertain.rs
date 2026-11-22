//! A publication that cannot be allocated writes nothing, and one whose ref
//! mutation got no answer is held as uncertain and blocks the next.

use af_core::{error_code, test_util::trace::Capture};
use af_wire::task::{TaskEffectInput, TaskEffectState};
use afd_db::test_util::mint_id;
use afd_fleet::task::{TaskLifecycle as _, Tasks};
use serde_json::{Value, json};
use sqlx::types::Json;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::Decisions,
    github_authority,
    github_server::Server,
    publication_refusal::prepared,
    publication_transport::preparation,
};

fn valid(input: TaskEffectInput) -> garde::Valid<TaskEffectInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("a bounded publication")
}

async fn refused(tasks: &Tasks, case: &Case, input: TaskEffectInput) -> error_code::ErrorCode {
    tasks
        .publish_effect(case.run(), &valid(input))
        .await
        .expect_err("the publication is not allocated")
        .code()
}

/// The task's bound target, as stored.
async fn target(case: &Case, task: &str) -> Value {
    sqlx::query_scalar("SELECT target FROM fleet.tasks WHERE id=$1")
        .bind(id(task))
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("observer"),
        )
        .await
        .expect("the task's target")
}

async fn set_target(case: &Case, task: &str, target: &Value) {
    sqlx::query("UPDATE fleet.tasks SET target=$2 WHERE id=$1")
        .bind(id(task))
        .bind(Json(target))
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the task's target is replaced");
}

/// The task's effect rows and its spent publications.
async fn spent(case: &Case, task: &str) -> (i64, i32) {
    sqlx::query_as("SELECT (SELECT count(*) FROM fleet.task_effects WHERE task_id=t.id), used_publications FROM fleet.tasks t WHERE id=$1")
        .bind(id(task))
        .fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("publication counters")
}

// Value: protects=a publication for another task, or for a target whose GitHub
// identifiers exceed what the ledger stores, is refused before any intent is
// written; fails_when=the current-task check or either identifier conversion
// stops refusing; why_new=these allocation refusals had no case; seam=the
// production publication verb over live Postgres, the target's stored
// identifiers raised past `BIGINT`; no_claim=remote preparation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_publication_that_cannot_be_allocated_writes_nothing() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let server = Server::start(vec![]).await;
        let tasks = github_authority::publisher(&fixture, &server).await;
        let input = prepared(case, task).await;
        let task_id = input.task.task_id.clone();
        let mut elsewhere = input.clone();
        elsewhere.task.task_id = mint_id().into();
        assert_eq!(
            refused(&tasks, case, elsewhere).await,
            error_code::TASK_NOT_FOUND
        );
        let stored = target(case, &task_id).await;
        for field in ["repository_id", "installation_id"] {
            let mut oversized = stored.clone();
            *oversized
                .get_mut(field)
                .expect("the target names its GitHub identifiers") = json!(u64::MAX);
            set_target(case, &task_id, &oversized).await;
            assert_eq!(
                refused(&tasks, case, input.clone()).await,
                error_code::TASK_INVALID_TARGET,
                "{field}"
            );
        }
        set_target(case, &task_id, &stored).await;
        assert_eq!(spent(case, &task_id).await, (0, 0), "no intent was written");
        fixture.model.finish().await.expect("fixture cleanup");
    })
    .await;
}

// Value: protects=a ref mutation whose answer was lost is recorded as uncertain,
// counted as spent, logged, and holds back the next publication; fails_when=the
// failed mutation is refused instead, its effect stays issued, or a second
// publication is allocated beside it; why_new=the uncertain publication had no
// case; seam=the production publication verb over live Postgres and a scripted
// GitHub origin whose mutation answers 502; no_claim=later reconciliation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_publication_whose_mutation_fails_is_uncertain_and_blocks_the_next() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let server = Server::start(vec![]).await;
        let tasks = github_authority::publisher(&fixture, &server).await;
        let input = prepared(case, task).await;
        let task_id = input.task.task_id.clone();
        preparation(&server);
        server.push(json!({"fixture_request":{"method":"POST","path":"/graphql","publication":true},
            "response":{"fixture_http_status":502,"fixture_http_body":{"message":"fixture unavailable"}}}));
        // The answer the uncertain effect is read back through: no commit yet.
        server.push(json!({"data":{"repository":{"databaseId":101,
            "ref":{"target":{"history":{"nodes":[]}}}}}}));
        let capture = Capture::install();
        let (receipt, replayed) = tasks
            .publish_effect(case.run(), &valid(input.clone()))
            .await
            .expect("an unanswered mutation is answered as uncertain");
        let event = capture.only("dt_effect_uncertain");
        drop(capture);
        assert!(!replayed);
        assert_eq!(receipt.state, TaskEffectState::Unknown);
        assert_eq!(receipt.commit, None);
        assert_eq!(event.field("effect_id"), Some(receipt.effect_id.as_str()));
        assert!(event.field("error_code").is_some(), "{event:?}");
        assert_eq!(spent(case, &task_id).await, (1, 1), "the mutation was spent");
        let mut next = input;
        next.operation_id = mint_id().into();
        assert_eq!(
            refused(&tasks, case, next).await,
            error_code::TASK_EFFECT_UNCERTAIN
        );
        assert_eq!(spent(case, &task_id).await, (1, 1), "nothing new allocated");
        server.finish().await;
        fixture.model.finish().await.expect("fixture cleanup");
    })
    .await;
}
