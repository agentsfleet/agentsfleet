//! Model settings and pricing remain one atomic revision under concurrent edits.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration fixtures fail loudly"
)]

use afd_admin::{CreateModel, DeleteModel, Model, ModelInput, ModelRates, Models};
use afd_core::{error_code, id::Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::{
    Db,
    config::DbRole,
    test_util::{TestDatabase, mint_id},
};
use afd_wire::inference::{InferenceApi, ModelInference};
use jiff::Timestamp;

/// The instant every model here is minted at.
const NOW: Timestamp = Timestamp::constant(1_725_000_000, 0);

fn settings(api: InferenceApi, limit: u32) -> ModelInference {
    ModelInference {
        api,
        base_url: "https://models.example.com/v1".into(),
        dialect: None,
        max_output_tokens: limit,
    }
}
const fn rates(context: i32, rate: i64) -> ModelRates {
    ModelRates {
        context_cap_tokens: context,
        input_nanos_per_mtok: rate,
        cached_input_nanos_per_mtok: 0,
        output_nanos_per_mtok: rate,
    }
}
async fn read(models: &Models, id: &Uuid7) -> Model {
    models
        .list()
        .await
        .expect("read settings")
        .into_iter()
        .find(|model| model.id == *id)
        .expect("owned model exists")
}
async fn stamp(db: &Db, id: &Uuid7) -> (i64, String) {
    sqlx::query_as(
        "SELECT context_cap_tokens::bigint, updated_at::text FROM model.platform_models WHERE id=$1::uuid",
    )
    .bind(id.as_str())
    .fetch_one(&mut *db.acquire().await.expect("owned connection"))
    .await
    .expect("saved row")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn saved_inference_round_trips_preserves_on_rates_only_updates_and_refuses_invalid_replacements()
 {
    let lane = TestDatabase::shared();
    let db = lane.open(DbRole::Api, &[]).await;
    let models = Models::new(db.clone(), Entropy::new());
    let model = create(&models, InferenceApi::Responses, 32768).await;
    assert_eq!(
        model.inference.as_ref(),
        Some(&settings(InferenceApi::Responses, 32768))
    );
    assert_eq!(read(&models, &model.id).await, model);
    assert!(
        models
            .update(&model.id, rates(128_000, 2), None)
            .await
            .expect("rates only")
    );
    assert_eq!(
        read(&models, &model.id).await.inference.as_ref(),
        model.inference.as_ref()
    );
    assert_invalid_updates(&models, &db, &model.id).await;
    replace_inference(&models, &model.id).await;
    assert_eq!(
        models.delete(&model.id).await.expect("owned cleanup"),
        DeleteModel::Deleted
    );
    assert!(
        !models
            .update(&model.id, rates(128_000, 3), None)
            .await
            .expect("missing row")
    );
    drop(models);
    drop(db);
    lane.cleanup().await;
}

async fn create(models: &Models, api: InferenceApi, output: u32) -> Model {
    let input = ModelInput {
        provider: format!("settings-{}", mint_id()),
        model_name: "owner/model-next".into(),
        rates: rates(128_000, 1),
        inference: Some(settings(api, output)),
    };
    let CreateModel::Created(model) = models
        .create(input, NOW)
        .await
        .expect("configured model creates")
    else {
        panic!("new identity")
    };
    model
}

async fn assert_invalid_updates(models: &Models, db: &Db, id: &Uuid7) {
    let before = read(models, id).await;
    let stamp_before = stamp(db, id).await;
    for invalid in [
        settings(InferenceApi::Chat, 128_001),
        ModelInference {
            base_url: "http://models.example".into(),
            ..settings(InferenceApi::Chat, 1)
        },
    ] {
        let error = models
            .update(id, rates(128_000, 999), Some(&invalid))
            .await
            .expect_err("invalid settings refuse");
        assert_eq!(error.code(), error_code::INVALID_REQUEST);
        assert_eq!(
            read(models, id).await,
            before,
            "refusal cannot partially change prices or configuration"
        );
        assert_eq!(stamp(db, id).await, stamp_before);
    }
}

async fn replace_inference(models: &Models, id: &Uuid7) {
    assert!(
        models
            .update(
                id,
                rates(128_000, 3),
                Some(&settings(InferenceApi::Messages, 65536)),
            )
            .await
            .expect("API and limit change")
    );
    assert_eq!(
        read(models, id).await.inference.as_ref(),
        Some(&settings(InferenceApi::Messages, 65536))
    );
    assert!(
        models.update(id, rates(1, 3), None).await.is_err(),
        "a context-only edit must validate saved output limits"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_rate_and_inference_updates_keep_a_valid_saved_pair() {
    let lane = TestDatabase::shared();
    let db = lane.open(DbRole::Api, &[]).await;
    let models = Models::new(db.clone(), Entropy::new());
    let model = create(&models, InferenceApi::Chat, 65536).await;
    let replacement = settings(InferenceApi::Responses, 32768);
    let (rate_update, setting_update) = tokio::join!(
        models.update(&model.id, rates(64_000, 2), None),
        models.update(&model.id, rates(128_000, 3), Some(&replacement)),
    );
    assert!(setting_update.expect("explicit configuration update"));
    let final_model = read(&models, &model.id).await;
    assert_eq!(final_model.inference.as_ref(), Some(&replacement));
    let expected_rate = if rate_update.is_ok() { 2 } else { 3 };
    assert_eq!(final_model.rates.input_nanos_per_mtok, expected_rate);
    assert!(
        final_model.rates.context_cap_tokens
            >= i32::try_from(replacement.max_output_tokens).expect("fixture limit")
    );
    assert_eq!(
        models.delete(&model.id).await.expect("owned cleanup"),
        DeleteModel::Deleted
    );
    drop(models);
    drop(db);
    lane.cleanup().await;
}
