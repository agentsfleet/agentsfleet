//! Choices use the same configured-model predicate as dispatch.

use crate::{
    Result,
    draft::value,
    model,
    types::{ModelEligibility, ModelPricing, ModelPurpose, WorkspaceModel},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct Row {
    pub(super) reference: String,
    provider: String,
    model: String,
    inference: Option<afd_wire::inference::ModelInference>,
    base_url: Option<String>,
    has_key: bool,
    context: i32,
    input: i64,
    output: i64,
}

pub(super) fn decode(raw: &str) -> Result<Row> {
    value(raw)
}

pub(super) fn eligible(row: Row, purpose: Option<ModelPurpose>) -> Option<WorkspaceModel> {
    let route = row
        .has_key
        .then(|| {
            model::configured(
                row.inference.as_ref(),
                row.base_url.as_deref(),
                row.context,
                row.input,
                row.output,
            )
        })
        .flatten();
    let runtime = route.is_some();
    let authoring = route.is_some_and(|route| route.supports_tools(&row.model));
    let allowed = match purpose {
        Some(ModelPurpose::Authoring) => authoring,
        Some(ModelPurpose::Runtime) => runtime,
        None => true,
    };
    allowed.then(|| WorkspaceModel {
        label: format!("{} · {}", row.model, row.provider),
        reference: row.reference,
        provider: row.provider,
        model: row.model,
        context_window_tokens: row.context,
        max_output_tokens: row
            .inference
            .as_ref()
            .map(|settings| i64::from(settings.max_output_tokens)),
        eligibility: ModelEligibility { authoring, runtime },
        pricing: ModelPricing {
            input_nanos_per_million: row.input.to_string(),
            output_nanos_per_million: row.output.to_string(),
            runtime_nanos_per_second: afd_billing::RUN_NANOS_PER_SEC.to_string(),
        },
    })
}
