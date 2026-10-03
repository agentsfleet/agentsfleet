//! What a process driving the providers logs of rig's own lines.
//!
//! rig traces every request it builds and every reply it reads, whole, before
//! the loop's scrub has seen them (`rig_core::providers::internal::trace_json`),
//! and one of its warnings repeats a provider's error message, which may quote
//! the conversation. So whatever level an operator names, rig keeps its
//! warnings and nothing below them, and that one warning is off. rig is pinned
//! exactly, so an upgrade that adds a line like it is a reviewed change.

use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Targets;

/// rig's targets: its own `rig::…` names and its module paths.
const RIG: [&str; 2] = ["rig", "rig_core"];
/// The rig module whose warning repeats a provider's error message.
const QUOTES_THE_PROVIDER: &str =
    "rig_core::providers::internal::openai_chat_completions_compatible";

/// The filter for `level`: everything at `level`, rig at warn or quieter, and
/// rig's quoting warning off.
#[must_use]
pub fn log_filter(level: LevelFilter) -> Targets {
    let rig = level.min(LevelFilter::WARN);
    Targets::new()
        .with_default(level)
        .with_targets(RIG.map(|target| (target, rig)))
        .with_target(QUOTES_THE_PROVIDER, LevelFilter::OFF)
}

#[cfg(test)]
#[path = "logs/tests.rs"]
mod tests;
