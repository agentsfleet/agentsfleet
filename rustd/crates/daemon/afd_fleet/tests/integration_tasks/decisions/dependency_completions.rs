//! Signed completion identity can signal a persisted wait but cannot admit work.
use super::{
    super::fixture::{Case, exercise, id},
    dependency_waits::{seed_wait, wait_snapshot},
    fixture::Decisions,
};
use af_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_fleet::task::{CompletionScope, TaskLifecycle as _, VerifiedCompletion};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn completions_preserve_identity_attempt_order_and_cooldown_without_admission() {
    exercise(completions).await;
}

async fn completions(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let task_id = id(&task.task_id);
    seed_wait(case, &task_id, true).await;
    let before = case.counts().await;
    let completion = completion(case);
    mismatches(&fixture, &completion).await;
    fanout(&fixture, &task_id, &completion).await;
    assert_eq!(case.counts().await, before);
    fixture.model.finish().await.expect("fixture cleanup");
}

fn completion(case: &Case) -> VerifiedCompletion {
    VerifiedCompletion {
        scope: CompletionScope::Fleet(case.scope.clone()),
        installation_id: 1,
        repository_id: 101,
        head: "0".repeat(40),
        workflow_id: 42,
        run_id: 100,
        run_number: 10,
        run_attempt: 2,
        conclusion: "success".to_owned(),
        body_digest: "a".repeat(64),
    }
}

async fn mismatches(fixture: &Decisions<'_>, completion: &VerifiedCompletion) {
    let case = fixture.case;
    for mismatch in ["invalid", "head", "repository", "installation"] {
        let mut input = completion.clone();
        match mismatch {
            "invalid" => input.run_attempt = 0,
            "head" => input.head = "f".repeat(40),
            "repository" => input.repository_id = 102,
            _ => input.installation_id = 2,
        }
        assert_eq!(
            fixture
                .tasks
                .record_github_completion(&input)
                .await
                .expect("bounded receipt"),
            0
        );
    }
    let mut foreign = completion.clone();
    foreign.scope = CompletionScope::Workspace(mint_id());
    assert_eq!(
        fixture
            .tasks
            .record_github_completion(&foreign)
            .await
            .expect("foreign workspace ignored"),
        0
    );
    let mut other_fleet = case.scope.clone();
    other_fleet.fleet_id = mint_id();
    foreign.scope = CompletionScope::Fleet(other_fleet);
    assert_eq!(
        fixture
            .tasks
            .record_github_completion(&foreign)
            .await
            .expect("foreign fleet ignored"),
        0
    );
}

async fn fanout(fixture: &Decisions<'_>, task: &Uuid7, completion: &VerifiedCompletion) {
    let case = fixture.case;
    // Earlier attempts and unrelated workflows may be retained, but cannot signal this wait.
    let mut older = completion.clone();
    older.run_attempt = 1;
    let mut unrelated = completion.clone();
    unrelated.workflow_id = 43;
    unrelated.run_id = 101;
    for input in [&older, &unrelated] {
        assert_eq!(
            fixture
                .tasks
                .record_github_completion(input)
                .await
                .expect("retained identity"),
            1
        );
    }
    fixture
        .tasks
        .maintain_waits()
        .await
        .expect("bounded fanout");
    assert_eq!(
        *wait_snapshot(case, task)
            .await
            .get("signal_sequence")
            .expect("signal sequence"),
        0
    );
    assert_eq!(
        fixture
            .tasks
            .record_github_completion(completion)
            .await
            .expect("current completion"),
        1
    );
    let mut replay = completion.clone();
    replay.body_digest = "b".repeat(64);
    "failure".clone_into(&mut replay.conclusion);
    assert_eq!(
        fixture
            .tasks
            .record_github_completion(&replay)
            .await
            .expect("same native receipt identity"),
        0
    );
    cooldown(fixture, task).await;
}

async fn cooldown(fixture: &Decisions<'_>, task: &Uuid7) {
    let case = fixture.case;
    let result = fixture
        .tasks
        .maintain_waits()
        .await
        .expect("signal without bypassing cooldown");
    assert_eq!((result.checked, result.woken, result.deferred), (0, 0, 0));
    let wait = wait_snapshot(case, task).await;
    assert_eq!(
        (*wait
            .pointer("/signal_sequence")
            .expect("fixture signal_sequence")),
        1
    );
    assert_eq!(
        (*wait
            .pointer("/observed_signal_sequence")
            .expect("fixture observed_signal_sequence")),
        0
    );
    assert!(
        wait.pointer("/claim_id")
            .expect("fixture claim_id")
            .is_null()
    );
    assert!(
        wait.pointer("/consumed_admission_id")
            .expect("fixture consumed_admission_id")
            .is_null()
    );
    let cooling: bool = sqlx::query_scalar("SELECT due_at >= cooldown_expires_at AND due_at > now() FROM fleet.task_waits WHERE task_id=$1")
        .bind(task).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("observer"))
        .await.expect("cooldown retained");
    assert!(cooling);
    fixture
        .tasks
        .maintain_waits()
        .await
        .expect("idempotent fanout");
    assert_eq!(
        *wait_snapshot(case, task)
            .await
            .get("signal_sequence")
            .expect("signal sequence"),
        1
    );
}
