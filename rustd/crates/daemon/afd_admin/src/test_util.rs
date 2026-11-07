//! Reviewed catalogue identities created through the native admin boundary.

use crate::{
    CreateModel, DecisionModel, DeleteModel, Model, ModelInput, ModelUpdate, Models, Result,
};
use af_core::{
    catalogue::{Metadata, Price},
    clock,
};
use af_crypto::entropy::Entropy;
use afd_db::{Db, test_util::mint_id};
use serde_json::json;

/// The production catalogue statement, exposed for executed query-plan proofs.
/// Parameters are the row limit, provider boundary and model-name boundary.
pub const MODEL_PAGE_QUERY: &str = crate::sql::LIST_PLATFORM_MODELS;

/// A unique reviewed decision model created through the real admin API.
#[derive(Debug)]
pub struct DecisionModelFixture {
    /// Catalogue API used to update or remove this fixture.
    pub models: Models,
    /// Persisted reviewed identity and settings.
    pub model: Model,
}

impl DecisionModelFixture {
    /// Create a reviewed model with a maximum-length provider model name.
    ///
    /// # Errors
    /// Returns catalogue failures or an unexpected duplicate fixture identity.
    pub async fn create(database: &Db) -> Result<Self> {
        let models = Models::new(database.clone(), Entropy::new());
        let mut model_name = format!("runner-decision-{}", mint_id());
        model_name.extend(std::iter::repeat_n(
            'x',
            af_wire::inference::MODEL_ID_MAX_BYTES - model_name.len(),
        ));
        let input = ModelInput {
            provider: "typesafe".into(),
            model_name,
            details: details()?,
        };
        let created = models.create(input, clock::now()).await?;
        let CreateModel::Created(model) = created else {
            return Err(crate::error::fixture("duplicate catalogue identity"));
        };
        Ok(Self {
            models,
            model: *model,
        })
    }

    /// Disable the fixture's reviewed decision configuration.
    ///
    /// # Errors
    /// Returns catalogue failures or a missing fixture/profile.
    pub async fn disable(&mut self) -> Result<()> {
        let mut replacement = self.model.details.clone();
        replacement
            .decision
            .as_mut()
            .ok_or_else(|| crate::error::fixture("missing decision profile"))?
            .configuration
            .enabled = false;
        if !self
            .models
            .update(&self.model.id, replacement, clock::now())
            .await?
        {
            return Err(crate::error::fixture("model disappeared before disable"));
        }
        Ok(())
    }

    /// Delete the fixture after all owners have released it.
    ///
    /// # Errors
    /// Returns catalogue failures or a model that was not deleted.
    pub async fn finish(self) -> Result<()> {
        if self.models.delete(&self.model.id).await? != DeleteModel::Deleted {
            return Err(crate::error::fixture("unused model was not deleted"));
        }
        Ok(())
    }
}

fn details() -> Result<ModelUpdate> {
    let now = clock::now();
    Ok(ModelUpdate {
        metadata: Metadata { display_name: "Runner decisions".into(), context_cap_tokens: 64_000,
            context_source_url: "https://example.test/limits".into(), context_verified_at: now, advanced: true },
        inference: None, prices: Vec::new(),
        decision: Some(DecisionModel {
            configuration: serde_json::from_value(json!({"enabled":true,"profile":{
                "revision":"reviewed-1","api":"typesafe","endpoint":"https://api.typesafe.ai/v1/systemone",
                "accepted_models":["evaluated-fixture-model"],"capabilities":{"noul":true,"choice":true,"score":true},
                "input_max_bytes":24_576,
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
            }})).map_err(crate::error::fixture_definition)?,
            prices: vec![Price { effective_from: now, effective_until: None, prompt_tokens_above: 0,
                input_nanos_per_mtok: 42, cached_input_nanos_per_mtok: 42, cache_write_nanos_per_mtok: None,
                cache_write_5m_nanos_per_mtok: None, cache_write_1h_nanos_per_mtok: None,
                output_nanos_per_mtok: 42, source_url: "https://example.test/prices".into(),
                verified_at: now, conditions: "Synthetic fixture; no provider call".into() }],
        }),
    })
}
