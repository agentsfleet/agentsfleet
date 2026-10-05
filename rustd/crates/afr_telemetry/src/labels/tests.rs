//! Every label value comes from its closed set, and none is spelled twice.

use std::collections::BTreeSet;

use afd_observability::semconv::provider::WELL_KNOWN;
use afr_tools::catalog::PUBLISHED;

use super::{
    FrameDrop, OTHER, Provider, PushFailure, RetryReason, SandboxStart, Tool, ToolOutcome,
    TurnOutcome,
};

/// A provider OpenTelemetry names keeps its name; any other is `_other`.
#[test]
fn a_provider_is_its_well_known_name_or_other() {
    assert_eq!(Provider::of("Anthropic").as_str(), "anthropic");
    assert_eq!(Provider::of("our-internal-gateway").as_str(), OTHER);
    assert_eq!(Provider::COUNT, WELL_KNOWN.len() + 1);
}

/// A tool the catalog publishes keeps its name; a made-up one is `_other`.
#[test]
fn a_tool_is_its_catalog_name_or_other() {
    assert_eq!(Tool::of("file_read").as_str(), "file_read");
    assert_eq!(Tool::of("rm_rf_slash").as_str(), OTHER);
    assert_eq!(Tool::COUNT, PUBLISHED.len() + 1);
}

/// A retried send's status names its reason.
#[test]
fn a_retry_reason_follows_the_status() {
    assert_eq!(RetryReason::of_status(Some(429)), RetryReason::RateLimited);
    assert_eq!(RetryReason::of_status(Some(503)), RetryReason::ServerError);
    assert_eq!(RetryReason::of_status(None), RetryReason::Transport);
}

/// No closed set spells two members alike, and none spells `_other`, which
/// belongs to the open sets alone.
#[test]
fn no_closed_set_spells_two_members_alike() {
    let sets: [(&str, Vec<&str>); 6] = [
        (
            "TurnOutcome",
            TurnOutcome::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "RetryReason",
            RetryReason::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "SandboxStart",
            SandboxStart::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "FrameDrop",
            FrameDrop::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "PushFailure",
            PushFailure::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "ToolOutcome",
            ToolOutcome::ALL.iter().map(|m| m.as_str()).collect(),
        ),
    ];
    for (name, spellings) in sets {
        let distinct: BTreeSet<&str> = spellings.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            spellings.len(),
            "`{name}` repeats a spelling"
        );
        assert!(
            !distinct.contains(OTHER),
            "`{name}` spells the overflow value"
        );
    }
}
