//! One selection survives every runner projection and respects catalogue changes.

use std::{borrow::Cow, time::Duration};

use afd_admin::DeleteModel;
use afd_core::{clock, error_code, id::Uuid7};
use afd_db::test_util::mint_id;
use afd_runner::{KeysetCursor, NO_REPORT, RunnerEdit};
use afd_wire::runner::{AssignedPolicy, BindMode, ExtraBind};

use super::{Fixture, decision_fixture::DecisionModelFixture, enrolment};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn selection_round_trips_and_clearing_releases_the_catalogue_reference() {
    let mut fixture = Fixture::create().await;
    let model = DecisionModelFixture::create(&fixture.database).await;
    let mut request = enrolment();
    request.assigned_policy.decision_model_id = Some(Cow::Borrowed(model.model.id.as_str()));
    request.assigned_policy.extra_binds.push(ExtraBind {
        path: Cow::Borrowed("/srv/models"),
        mode: BindMode::ReadOnly,
        note: Cow::Borrowed("shared model cache"),
    });
    let enrolled = fixture
        .runners
        .register(&request, clock::now())
        .await
        .expect("enrol with decisions");
    let runner = enrolled.runner_id;
    fixture.enrolled.push(runner.clone());
    let before =
        assert_enrollment(&fixture, &runner, &model.model.id, &request.assigned_policy).await;
    assign(&fixture, &runner, &request.assigned_policy).await;
    assert_eq!(
        fixture.history(&runner).await.len(),
        before,
        "idempotent selection adds no event"
    );
    assert_eq!(
        model
            .models
            .delete(&model.model.id)
            .await
            .expect("referenced deletion is classified"),
        DeleteModel::InUse
    );
    request.assigned_policy.decision_model_id = None;
    assign(&fixture, &runner, &request.assigned_policy).await;
    assert_cleared(&fixture, &runner, before).await;
    model.finish().await;
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn unavailable_selections_cannot_partially_edit_a_runner() {
    let mut fixture = Fixture::create().await;
    let mut model = DecisionModelFixture::create(&fixture.database).await;
    let runner = fixture.enrol().await;
    let mut policy = enrolment().assigned_policy;
    policy.decision_model_id = Some(Cow::Owned(model.model.id.as_str().to_owned()));
    assign(&fixture, &runner, &policy).await;
    model.disable().await;
    policy.worker_count = 2;
    assign(&fixture, &runner, &policy).await;
    assert_readers(&fixture, &runner, Some(model.model.id.as_str())).await;
    let mut registration = enrolment();
    registration.assigned_policy = policy.clone();
    assert!(
        fixture
            .runners
            .register(&registration, clock::now())
            .await
            .expect_err("new disabled assignment refused")
            .is_rejected()
    );
    assert_refused_edit(&fixture, &runner, &mut policy).await;
    let detail = fixture
        .runners
        .runner_detail(&runner)
        .await
        .expect("unchanged runner");
    assert_eq!(detail.item().labels(), ["plane"]);
    assert_eq!(detail.item().assigned_policy().worker_count, 2);
    assert_eq!(
        detail.item().assigned_policy().decision_model_id.as_deref(),
        Some(model.model.id.as_str())
    );
    policy.decision_model_id = None;
    assign(&fixture, &runner, &policy).await;
    model.finish().await;
    fixture.cleanup().await;
}

async fn assign(fixture: &Fixture, runner: &Uuid7, policy: &AssignedPolicy<'_>) {
    let edited = fixture
        .runners
        .edit(
            runner,
            RunnerEdit {
                assigned_policy: Some(policy),
                labels: None,
            },
            clock::now(),
        )
        .await
        .expect("selection stored atomically");
    assert_eq!(edited.assigned_policy.as_ref(), Some(policy));
}

async fn assert_readers(fixture: &Fixture, runner: &Uuid7, selected: Option<&str>) {
    let stored = fixture
        .runners
        .self_record(runner)
        .await
        .expect("self projection");
    assert_eq!(
        stored
            .assignment
            .decode()
            .expect("self policy")
            .decision_model_id
            .as_deref(),
        selected
    );
    let beat = fixture
        .runners
        .heartbeat(runner, &NO_REPORT, None, clock::now())
        .await
        .expect("heartbeat projection");
    assert_eq!(
        beat.assignment
            .decode()
            .expect("heartbeat policy")
            .decision_model_id
            .as_deref(),
        selected
    );
    let detail = fixture
        .runners
        .runner_detail(runner)
        .await
        .expect("operator projection");
    assert_eq!(
        detail.item().assigned_policy().decision_model_id.as_deref(),
        selected
    );
    assert_pages(fixture, runner, selected, detail.item().created_at()).await;
}

async fn assert_pages(
    fixture: &Fixture,
    runner: &Uuid7,
    selected: Option<&str>,
    created_at: jiff::Timestamp,
) {
    let first = fixture
        .runners
        .list_runners(None, 100)
        .await
        .expect("first page");
    let item = first
        .items()
        .iter()
        .find(|item| item.id() == runner)
        .expect("owned runner in first page");
    assert_eq!(
        item.assigned_policy().decision_model_id.as_deref(),
        selected
    );
    let boundary = KeysetCursor::new(created_at + Duration::from_secs(1), runner.clone());
    let next = fixture
        .runners
        .list_runners(Some(&boundary), 100)
        .await
        .expect("cursor page");
    let item = next
        .items()
        .iter()
        .find(|item| item.id() == runner)
        .expect("owned runner after cursor");
    assert_eq!(
        item.assigned_policy().decision_model_id.as_deref(),
        selected
    );
}

async fn assert_refused_edit(fixture: &Fixture, runner: &Uuid7, policy: &mut AssignedPolicy<'_>) {
    let before = fixture.history(runner).await.len();
    policy.decision_model_id = Some(Cow::Owned(mint_id()));
    let labels = [Cow::Borrowed("must-not-land")];
    let refusal = fixture
        .runners
        .edit(
            runner,
            RunnerEdit {
                assigned_policy: Some(policy),
                labels: Some(&labels),
            },
            clock::now(),
        )
        .await
        .expect_err("unknown model refused");
    assert_eq!(refusal.code(), error_code::INVALID_REQUEST);
    assert_eq!(fixture.history(runner).await.len(), before);
}

async fn assert_enrollment(
    fixture: &Fixture,
    runner: &Uuid7,
    model: &Uuid7,
    policy: &AssignedPolicy<'_>,
) -> usize {
    let history = fixture.history(runner).await;
    assert_eq!(
        history
            .first()
            .expect("registered event")
            .metadata
            .get("decision_model_id"),
        Some(&serde_json::Value::from(model.as_str()))
    );
    assert_readers(fixture, runner, Some(model.as_str())).await;
    let before = fixture.history(runner).await.len();
    let stored = fixture
        .runners
        .self_record(runner)
        .await
        .expect("complete enrollment policy");
    assert_eq!(
        stored
            .assignment
            .decode()
            .expect("enrollment binds")
            .extra_binds,
        policy.extra_binds
    );
    before
}

async fn assert_cleared(fixture: &Fixture, runner: &Uuid7, before: usize) {
    assert_readers(fixture, runner, None).await;
    let cleared = fixture.history(runner).await;
    assert_eq!(cleared.len(), before + 1);
    assert!(
        cleared
            .first()
            .expect("clear event")
            .metadata
            .get("decision_model_id")
            .is_some_and(serde_json::Value::is_null)
    );
}
