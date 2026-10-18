//! Bounded admin model bodies share field-specific public refusal sentences.

use afd_api_wire::admin::{AdminModelCreate, AdminModelPatch};
use afd_validate::Sentences;
use garde::Validate as _;

const DETAIL_BODY_REQUIRED: &str = "Request body required";
const DETAIL_MALFORMED_JSON: &str = "Malformed JSON";
/// The path `garde` reports a provider-length break under.
const FIELD_PROVIDER: &str = "provider";
/// The path a model-identity break is reported under.
const FIELD_MODEL_ID: &str = "model_id";
/// The path a context-ceiling break is reported under.
const FIELD_CONTEXT_CAP: &str = "context_cap_tokens";
/// The same break from the create, where the rates nest under `rates`.
const FIELD_RATES_CONTEXT_CAP: &str = "rates.context_cap_tokens";

const DETAIL_PROVIDER_LEN: &str = "provider must be 1–64 chars";
const DETAIL_MODEL_ID_LEN: &str = "model_id must be 1–256 chars";
const DETAIL_CAP_POSITIVE: &str = "context_cap_tokens must be > 0";
const DETAIL_RATES_NONNEGATIVE: &str = "rates (input/cached/output nanos_per_mtok) must be >= 0";
const DETAIL_INFERENCE_URL: &str = "inference.base_url must be 1–2048 bytes";
const DETAIL_INFERENCE_DIALECT: &str = "inference.dialect must be 1–64 bytes when set";
const DETAIL_INFERENCE_OUTPUT: &str = "inference.max_output_tokens must be > 0";

pub(super) fn create_request(body: &[u8]) -> Result<AdminModelCreate<'_>, &'static str> {
    if body.is_empty() {
        return Err(DETAIL_BODY_REQUIRED);
    }
    let request = afd_http::handler::read_body::<AdminModelCreate<'_>>(body)
        .map_err(|_error| DETAIL_MALFORMED_JSON)?;
    request.validate().map_err(|report| BOUNDS.pick(&report))?;
    Ok(request)
}

pub(super) fn rates_request(body: &[u8]) -> Result<AdminModelPatch, &'static str> {
    if body.is_empty() {
        return Err(DETAIL_BODY_REQUIRED);
    }
    let rates = afd_http::handler::read_body::<AdminModelPatch>(body)
        .map_err(|_error| DETAIL_MALFORMED_JSON)?;
    rates.validate().map_err(|report| BOUNDS.pick(&report))?;
    Ok(rates)
}

/// The sentence a caller is told, keyed by the path of the bound their body
/// broke.
///
/// The wording is a public commitment the dashboard renders, so the bound
/// lives on the type and only the sentence stays here. The rates ride
/// FLATTENED on the wire but nested in the create type, so `dive` reports the
/// ceiling as `rates.context_cap_tokens` there and `context_cap_tokens` from
/// the patch; both spellings are listed. Inference settings carry their own bounds.
const BOUNDS: Sentences = Sentences::new(
    &[
        (FIELD_PROVIDER, DETAIL_PROVIDER_LEN),
        (FIELD_MODEL_ID, DETAIL_MODEL_ID_LEN),
        (FIELD_CONTEXT_CAP, DETAIL_CAP_POSITIVE),
        (FIELD_RATES_CONTEXT_CAP, DETAIL_CAP_POSITIVE),
        ("inference.base_url", DETAIL_INFERENCE_URL),
        ("inference.dialect", DETAIL_INFERENCE_DIALECT),
        ("inference.max_output_tokens", DETAIL_INFERENCE_OUTPUT),
    ],
    DETAIL_RATES_NONNEGATIVE,
);

#[cfg(test)]
mod tests;
