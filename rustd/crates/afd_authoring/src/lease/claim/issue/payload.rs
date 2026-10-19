//! Only native setup and bounded text accompany the scoped invocation token.

use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, Limits, ModelAccess, Pricing, SelectedModel};

use super::{FENCE, Placement};
use crate::{
    Result,
    error::{ErrorKind, raise::stored},
    lease::claim::{
        Claimed,
        context::Context,
        selection::{Hint, Selected},
    },
    model::budget,
};

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX))]
    bytes: &'a [u8],
}

pub(super) fn build(
    hint: &Hint,
    selected: &Selected,
    model: &RegisteredModel,
    context: Context,
    placement: &Placement,
) -> Result<Claimed> {
    let payload = Claimed {
        lease_id: placement.lease.to_string(),
        attempt_id: placement.attempt.to_string(),
        draft_id: hint.id.to_string(),
        sequence: selected.sequence.to_string(),
        fence: FENCE.to_string(),
        expires_at: placement.until,
        deadline: placement.deadline,
        model: selected_model(model),
        model_access: ModelAccess {
            token: placement.token.clone(),
            expires_at: placement.until,
            invocation_path: afd_wire::paths::AUTHORING_MODEL_CALLS
                .replace("{lease_id}", placement.lease.as_str()),
        },
        candidate: context.candidate,
        messages: context.messages,
        limits: Limits {
            input_per_call: budget::INPUT_PER_CALL,
            output_per_turn: budget::output_limit(model),
            max_calls: budget::MAX_CALLS,
            wall_seconds: seconds(budget::WALL),
            provider_timeout_seconds: seconds(budget::PROVIDER_TIMEOUT),
        },
    };
    let bytes = serde_json::to_vec(&payload).map_err(stored)?;
    garde::Unvalidated::new(Envelope { bytes: &bytes })
        .validate()
        .map_err(|_oversized| ErrorKind::Token)?;
    Ok(payload)
}

/// A limit the runner protocol spells in whole seconds.
fn seconds(span: std::time::Duration) -> i64 {
    i64::try_from(span.as_secs()).unwrap_or(i64::MAX)
}

fn selected_model(model: &RegisteredModel) -> SelectedModel {
    SelectedModel {
        reference: model.reference.to_string(),
        provider: model.provider.to_string(),
        model: model.model.to_string(),
        pricing: Pricing {
            input_nanos_per_million: model.input_nanos_per_million.to_string(),
            output_nanos_per_million: model.output_nanos_per_million.to_string(),
            runtime_nanos_per_second: afd_billing::RUN_NANOS_PER_SEC.to_string(),
        },
    }
}
