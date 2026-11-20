//! Choices use the same eligibility the setup run's model check reads.

use afd_credential::provider::ModelFacts;

use crate::types::{ModelEligibility, ModelPricing, ModelPurpose, WorkspaceModel};

/// [`super::sql::PAGE`]'s row, by column name.
#[derive(sqlx::FromRow)]
pub(super) struct Row {
    pub(super) reference: String,
    provider: String,
    model: String,
    display_name: String,
    inference: Option<sqlx::types::Json<af_wire::inference::ModelInference>>,
    base_url: Option<String>,
    has_key: bool,
    context: i32,
    input: i64,
    output: i64,
}

pub(super) fn eligible(row: Row, purpose: Option<ModelPurpose>) -> Option<WorkspaceModel> {
    let eligible = ModelFacts {
        model: &row.model,
        inference: row.inference.as_ref().map(|settings| &settings.0),
        endpoint: row.base_url.as_deref(),
        context_window_tokens: row.context,
        input_nanos_per_million: row.input,
        output_nanos_per_million: row.output,
    }
    .eligibility();
    let runtime = row.has_key && eligible.runtime;
    let setup = row.has_key && eligible.tools;
    let allowed = match purpose {
        Some(ModelPurpose::Setup) => setup,
        Some(ModelPurpose::Runtime) => runtime,
        None => true,
    };
    allowed.then(|| WorkspaceModel {
        label: row.display_name,
        reference: row.reference,
        provider: row.provider,
        model: row.model,
        context_window_tokens: row.context,
        max_output_tokens: row
            .inference
            .as_ref()
            .map(|settings| i64::from(settings.0.max_output_tokens)),
        eligibility: ModelEligibility { setup, runtime },
        pricing: ModelPricing {
            input_nanos_per_million: row.input.to_string(),
            output_nanos_per_million: row.output.to_string(),
            runtime_nanos_per_second: afd_billing::RUN_NANOS_PER_SEC.to_string(),
        },
    })
}

#[cfg(test)]
mod tests;
