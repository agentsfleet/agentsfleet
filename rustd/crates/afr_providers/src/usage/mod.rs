//! A finished turn's usage, as rig reports it, split into the counts the
//! daemon bills apart.
//!
//! rig counts every prompt token inside `input_tokens`: cache reads and cache
//! writes included, on every wire (rig-core 0.44 `completion/request.rs`,
//! `Usage`). The daemon prices uncached input, cache reads and each class of
//! cache write at its own rate, so both come out of the prompt total here,
//! once, where every wire meets.
//!
//! Where each wire reports a cache write, and what rig makes of it:
//!
//! | Wire | Write count | Lifetime |
//! |---|---|---|
//! | Messages | `usage.cache_creation_input_tokens` | `usage.cache_creation.ephemeral_5m_input_tokens`, `ephemeral_1h_input_tokens`, read here |
//! | Responses | `usage.input_tokens_details.cache_write_tokens` | none reported |
//! | Chat | `usage.prompt_tokens_details.cache_write_tokens` | none reported |
//! | Gemini, Ollama | none: Gemini's implicit cache only reads | none |
//!
//! A write with no lifetime stays apart from both lifetimes, so the daemon
//! prices it at the tier's unspecified-lifetime write rate.

use rig_core::completion::Usage as Reported;
use serde::Deserialize;
use serde_json::Value;

use crate::provider::Usage;
use afd_wire::inference::InferenceApi;

/// Where a Messages reply keeps its cache writes split by lifetime.
const MESSAGES_CACHE_CREATION: &str = "/usage/cache_creation";

/// A Messages reply's `usage.cache_creation`: the prompt tokens it wrote to
/// the cache, per lifetime. A lifetime the reply leaves out wrote nothing.
#[derive(Debug, Default, Deserialize)]
struct CacheCreation {
    #[serde(default)]
    ephemeral_5m_input_tokens: u64,
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

impl CacheCreation {
    /// The split `raw`, a reply on `api`, reports. Only Messages reports one;
    /// a split it cannot read is no split, and its writes stay unspecified.
    fn of(api: InferenceApi, raw: &Value) -> Self {
        match api {
            InferenceApi::Messages => raw
                .pointer(MESSAGES_CACHE_CREATION)
                .and_then(|split| Self::deserialize(split).ok())
                .unwrap_or_default(),
            InferenceApi::Chat
            | InferenceApi::Responses
            | InferenceApi::Gemini
            | InferenceApi::Ollama => Self::default(),
        }
    }
}

/// What a turn on `api` spent: rig's `reported` usage, with `raw`, the reply
/// the provider sent, for the lifetime split rig does not normalise.
///
/// Each token lands in one count. A lifetime split larger than the write
/// total rig read is the split's: Messages counts its writes beside
/// `input_tokens`, and rig adds only the total it read, so the excess was
/// never inside rig's input and is not taken out of it.
pub(crate) fn spent(api: InferenceApi, reported: &Reported, raw: &Value) -> Usage {
    let cached_input = reported.cached_input_tokens.unwrap_or(0);
    let written = reported.cache_creation_input_tokens.unwrap_or(0);
    let split = CacheCreation::of(api, raw);
    let lifetimes = split
        .ephemeral_5m_input_tokens
        .saturating_add(split.ephemeral_1h_input_tokens);
    Usage {
        input: reported
            .input_tokens
            .unwrap_or(0)
            .saturating_sub(cached_input)
            .saturating_sub(written),
        cached_input,
        cache_write: written.saturating_sub(lifetimes),
        cache_write_5m: split.ephemeral_5m_input_tokens,
        cache_write_1h: split.ephemeral_1h_input_tokens,
        output: reported.output_tokens.unwrap_or(0),
    }
}

#[cfg(test)]
mod tests;
