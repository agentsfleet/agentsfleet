//! Native dependency reads consume one exact-head wait without another model call.
use super::super::super::fixture::{Case, exercise, id};
use super::super::{
    fixture::Decisions, github_authority, github_server::Server, publication_refusal,
};
use super::{
    capture, settle,
    support::{Observation, counts, decision, observe},
};
use afd_fleet::task::{TaskLifecycle as _, Tasks};
use afd_wire::task::{DecisionTaskVersion, TaskState};
use serde_json::{Value, json};

mod guards;
mod races;

struct Waiting<'a> {
    fixture: Decisions<'a>,
    tasks: Tasks,
    task: DecisionTaskVersion,
    server: Server,
    checkpoint: String,
}

impl Waiting<'_> {
    async fn create(case: &Case) -> Waiting<'_> {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let server = Server::start(vec![]).await;
        let tasks = github_authority::tasks(&fixture, &server).await;
        let _target = publication_refusal::prepared(case, task.clone()).await;
        let (checkpoint, saved) = Box::pin(capture(&fixture, &task)).await;
        let report = decision(&fixture, task.clone(), &checkpoint).await;
        observe(&server, Observation::Pending);
        Box::pin(settle(
            &fixture,
            &tasks,
            report,
            TaskState::Waiting,
            &checkpoint,
            &saved,
        ))
        .await;
        Waiting {
            fixture,
            tasks,
            task,
            server,
            checkpoint,
        }
    }

    async fn due(&self) {
        let case = self.fixture.case;
        let updated = sqlx::query("UPDATE fleet.task_waits SET due_at=clock_timestamp()-interval '1 second',cooldown_expires_at=clock_timestamp()-interval '1 second' WHERE task_id=$1::uuid AND consumed_at IS NULL AND cancelled_at IS NULL")
            .bind(&self.task.task_id).execute(&mut *case.held.fixtures.database.acquire().await.expect("wait scheduler fixture"))
            .await.expect("due native dependency wait");
        assert_eq!(updated.rows_affected(), 1);
    }

    async fn finish(self) {
        self.server.finish().await;
        self.fixture
            .model
            .finish()
            .await
            .expect("paid decision fixture cleanup");
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn completed_dependencies_consume_one_native_wait_for_success_and_failure() {
    for observation in [Observation::Success, Observation::Failure] {
        exercise(async |case| {
            let waiting = Box::pin(Waiting::create(case)).await;
            waiting.due().await;
            observe(&waiting.server, observation);
            let result = waiting
                .tasks
                .maintain_waits()
                .await
                .expect("native status read and wake");
            assert_eq!((result.checked, result.woken, result.deferred), (1, 1, 0));
            let snapshot = continued(&waiting).await;
            let repeated = waiting
                .tasks
                .maintain_waits()
                .await
                .expect("consumed wait maintenance replay");
            assert_eq!(
                (repeated.checked, repeated.woken, repeated.deferred),
                (0, 0, 0)
            );
            assert_eq!(
                waiting
                    .tasks
                    .read(&case.scope, &id(&waiting.task.task_id))
                    .await
                    .expect("stable continuation"),
                snapshot
            );
            assert_eq!(counts(case, &waiting.task.task_id).await, (1, 3, 1, 2, 2));
            assert_eq!(waiting.fixture.counts().await, (1, 1, 1));
            waiting.finish().await;
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn unchanged_dependencies_defer_then_charge_only_the_bounded_fallback_read() {
    exercise(async |case| {
        let waiting = Box::pin(Waiting::create(case)).await;
        for expected_reads in [0, 1] {
            waiting.due().await;
            observe(&waiting.server, Observation::Pending);
            let result = waiting
                .tasks
                .maintain_waits()
                .await
                .expect("unchanged dependency observation");
            assert_eq!((result.checked, result.woken, result.deferred), (1, 0, 1));
            unchanged(&waiting, expected_reads).await;
            let cooldown = waiting
                .tasks
                .maintain_waits()
                .await
                .expect("native observation backoff");
            assert_eq!(
                (cooldown.checked, cooldown.woken, cooldown.deferred),
                (0, 0, 0)
            );
        }
        assert_eq!(waiting.fixture.counts().await, (1, 1, 1));
        waiting.finish().await;
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn closed_or_changed_head_dependencies_block_without_admitting_work() {
    for (observation, reason) in [
        (Observation::Closed, "pull_request_closed"),
        (Observation::ChangedHead, "pull_request_head_changed"),
    ] {
        exercise(async |case| {
            let waiting = Box::pin(Waiting::create(case)).await;
            waiting.due().await;
            observe(&waiting.server, observation);
            let result = waiting.tasks.maintain_waits().await.expect("refused remote dependency");
            assert_eq!((result.checked, result.woken, result.deferred), (1, 0, 1));
            let state: (String, String, i64, i64) = sqlx::query_as("SELECT status,status_reason,generation,stop_generation FROM fleet.tasks WHERE id=$1::uuid")
                .bind(&waiting.task.task_id).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("blocked owner observer"))
                .await.expect("current native task state");
            assert_eq!(state, (TaskState::Blocked.as_str().to_owned(), reason.to_owned(), 3, 2));
            assert_eq!(counts(case, &waiting.task.task_id).await, (0, 0, 0, 2, 1));
            assert_eq!(waiting.fixture.counts().await, (1, 1, 1));
            waiting.finish().await;
        }).await;
    }
}

async fn wait_state(waiting: &Waiting<'_>) -> (bool, bool, bool, bool, i32) {
    sqlx::query_as("SELECT consumed_at IS NOT NULL,cancelled_at IS NOT NULL,initial_read_at IS NOT NULL,claim_id IS NOT NULL,fallback_count FROM fleet.task_waits WHERE task_id=$1::uuid")
        .bind(&waiting.task.task_id).fetch_one(&mut *waiting.fixture.case.held.fixtures.database.acquire().await.expect("wait evidence observer"))
        .await.expect("durable observation custody")
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn failed_dependency_reads_retain_prior_evidence_and_observe_provider_cooldown() {
    for response in [
        json!({}),
        json!({"fixture_http_status":429,"fixture_retry_after":60,"fixture_http_body":{"message":"rate limited"}}),
    ] {
        exercise(async |case| {
            let waiting = Box::pin(Waiting::create(case)).await;
            let prior = baseline(&waiting).await;
            waiting.due().await;
            waiting.server.push(json!({"fixture_request":{"method":"GET","path":"/repos/example/linkwarden/pulls/101"},"response":response}));
            let result = waiting.tasks.maintain_waits().await.expect("remote failure is deferred");
            assert_eq!((result.checked, result.woken, result.deferred), (1, 0, 1));
            assert_eq!(baseline(&waiting).await, prior, "failed reads retain prior check facts and digest");
            assert_eq!(wait_state(&waiting).await, (false, false, true, false, 0));
            let snapshot = waiting.tasks.read(&case.scope, &id(&waiting.task.task_id)).await.expect("recoverable task");
            assert_eq!(snapshot.context.state, TaskState::Waiting);
            assert_eq!(snapshot.context.usage.fallback_reads, 0);
            assert_eq!(counts(case, &waiting.task.task_id).await, (0, 0, 1, 2, 1));
            let cooled: bool = sqlx::query_scalar("SELECT cooldown_expires_at > clock_timestamp() FROM fleet.task_waits WHERE task_id=$1::uuid")
                .bind(&waiting.task.task_id).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("cooldown observer"))
                .await.expect("native provider cooldown");
            assert_eq!(cooled, response.get("fixture_http_status").is_some());
            let retry = waiting.tasks.maintain_waits().await.expect("failure backoff preserves the wait");
            assert_eq!((retry.checked, retry.woken, retry.deferred), (0, 0, 0));
            assert_eq!(waiting.fixture.counts().await, (1, 1, 1));
            waiting.finish().await;
        }).await;
    }
}

async fn baseline(waiting: &Waiting<'_>) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('checks',checks,'baseline_digest',baseline_digest) FROM fleet.task_waits WHERE task_id=$1::uuid")
        .bind(&waiting.task.task_id).fetch_one(&mut *waiting.fixture.case.held.fixtures.database.acquire().await.expect("baseline observer"))
        .await.expect("committed native observation")
}

async fn continued(waiting: &Waiting<'_>) -> afd_fleet::task::TaskSnapshot {
    let case = waiting.fixture.case;
    let snapshot = waiting
        .tasks
        .read(&case.scope, &id(&waiting.task.task_id))
        .await
        .expect("continued task");
    assert_eq!(snapshot.context.state, TaskState::Ready);
    assert_eq!(
        (
            snapshot.context.generation,
            snapshot.context.stop_generation
        ),
        (3, 2)
    );
    assert_eq!(
        snapshot.context.checkpoint_id.as_deref(),
        Some(waiting.checkpoint.as_str())
    );
    assert_eq!(counts(case, &waiting.task.task_id).await, (1, 3, 1, 2, 2));
    assert_eq!(wait_state(waiting).await, (true, false, true, false, 0));
    snapshot
}

async fn unchanged(waiting: &Waiting<'_>, expected_reads: u32) {
    let case = waiting.fixture.case;
    let snapshot = waiting
        .tasks
        .read(&case.scope, &id(&waiting.task.task_id))
        .await
        .expect("still waiting");
    assert_eq!(snapshot.context.state, TaskState::Waiting);
    assert_eq!(
        (
            snapshot.context.generation,
            snapshot.context.stop_generation
        ),
        (2, 1)
    );
    assert_eq!(snapshot.context.usage.fallback_reads, expected_reads);
    assert_eq!(counts(case, &waiting.task.task_id).await, (0, 0, 1, 2, 1));
    assert_eq!(
        wait_state(waiting).await,
        (
            false,
            false,
            true,
            false,
            i32::try_from(expected_reads).expect("one fallback read")
        )
    );
}
