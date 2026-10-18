//! Administrator writes commit identity, every price and one generation together.

use super::{Lane, NOW, details};
use afd_admin::CreateModel;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn failed_price_writes_roll_back_identity_and_generation_then_retry_heals() {
    let lane = Lane::create().await;
    let model = lane.file_model().await;
    let before = version(&lane).await;
    let mut invalid = details(true);
    invalid
        .prices
        .first_mut()
        .expect("a fixture price")
        .input_nanos_per_mtok = -1;
    lane.models
        .update(&model.id, invalid.clone())
        .await
        .expect_err("a negative rate is refused by the store");
    assert_eq!(lane.models.list().await.expect("list"), vec![model.clone()]);
    assert_eq!(version(&lane).await, before);
    let mut input = lane.input();
    input.model_name.push_str("-refused");
    input.details = invalid;
    lane.models
        .create(input, NOW)
        .await
        .expect_err("a negative rate is refused by the store");
    assert_eq!(lane.models.list().await.expect("list"), vec![model.clone()]);
    assert_eq!(version(&lane).await, before);
    assert!(
        lane.models
            .update(&model.id, details(true))
            .await
            .expect("retry")
    );
    assert_eq!(version(&lane).await, before + 1);
    assert_eq!(listed(&lane).await, details(true));
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_complete_replacement_prunes_tiers_and_keeps_the_retained_price_id() {
    let lane = Lane::create().await;
    let model = lane.file_model().await;
    let retained = price_ids(&lane).await;
    let mut tiered = details(true);
    let mut upper = tiered.prices.first().expect("a fixture price").clone();
    upper.prompt_tokens_above = 272_000;
    tiered.prices.push(upper);
    assert!(
        lane.models
            .update(&model.id, tiered.clone())
            .await
            .expect("add tier")
    );
    assert_eq!(listed(&lane).await, tiered);
    assert_eq!(price_ids(&lane).await.len(), 2);
    assert!(
        lane.models
            .update(&model.id, details(false))
            .await
            .expect("prune tier")
    );
    assert_eq!(price_ids(&lane).await, retained);
    assert_eq!(listed(&lane).await, details(false));
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn simultaneous_identical_creates_commit_one_model_and_one_generation() {
    let lane = Lane::create().await;
    let before = version(&lane).await;
    let (left, right) = tokio::join!(
        lane.models.create(lane.input(), NOW),
        lane.models.create(lane.input(), NOW)
    );
    let results = [left.expect("left"), right.expect("right")];
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, CreateModel::Created(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, CreateModel::Duplicate))
            .count(),
        1
    );
    assert_eq!(version(&lane).await, before + 1);
    assert_eq!(price_ids(&lane).await.len(), 1);
    lane.cleanup().await;
}

async fn version(lane: &Lane) -> i64 {
    let mut connection = lane.database.acquire().await.expect("connection");
    sqlx::query_scalar("SELECT resource_version FROM model.catalogue_version WHERE id = 1")
        .fetch_one(&mut *connection)
        .await
        .expect("generation")
}

async fn price_ids(lane: &Lane) -> Vec<String> {
    let mut connection = lane.database.acquire().await.expect("connection");
    sqlx::query_scalar(
        "SELECT id::text FROM model.platform_model_prices ORDER BY prompt_tokens_above",
    )
    .fetch_all(&mut *connection)
    .await
    .expect("price ids")
}

/// The one model the lane files, as the store lists it.
async fn listed(lane: &Lane) -> afd_admin::ModelUpdate {
    lane.models
        .list()
        .await
        .expect("list")
        .into_iter()
        .next()
        .expect("the filed model")
        .details
}
