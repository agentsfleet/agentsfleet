//! Eligibility reuses the dispatch predicate and the runtime endpoint registry.

use crate::{
    model,
    types::{ModelEligibility, ModelPricing, ModelPurpose, WorkspaceModel},
};

/// [`super::sql::PAGE`]'s row, by column name.
#[derive(sqlx::FromRow)]
pub(super) struct Row {
    pub(super) reference: String,
    provider: String,
    model: String,
    base_url: Option<String>,
    has_key: bool,
    context: i32,
    input: i64,
    output: i64,
}

pub(super) fn eligible(row: Row, purpose: Option<ModelPurpose>) -> Option<WorkspaceModel> {
    let authoring = row.has_key
        && row.base_url.is_none()
        && model::eligible(
            &row.provider,
            &row.model,
            row.context,
            row.input,
            row.output,
        );
    let runtime =
        afd_credential::provider::validate_endpoint_pair(&row.provider, row.base_url.as_deref())
            .is_ok();
    let allowed = match purpose {
        Some(ModelPurpose::Authoring) => authoring,
        Some(ModelPurpose::Runtime) => runtime,
        None => authoring || runtime,
    };
    allowed.then(|| WorkspaceModel {
        label: format!("{} · {}", row.model, row.provider),
        reference: row.reference,
        provider: row.provider,
        model: row.model,
        context_window_tokens: row.context,
        max_output_tokens: model::budget::OUTPUT_PER_TURN.min(i64::from(row.context)),
        eligibility: ModelEligibility { authoring, runtime },
        pricing: ModelPricing {
            input_nanos_per_million: row.input.to_string(),
            output_nanos_per_million: row.output.to_string(),
            runtime_nanos_per_second: afd_billing::RUN_NANOS_PER_SEC.to_string(),
        },
    })
}
