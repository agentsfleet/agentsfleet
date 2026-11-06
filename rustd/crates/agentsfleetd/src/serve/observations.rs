//! Database-backed snapshots are sampled outside the SDK's collection lock.
use afd_admin::Models;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::producers::{models, scheduler};
use std::time::Duration;

const BUDGET: Duration = Duration::from_secs(2);

struct Snapshot {
    catalogue: Option<(Vec<models::ModelInfo>, bool)>,
    schedules: Option<[u64; 7]>,
}

pub(super) async fn refresh(database: &Db) {
    let snapshot = sample(database).await;
    models::catalogue(snapshot.catalogue);
    scheduler::counts(snapshot.schedules);
}

async fn sample(database: &Db) -> Snapshot {
    let catalogue = Models::new(database.clone(), Entropy::new());
    let sampled = tokio::time::timeout(BUDGET, catalogue.bounded(models::MAX_MODELS)).await;
    let catalogue = sampled
        .ok()
        .and_then(Result::ok)
        .map(|(rows, clipped)| (rows.into_iter().filter_map(model_info).collect(), clipped));
    let schedules = afd_cron::Schedules::new(database.clone(), Entropy::new());
    let sampled = tokio::time::timeout(BUDGET, schedules.health(jiff::Timestamp::now())).await;
    Snapshot {
        catalogue,
        schedules: sampled.ok().and_then(Result::ok),
    }
}

fn model_info(model: afd_admin::Model) -> Option<models::ModelInfo> {
    let context_tokens = u64::try_from(model.details.metadata.context_cap_tokens)
        .ok()
        .filter(|value| *value > 0)?;
    let route = model.details.inference.as_ref().and_then(|settings| {
        afd_inference::Configured::new(settings, u32::try_from(context_tokens).ok()?).ok()
    });
    Some(models::ModelInfo {
        tools: route
            .as_ref()
            .map(|route| route.supports_tools(&model.model_name)),
        image_transport: route
            .as_ref()
            .and(model.details.inference.as_ref())
            .map(|settings| settings.api.carries_images()),
        output_tokens: model
            .details
            .inference
            .as_ref()
            .map(|settings| u64::from(settings.max_output_tokens)),
        model: model.model_name,
        provider: model.provider,
        context_tokens,
    })
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "fixed valid catalogue fixtures")]
mod tests {
    use super::*;
    use afd_core::{catalogue::Metadata, id::Uuid7};
    use afd_wire::inference::{InferenceApi, ModelInference};

    fn model(context: i32, inference: Option<ModelInference>) -> afd_admin::Model {
        afd_admin::Model {
            id: Uuid7::parse("01900000-0000-7000-8000-000000000001")
                .expect("canonical fixture UUID"),
            provider: "openai".into(),
            model_name: "gpt-4o".into(),
            details: afd_admin::ModelUpdate {
                metadata: Metadata {
                    display_name: "Fixture".into(),
                    context_cap_tokens: context,
                    context_source_url: "https://example.test/context".into(),
                    context_verified_at: jiff::Timestamp::constant(1_700_000_000, 0),
                    advanced: false,
                },
                prices: Vec::new(),
                inference,
                decision: None,
            },
        }
    }

    #[test]
    fn catalogue_readings_keep_missing_and_invalid_route_metadata_unknown() {
        for context in [-1, 0] {
            assert!(model_info(model(context, None)).is_none());
        }
        let unconfigured = model_info(model(128_000, None)).expect("positive verified context");
        assert_eq!(unconfigured.context_tokens, 128_000);
        assert_eq!(unconfigured.output_tokens, None);
        assert_eq!(unconfigured.tools, None);
        assert_eq!(unconfigured.image_transport, None);
        let configured = ModelInference {
            api: InferenceApi::Chat,
            base_url: "https://example.test/v1".into(),
            dialect: None,
            max_output_tokens: 4096,
        };
        let reading = model_info(model(128_000, Some(configured))).expect("configured context");
        assert_eq!(reading.model, "gpt-4o");
        assert_eq!(reading.provider, "openai");
        assert_eq!(reading.output_tokens, Some(4096));
        assert!(reading.tools.is_some());
        assert!(reading.image_transport.is_some());
    }
}

#[cfg(all(test, feature = "test-util"))]
#[expect(clippy::expect_used, reason = "real catalogue fixture")]
mod sampling_tests {
    use super::*;
    use afd_db::config::DbRole;
    use afd_db::test_util::{ModelSeed, TestDatabase, mint_id};

    #[tokio::test]
    #[ignore = "needs live Postgres: make test-coverage-rustd"]
    async fn snapshots_read_the_catalogue_and_clear_on_database_failure() {
        let lane = TestDatabase::shared();
        let db = lane.open(DbRole::Api, &[]).await;
        let id = mint_id();
        let name = format!("observability-{id}");
        let mut connection = db.acquire().await.expect("fixture connection");
        ModelSeed::new(&id, "openai", &name, 128_000)
            .insert(&mut *connection)
            .await
            .expect("owned model");
        drop(connection);
        let snapshot = sample(&db).await;
        let (models, clipped) = snapshot.catalogue.expect("successful catalogue snapshot");
        assert!(!clipped);
        let row = models
            .iter()
            .find(|row| row.model == name)
            .expect("actual catalogue row");
        assert_eq!(row.context_tokens, 128_000);
        assert_eq!(row.output_tokens, Some(1024));
        assert!(snapshot.schedules.is_some());
        refresh(&db).await;
        let mut connection = db.acquire().await.expect("cleanup connection");
        ModelSeed::remove(&mut *connection, &id)
            .await
            .expect("owned cleanup");
        drop(connection);
        db.close().await;
        let failed = sample(&db).await;
        assert!(failed.catalogue.is_none());
        assert!(failed.schedules.is_none());
        refresh(&db).await;
        drop(db);
        lane.cleanup().await;
    }
}
