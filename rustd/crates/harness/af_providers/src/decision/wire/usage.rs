//! Cache categories are subsets of input; aggregate input is never multiplied.

use af_wire::decision::DecisionUsage;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct RawUsage {
    input_tokens: u32,
    output_tokens: u32,
    #[serde(default)]
    cached_input_tokens: u32,
    #[serde(default)]
    cache_write_tokens: u32,
    input_tokens_details: Option<InputDetails>,
}

#[derive(Deserialize)]
struct InputDetails {
    #[serde(default)]
    cached_tokens: u32,
    #[serde(default)]
    cache_write_tokens: u32,
}

pub(super) fn decode(value: Option<&Value>) -> Option<DecisionUsage> {
    let raw = RawUsage::deserialize(value?).ok()?;
    let (cached, written) = match raw.input_tokens_details {
        Some(details) if raw.cached_input_tokens == 0 && raw.cache_write_tokens == 0 => {
            (details.cached_tokens, details.cache_write_tokens)
        }
        Some(_) => return None,
        None => (raw.cached_input_tokens, raw.cache_write_tokens),
    };
    let usage = DecisionUsage {
        input_tokens: raw.input_tokens,
        output_tokens: raw.output_tokens,
        cached_input_tokens: cached,
        cache_write_tokens: written,
    };
    usage.uncached_input()?;
    Some(usage)
}
