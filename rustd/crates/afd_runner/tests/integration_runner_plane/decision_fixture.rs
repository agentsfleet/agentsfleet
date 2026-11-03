//! Reviewed catalogue identities created through the native admin boundary.

use afd_admin::{CreateModel, DecisionModel, DeleteModel, Model, ModelInput, ModelUpdate, Models};
use afd_core::{
    catalogue::{Metadata, Price},
    clock,
};
use afd_crypto::entropy::Entropy;
use afd_db::{Db, test_util::mint_id};
use serde_json::json;

pub(super) struct DecisionModelFixture {
    pub(super) models: Models,
    pub(super) model: Model,
}

impl DecisionModelFixture {
    pub(super) async fn create(database: &Db) -> Self {
        let models = Models::new(database.clone(), Entropy::new());
        let input = ModelInput {
            provider: "typesafe".into(),
            model_name: format!("runner-decision-{}", mint_id()),
            details: details(),
        };
        let created = models
            .create(input, clock::now())
            .await
            .expect("reviewed model");
        let model = match created {
            CreateModel::Created(model) => Some(*model),
            CreateModel::Duplicate => None,
        };
        Self {
            models,
            model: model.expect("unique fixture catalogue identity"),
        }
    }

    pub(super) async fn disable(&mut self) {
        let mut replacement = self.model.details.clone();
        replacement
            .decision
            .as_mut()
            .expect("decision profile")
            .configuration
            .enabled = false;
        assert!(
            self.models
                .update(&self.model.id, replacement, clock::now())
                .await
                .expect("disable model")
        );
    }

    pub(super) async fn finish(self) {
        assert_eq!(
            self.models
                .delete(&self.model.id)
                .await
                .expect("delete unused model"),
            DeleteModel::Deleted
        );
    }
}

fn details() -> ModelUpdate {
    let now = clock::now();
    ModelUpdate {
        metadata: Metadata { display_name: "Runner decisions".into(), context_cap_tokens: 64_000,
            context_source_url: "https://example.test/limits".into(), context_verified_at: now, advanced: true },
        inference: None, prices: Vec::new(),
        decision: Some(DecisionModel {
            configuration: serde_json::from_value(json!({"enabled":true,"profile":{
                "revision":"reviewed-1","api":"typesafe","endpoint":"https://api.typesafe.ai/v1/systemone",
                "accepted_models":["evaluated-fixture-model"],"capabilities":{"noul":true,"choice":true,"score":true},
                "input_max_bytes":24576,
                "intent":{"questions_revision":"dt-routing-v1",
                    "intent":{"probability_bps":8500,"confidence_bps":7000},
                    "association":{"probability_bps":9500,"confidence_bps":8500},
                    "single_request_max_bps":3000,"multiple_request_min_bps":7000},
                "goal":{"questions_revision":"dt-goal-v1","statement_min_bps":9500,
                    "continue_action":{"probability_bps":8000,"confidence_bps":7000},
                    "wait":{"probability_bps":8500,"confidence_bps":7500},
                    "done":{"probability_bps":9500,"confidence_bps":8500}},
                "evaluation_digest":"ab".repeat(32),"evaluation_source_url":"https://example.test/evaluation",
                "evaluated_at":now.to_string()
            }})).expect("complete synthetic reviewed definition"),
            prices: vec![Price { effective_from: now, effective_until: None, prompt_tokens_above: 0,
                input_nanos_per_mtok: 42, cached_input_nanos_per_mtok: 42, cache_write_nanos_per_mtok: None,
                cache_write_5m_nanos_per_mtok: None, cache_write_1h_nanos_per_mtok: None,
                output_nanos_per_mtok: 42, source_url: "https://example.test/prices".into(),
                verified_at: now, conditions: "Synthetic fixture; no provider call".into() }],
        }),
    }
}
