//! Validate replacements against the current persisted protocol settings.

use afd_core::catalogue::{Metadata, Price};
use afd_wire::{decision::DecisionConfiguration, inference::ModelInference};
use sqlx::types::Json;

use crate::{ModelUpdate, Result};

#[derive(sqlx::FromRow)]
pub(crate) struct CurrentSettings {
    pub(crate) inference: Option<Json<ModelInference>>,
    pub(crate) decision: Option<Json<DecisionConfiguration>>,
}

impl CurrentSettings {
    pub(crate) fn validate_replacement(&self, details: &ModelUpdate) -> Result<()> {
        let decision = details
            .decision
            .as_ref()
            .map(|model| &model.configuration)
            .or(self.decision.as_ref().map(|configuration| &configuration.0));
        validate(
            details
                .inference
                .as_ref()
                .or(self.inference.as_ref().map(|settings| &settings.0)),
            &details.metadata,
            decision,
            &details.prices,
        )?;
        if let (Some(current), Some(changed)) = (self.decision.as_ref(), details.decision.as_ref())
            && current.profile.revision == changed.configuration.profile.revision
            && current.profile != changed.configuration.profile
        {
            return Err(crate::error::decision_revision());
        }
        Ok(())
    }
}

/// Proves saved settings route: a supported API, a safe HTTPS base and an output
/// limit inside the model's context. Absent settings are an unconfigured model.
pub(crate) fn validate(
    inference: Option<&ModelInference>,
    metadata: &Metadata,
    decision: Option<&DecisionConfiguration>,
    coding_prices: &[Price],
) -> Result<()> {
    if coding_prices.is_empty() && (inference.is_some() || decision.is_none()) {
        return Err(crate::error::coding_prices());
    }
    if let Some(inference) = inference {
        let context = u32::try_from(metadata.context_cap_tokens).unwrap_or_default();
        afd_inference::Configured::new(inference, context)?;
    }
    if let Some(configuration) = decision {
        afd_inference::validate_decision(configuration).map_err(crate::error::decision_settings)?;
    }
    Ok(())
}
