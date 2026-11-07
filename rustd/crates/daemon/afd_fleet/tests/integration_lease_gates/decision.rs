//! Lease delivery and stored history retain one coherent reviewed selection.

use afd_admin::test_util::DecisionModelFixture;
use std::{borrow::Cow, time::Duration};

use afd_core::{clock, id::Uuid7};
use afd_db::Db;
use afd_runner::RunnerEdit;
use afd_wire::{
    decision::DecisionAssignment,
    lease::{LeaseResponse, TaggedLease},
};
use garde::Validate as _;
use sqlx::{Connection as _, types::Json};

use super::{
    READ_BOUND_CONFIG, Ready, claim, drive, ready, seed::seed_provider_resolution, set_config,
};
use crate::support::Fixtures;

/// Bound a real lock-order regression instead of hanging the test process.
const ISSUE_TIMEOUT: Duration = Duration::from_secs(5);

struct Issued {
    id: String,
    decision: Option<Box<DecisionAssignment>>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn issued_selection_survives_edits_revocation_and_catalogue_removal() {
    let fixtures = Fixtures::create_with_queue().await;
    let mut model = DecisionModelFixture::create(&fixtures.database)
        .await
        .expect("reviewed decision model fixture");
    let first = ready(&fixtures).await;
    let second = ready(&fixtures).await;
    let third = ready(&fixtures).await;
    for case in [&first, &second, &third] {
        select(&fixtures, &case.runner, Some(&model.model.id)).await;
    }
    let initial = issue(&fixtures, &first).await;
    let initial_profile = initial.decision.as_deref().expect("enabled selected model");
    initial_profile
        .validate()
        .expect("complete bounded assignment");
    assert_eq!(initial_profile.profile.model_id, model.model.id.as_str());
    assert_eq!(initial_profile.profile.model, model.model.model_name);
    assert_eq!(
        initial_profile.profile.model.len(),
        afd_wire::inference::MODEL_ID_MAX_BYTES
    );
    assert_eq!(initial_profile.profile.revision, "reviewed-1");
    assert_stored(&fixtures.database, &initial).await;

    let revised = revise_and_check(&fixtures, &mut model, &second, &initial).await;

    model
        .disable()
        .await
        .expect("decision model fixture lifecycle");
    let disabled = issue(&fixtures, &third).await;
    assert!(
        disabled.decision.is_none(),
        "ordinary chat does not invent a decision provider"
    );
    assert_stored(&fixtures.database, &disabled).await;
    for case in [&first, &second, &third] {
        select(&fixtures, &case.runner, None).await;
    }
    model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
    assert_stored(&fixtures.database, &initial).await;
    assert_stored(&fixtures.database, &revised).await;
    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn profile_snapshot_does_not_reverse_model_then_runner_lock_order() {
    let fixtures = Fixtures::create_with_queue().await;
    let model = DecisionModelFixture::create(&fixtures.database)
        .await
        .expect("reviewed decision model fixture");
    let case = ready(&fixtures).await;
    select(&fixtures, &case.runner, Some(&model.model.id)).await;
    let _default = prepare(&fixtures, &case).await;
    let acquired = claim(&fixtures, &case).await;
    let mut connection = fixtures.database.acquire().await.expect("model writer");
    let mut writer = connection
        .begin()
        .await
        .expect("model mutation transaction");
    sqlx::query("SELECT id FROM model.platform_models WHERE id=$1::uuid FOR UPDATE")
        .bind(model.model.id.as_str())
        .execute(&mut *writer)
        .await
        .expect("hold model row");
    let answer = tokio::time::timeout(ISSUE_TIMEOUT, drive(&fixtures, &case, acquired))
        .await
        .expect("lease issue never waits for the model row after locking runner");
    let issued = decode(&answer);
    writer.rollback().await.expect("release model writer");
    assert_eq!(
        issued
            .decision
            .as_deref()
            .expect("committed selection")
            .profile
            .model_id,
        model.model.id.as_str()
    );
    assert_stored(&fixtures.database, &issued).await;
    select(&fixtures, &case.runner, None).await;
    model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
    fixtures.cleanup().await;
}

pub(super) async fn select(fixtures: &Fixtures, runner: &Uuid7, model: Option<&Uuid7>) {
    let row = fixtures
        .runners()
        .self_record(runner)
        .await
        .expect("assigned policy");
    let mut policy = row.assignment.decode().expect("stored assignment");
    policy.decision_model_id = model.map(|id| Cow::Borrowed(id.as_str()));
    fixtures
        .runners()
        .edit(
            runner,
            RunnerEdit {
                assigned_policy: Some(&policy),
                labels: None,
            },
            clock::now(),
        )
        .await
        .expect("native reviewed assignment");
}

async fn prepare(fixtures: &Fixtures, case: &Ready) -> afd_db::test_util::PlatformDefault {
    set_config(fixtures, &case.fleet, READ_BOUND_CONFIG).await;
    fixtures
        .seed_wallet(&case.tenant, crate::report_seed::DEEP_POOL)
        .await;
    seed_provider_resolution(fixtures, &case.fleet).await
}

async fn issue(fixtures: &Fixtures, case: &Ready) -> Issued {
    let _default = prepare(fixtures, case).await;
    let acquired = claim(fixtures, case).await;
    let answer = drive(fixtures, case, acquired).await;
    decode(&answer)
}

fn decode(answer: &str) -> Issued {
    let response: LeaseResponse<'_> = serde_json::from_str(answer).expect("typed issued lease");
    response
        .lease
        .and_then(|lease| match lease {
            TaggedLease::Fleet(lease) => Some(Issued {
                id: lease.lease_id.into_owned(),
                decision: lease.decision,
            }),
            TaggedLease::Authoring(_) => None,
        })
        .expect("fleet work was issued")
}

async fn revise_and_check(
    fixtures: &Fixtures,
    model: &mut DecisionModelFixture,
    second: &Ready,
    initial: &Issued,
) -> Issued {
    let configuration = &mut model
        .model
        .details
        .decision
        .as_mut()
        .expect("decision")
        .configuration;
    configuration.profile.revision = "reviewed-2".into();
    configuration.profile.goal.statement_min_bps = 9600;
    configuration.profile.intent.intent.probability_bps = 9000;
    assert!(
        model
            .models
            .update(&model.model.id, model.model.details.clone(), clock::now())
            .await
            .expect("publish revised profile")
    );
    let revised = issue(fixtures, second).await;
    let revised_profile = revised.decision.as_deref().expect("revised selection");
    let initial_profile = initial.decision.as_deref().expect("first selection");
    assert_ne!(initial_profile.revision_id, revised_profile.revision_id);
    assert_eq!(revised_profile.profile.revision, "reviewed-2");
    assert_eq!(revised_profile.goal.statement_min_bps, 9600);
    assert_eq!(revised_profile.intent.intent.probability_bps, 9000);
    assert_stored(&fixtures.database, &revised).await;
    assert_stored(&fixtures.database, initial).await;
    revised
}

async fn assert_stored(database: &Db, issued: &Issued) {
    let mut connection = database.acquire().await.expect("read committed snapshot");
    let stored: Option<Json<DecisionAssignment>> =
        sqlx::query_scalar("SELECT decision_snapshot FROM runner.leases WHERE id=$1::uuid")
            .bind(&issued.id)
            .fetch_one(&mut *connection)
            .await
            .expect("lease still retained");
    assert_eq!(
        stored.as_ref().map(|value| &value.0),
        issued.decision.as_deref(),
        "wire and retained snapshot agree in every field"
    );
}
