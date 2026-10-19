//! Bounded admin model bodies share field-specific public refusal sentences.

use afd_api_wire::admin::{AdminModelCreate, AdminModelPatch};
use afd_validate::Sentences;
use garde::{Unvalidated, Valid};

const DETAIL_BODY_REQUIRED: &str = "Request body required";
const DETAIL_MALFORMED_JSON: &str = "Malformed JSON";
/// The path `garde` reports a provider-length break under.
const FIELD_PROVIDER: &str = "provider";
/// The path a model-identity break is reported under.
const FIELD_MODEL_ID: &str = "model_id";
/// The path a context-ceiling break is reported under.
const FIELD_CONTEXT_CAP: &str = "metadata.context_cap_tokens";
/// The path a label break is reported under.
const FIELD_DISPLAY_NAME: &str = "metadata.display_name";
/// The path a context-evidence URL break is reported under.
const FIELD_CONTEXT_SOURCE: &str = "metadata.context_source_url";
/// The path the schedule's own rule and its length report under.
const FIELD_PRICES: &str = "prices";
/// The path a price's evidence URL break is reported under.
const FIELD_PRICE_SOURCE: &str = "prices[].source_url";
/// The path a price's conditions break is reported under.
const FIELD_PRICE_CONDITIONS: &str = "prices[].conditions";
/// The path a negative tier threshold is reported under.
const FIELD_PRICE_TIER: &str = "prices[].prompt_tokens_above";

const DETAIL_PROVIDER_LEN: &str = "provider must be 1–64 chars";
const DETAIL_MODEL_ID_LEN: &str = "model_id must be 1–256 chars";
const DETAIL_CAP_POSITIVE: &str = "context_cap_tokens must be > 0";
const DETAIL_DISPLAY_NAME_LEN: &str = "metadata.display_name must be 1–128 bytes";
const DETAIL_CONTEXT_SOURCE_LEN: &str = "metadata.context_source_url must be 1–2048 bytes";
/// The schedule's refusal: its length, its base tier, and its date ranges,
/// which must each end after they start and never overlap within one tier.
pub(crate) const DETAIL_PRICES: &str = "prices must hold 1–64 entries, include a tier with prompt_tokens_above = 0, and give each tier date ranges that end after they start and never overlap";
const DETAIL_PRICE_SOURCE_LEN: &str = "prices[].source_url must be 1–2048 bytes";
const DETAIL_PRICE_CONDITIONS_LEN: &str = "prices[].conditions must be at most 1024 bytes";
const DETAIL_PRICE_TIER: &str = "prices[].prompt_tokens_above must be >= 0";
const DETAIL_RATES: &str =
    "every rate in prices must be a decimal string of whole nanos, at most 9223372036854775807";
const DETAIL_INFERENCE_URL: &str = "inference.base_url must be 1–2048 bytes";
const DETAIL_INFERENCE_DIALECT: &str = "inference.dialect must be 1–64 bytes when set";
const DETAIL_INFERENCE_OUTPUT: &str = "inference.max_output_tokens must be > 0";

/// A create body, proved against every bound its type declares. The store is
/// reached only through the [`Valid`] this returns.
pub(super) fn create_request(body: &[u8]) -> Result<Valid<AdminModelCreate<'_>>, &'static str> {
    if body.is_empty() {
        return Err(DETAIL_BODY_REQUIRED);
    }
    let request = afd_http::handler::read_body::<AdminModelCreate<'_>>(body)
        .map_err(|error| unreadable(&error))?;
    Unvalidated::new(request)
        .validate()
        .map_err(|report| BOUNDS.pick(&report))
}

/// A patch body, proved the same way.
pub(super) fn patch_request(body: &[u8]) -> Result<Valid<AdminModelPatch<'_>>, &'static str> {
    if body.is_empty() {
        return Err(DETAIL_BODY_REQUIRED);
    }
    let patch = afd_http::handler::read_body::<AdminModelPatch<'_>>(body)
        .map_err(|error| unreadable(&error))?;
    Unvalidated::new(patch)
        .validate()
        .map_err(|report| BOUNDS.pick(&report))
}

/// The sentence a body that would not parse earns.
///
/// A rate is refused while parsing, because its spelling is the bound
/// (`afd_wire::nanos`). Those two refusals are this daemon's own text and
/// carry no part of the value, so matching them is safe; every other parse
/// failure stays the generic sentence, since `serde_json` may quote the value
/// it rejected and these bodies are typed by an operator.
fn unreadable(error: &serde_json::Error) -> &'static str {
    let text = error.to_string();
    if [afd_wire::nanos::NOT_CANONICAL, afd_wire::nanos::TOO_LARGE]
        .iter()
        .any(|refusal| text.contains(refusal))
    {
        DETAIL_RATES
    } else {
        DETAIL_MALFORMED_JSON
    }
}

/// The sentence a caller is told, keyed by the path of the bound their body
/// broke.
///
/// The wording is a public commitment the dashboard renders, so the bound
/// lives on the type and only the sentence stays here. Create and patch nest
/// the evidence under `metadata` and the schedule under `prices`, so one table
/// serves both. Every bound garde checks is listed; the schedule sentence is
/// the fallback because every unlisted path is inside `prices`. Inference
/// settings carry their own bounds.
const BOUNDS: Sentences = Sentences::new(
    &[
        (FIELD_PROVIDER, DETAIL_PROVIDER_LEN),
        (FIELD_MODEL_ID, DETAIL_MODEL_ID_LEN),
        (FIELD_CONTEXT_CAP, DETAIL_CAP_POSITIVE),
        (FIELD_DISPLAY_NAME, DETAIL_DISPLAY_NAME_LEN),
        (FIELD_CONTEXT_SOURCE, DETAIL_CONTEXT_SOURCE_LEN),
        (FIELD_PRICE_SOURCE, DETAIL_PRICE_SOURCE_LEN),
        (FIELD_PRICE_CONDITIONS, DETAIL_PRICE_CONDITIONS_LEN),
        (FIELD_PRICE_TIER, DETAIL_PRICE_TIER),
        // After the per-price bounds: a negative tier also leaves no base
        // tier, and the caller is owed the sentence for the field they broke.
        (FIELD_PRICES, DETAIL_PRICES),
        ("inference.base_url", DETAIL_INFERENCE_URL),
        ("inference.dialect", DETAIL_INFERENCE_DIALECT),
        ("inference.max_output_tokens", DETAIL_INFERENCE_OUTPUT),
    ],
    DETAIL_PRICES,
);

#[cfg(test)]
mod tests;
