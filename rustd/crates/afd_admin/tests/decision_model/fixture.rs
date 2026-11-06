//! Complete synthetic catalogue definitions; no upstream model is contacted.

use afd_admin::{CreateModel, DecisionModel, DeleteModel, Model, ModelInput, ModelUpdate, Models};
use afd_core::{
    catalogue::{Metadata, Price},
    id::Uuid7,
};
use afd_crypto::entropy::Entropy;
use afd_db::{
    Db,
    config::DbRole,
    test_util::{TestDatabase, mint_id},
};
use afd_wire::decision::DecisionConfiguration;
use jiff::Timestamp;
use serde_json::json;

pub(super) const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

pub(super) struct Scenario {
    pub(super) database: Db,
    pub(super) models: Models,
    pub(super) model: Model,
}

impl Scenario {
    pub(super) async fn create(coding: Option<i64>) -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let models = Models::new(database.clone(), Entropy::new());
        let input = ModelInput {
            provider: "typesafe".into(),
            model_name: format!("decision-model-{}", mint_id()),
            details: details("reviewed-1", coding, 42),
        };
        let CreateModel::Created(model) = models
            .create(input, NOW)
            .await
            .expect("native model creation")
        else {
            panic!("unique model identity");
        };
        Self {
            database,
            models,
            model: *model,
        }
    }

    pub(super) async fn finish(self) {
        assert_eq!(
            self.models
                .delete(&self.model.id)
                .await
                .expect("remove model and its revisions"),
            DeleteModel::Deleted
        );
        self.database.close().await;
    }
}

pub(super) fn configuration(revision: &str) -> DecisionConfiguration {
    serde_json::from_value(json!({
        "enabled": true,
        "profile": {
            "revision": revision,
            "api": "typesafe",
            "endpoint": "https://api.typesafe.ai/v1/systemone",
            "accepted_models": ["evaluated-fixture-model"],
            "capabilities": {"noul":true,"choice":true,"score":true},
            "input_max_bytes": 24576,
            "intent": {
                "questions_revision":"dt-routing-v1",
                "intent":{"probability_bps":8500,"confidence_bps":7000},
                "association":{"probability_bps":9500,"confidence_bps":8500},
                "single_request_max_bps":3000,"multiple_request_min_bps":7000
            },
            "goal": {
                "questions_revision":"dt-goal-v1","statement_min_bps":9500,
                "continue_action":{"probability_bps":8000,"confidence_bps":7000},
                "wait":{"probability_bps":8500,"confidence_bps":7500},
                "done":{"probability_bps":9500,"confidence_bps":8500}
            },
            "evaluation_digest":"ab".repeat(32),
            "evaluation_source_url":"https://example.test/decision-evaluation",
            "evaluated_at":"2026-11-03T00:00:00.000Z"
        }
    }))
    .expect("complete synthetic reviewed configuration")
}

pub(super) fn price(rate: i64) -> Price {
    Price {
        effective_from: NOW,
        effective_until: None,
        prompt_tokens_above: 0,
        input_nanos_per_mtok: rate,
        cached_input_nanos_per_mtok: rate,
        cache_write_nanos_per_mtok: Some(rate),
        cache_write_5m_nanos_per_mtok: None,
        cache_write_1h_nanos_per_mtok: None,
        output_nanos_per_mtok: rate,
        source_url: "https://example.test/decision-pricing".into(),
        verified_at: NOW,
        conditions: "Synthetic rates; no provider request.".into(),
    }
}

pub(super) fn details(revision: &str, coding: Option<i64>, decision: i64) -> ModelUpdate {
    ModelUpdate {
        metadata: Metadata {
            display_name: "Decision profile persistence".into(),
            context_cap_tokens: 64_000,
            context_source_url: "https://example.test/model-limits".into(),
            context_verified_at: NOW,
            advanced: true,
        },
        prices: coding.into_iter().map(price).collect(),
        inference: None,
        decision: Some(DecisionModel {
            configuration: configuration(revision),
            prices: vec![price(decision)],
        }),
    }
}

pub(super) async fn read(models: &Models, id: &Uuid7) -> Model {
    models
        .list()
        .await
        .expect("read the production catalogue snapshot")
        .into_iter()
        .find(|model| model.id == *id)
        .expect("owned fixture model exists")
}
