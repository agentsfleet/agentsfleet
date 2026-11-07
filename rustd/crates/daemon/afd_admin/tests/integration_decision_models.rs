//! Revisions, activation and the two price protocols retain one native meaning.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration fixture preconditions fail loudly"
)]

#[path = "decision_model/fixture.rs"]
mod fixture;
#[path = "decision_model/ownership.rs"]
mod ownership;

use afd_admin::{Model, ModelUpdate, Models};
use afd_billing::price::{PriceBook, Tier};
use afd_core::{error_code, id::Uuid7};

use fixture::{NOW, Scenario, details, price, read};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn catalogue_cannot_select_a_reviewed_revision_owned_by_another_model() {
    use sqlx::Acquire as _;
    let first = Scenario::create(None).await;
    let second = Scenario::create(None).await;
    let before = read(&first.models, &first.model.id).await;
    let mut connection = first
        .database
        .acquire()
        .await
        .expect("catalogue connection");
    let mut transaction = connection
        .begin()
        .await
        .expect("revision selection transaction");
    sqlx::query("UPDATE model.platform_models SET decision_revision_id=(SELECT decision_revision_id FROM model.platform_models WHERE id=$2::uuid) WHERE id=$1::uuid")
        .bind(first.model.id.as_str()).bind(second.model.id.as_str())
        .execute(&mut *transaction).await.expect("deferred ownership check waits for commit");
    let rejected = transaction
        .commit()
        .await
        .expect_err("cross-model revision cannot commit");
    assert!(afd_db::constraint::violates_foreign_key(
        &rejected,
        "fk_platform_models_decision_revision_owner"
    ));
    assert_eq!(
        read(&first.models, &first.model.id).await,
        before,
        "failed selection rolls back"
    );
    drop(connection);
    first.finish().await;
    second.finish().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn immutable_decision_revisions_keep_protocol_rates_and_atomic_rejections() {
    let scenario = Scenario::create(Some(5)).await;
    let models = &scenario.models;
    let first = read(models, &scenario.model.id).await;
    assert_eq!(first.details, details("reviewed-1", Some(5), 42));
    reject_changed_revision(models, &first).await;
    let second = adopt_revision(models, &first.id).await;
    reject_historical_revision(models, &second).await;
    assert_coding_schedule(&scenario).await;
    deactivate(models, second).await;
    scenario.finish().await;
}

async fn reject_changed_revision(models: &Models, first: &Model) {
    let mut conflict = details("reviewed-1", Some(99), 999);
    conflict
        .decision
        .as_mut()
        .expect("decision")
        .configuration
        .profile
        .goal
        .done
        .probability_bps = 9900;
    let refused = models
        .update(&first.id, conflict, NOW)
        .await
        .expect_err("one revision cannot change meaning");
    assert_eq!(refused.code(), error_code::INVALID_REQUEST);
    assert_eq!(
        &read(models, &first.id).await,
        first,
        "neither price protocol nor metadata changed"
    );
}

async fn adopt_revision(models: &Models, id: &Uuid7) -> Model {
    let mut second_details = details("reviewed-2", Some(7), 84);
    second_details
        .decision
        .as_mut()
        .expect("decision")
        .configuration
        .profile
        .goal
        .done
        .probability_bps = 9900;
    assert!(
        models
            .update(id, second_details.clone(), NOW)
            .await
            .expect("new reviewed revision")
    );
    let second = read(models, id).await;
    assert_eq!(second.details, second_details);
    second
}

async fn reject_historical_revision(models: &Models, second: &Model) {
    let mut resurrected = second.details.clone();
    resurrected
        .decision
        .as_mut()
        .expect("decision")
        .configuration
        .profile
        .revision = "reviewed-1".into();
    let refused = models
        .update(&second.id, resurrected, NOW)
        .await
        .expect_err("historical revisions remain immutable");
    assert_eq!(refused.code(), error_code::INVALID_REQUEST);
    assert_eq!(&read(models, &second.id).await, second);
}

async fn assert_coding_schedule(scenario: &Scenario) {
    let database = &scenario.database;
    let coding_id: String = sqlx::query_scalar(
        "SELECT id::text FROM model.platform_model_prices WHERE platform_model_id=$1::uuid",
    )
    .bind(scenario.model.id.as_str())
    .fetch_one(&mut *database.acquire().await.expect("rate connection"))
    .await
    .expect("coding rate identity");
    let book = PriceBook::load(
        &mut *database.acquire().await.expect("book connection"),
        "typesafe",
        &scenario.model.model_name,
    )
    .await
    .expect("production coding price book");
    assert_eq!(
        book,
        PriceBook::from_tiers(vec![Tier {
            id: Uuid7::parse(&coding_id).expect("native price id"),
            price: price(7)
        }]),
        "decision schedules never join coding billing"
    );
}

async fn deactivate(models: &Models, second: Model) {
    let mut second_details = second.details;
    second_details
        .decision
        .as_mut()
        .expect("decision")
        .configuration
        .enabled = false;
    assert!(
        models
            .update(&second.id, second_details.clone(), NOW)
            .await
            .expect("deactivation preserves definition")
    );
    assert_eq!(read(models, &second.id).await.details, second_details);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn decision_only_models_need_no_fabricated_coding_prices() {
    let scenario = Scenario::create(None).await;
    let models = &scenario.models;
    let created = &scenario.model;
    assert_eq!(
        read(models, &created.id).await.details,
        details("reviewed-1", None, 42)
    );
    let book = PriceBook::load(
        &mut *scenario.database.acquire().await.expect("book connection"),
        "typesafe",
        &created.model_name,
    )
    .await
    .expect("coding book stays unconfigured");
    assert_eq!(book, PriceBook::default());

    let expected = rename_without_replacing_decision(models, &created.id).await;
    reject_unusable_profiles(models, &created.id, &expected).await;
    scenario.finish().await;
}

async fn rename_without_replacing_decision(models: &Models, id: &Uuid7) -> ModelUpdate {
    let mut metadata_only = details("reviewed-1", None, 42);
    metadata_only.decision = None;
    metadata_only.metadata.display_name = "Renamed decision profile".into();
    assert!(
        models
            .update(id, metadata_only.clone(), NOW)
            .await
            .expect("preserve saved decision settings")
    );
    let mut expected = details("reviewed-1", None, 42);
    expected.metadata = metadata_only.metadata;
    assert_eq!(read(models, id).await.details, expected);
    expected
}

async fn reject_unusable_profiles(models: &Models, id: &Uuid7, expected: &afd_admin::ModelUpdate) {
    for fault in 0..5 {
        let mut replacement = details("reviewed-2", None, 84);
        let profile = &mut replacement
            .decision
            .as_mut()
            .expect("decision")
            .configuration
            .profile;
        match fault {
            0 => profile.capabilities.noul = false,
            1 => profile
                .accepted_models
                .first_mut()
                .expect("accepted model")
                .push('\0'),
            2 => profile.endpoint.push('\0'),
            3 => profile.evaluation_source_url.push('\0'),
            _ => profile.intent.single_request_max_bps = 7000,
        }
        let refused = models
            .update(id, replacement, NOW)
            .await
            .expect_err("invalid reviewed definition");
        assert_eq!(refused.code(), error_code::INVALID_REQUEST, "fault {fault}");
        assert_eq!(
            &read(models, id).await.details,
            expected,
            "fault {fault} changed the catalogue"
        );
    }
}
