//! Database-backed snapshots are sampled outside the SDK's collection lock.
use afd_admin::Models;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::producers::{models, scheduler};
use std::time::Duration;

const BUDGET: Duration = Duration::from_secs(2);

pub(super) async fn refresh(database: &Db) {
    let catalogue = Models::new(database.clone(), Entropy::new());
    let sampled = tokio::time::timeout(BUDGET, catalogue.bounded(models::MAX_MODELS)).await;
    let observed = sampled
        .ok()
        .and_then(Result::ok)
        .map(|(rows, clipped)| (rows.into_iter().filter_map(model_info).collect(), clipped));
    models::catalogue(observed);

    let schedules = afd_cron::Schedules::new(database.clone(), Entropy::new());
    let sampled = tokio::time::timeout(BUDGET, schedules.health(jiff::Timestamp::now())).await;
    scheduler::counts(sampled.ok().and_then(Result::ok));
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
