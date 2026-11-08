//! Every provider label comes from its closed set, and none is spelled twice.

use std::collections::BTreeSet;

use super::labels::{
    DecisionOutcome, DecisionProvider, DecisionPurpose, OTHER, Provider, RetryReason,
};

/// Public catalogue providers keep their names; private endpoints stay unattributed.
#[test]
fn a_provider_is_a_canonical_public_vendor_or_other() {
    let vendors: BTreeSet<_> = rig_core::providers::registry::ProviderId::all()
        .map(|id| id.vendor())
        .collect();
    let mut emitted = BTreeSet::new();
    for vendor in &vendors {
        let value = Provider::of(vendor).as_str();
        assert_ne!(value, OTHER, "registered SDK provider {vendor}");
        emitted.insert(value);
    }
    for known in af_observability::semconv::provider::WELL_KNOWN {
        assert_eq!(Provider::of(known).as_str(), *known);
        emitted.insert(*known);
    }
    for (configured, expected) in [
        ("fireworks", "fireworks"),
        ("MINIMAX", "minimax"),
        ("openrouter", "openrouter"),
        ("gemini", "gcp.gemini"),
        ("mistral", "mistral_ai"),
        ("moonshot", "moonshot_ai"),
        ("ollama", "ollama"),
    ] {
        assert_eq!(Provider::of(configured).as_str(), expected);
        emitted.insert(expected);
    }
    for name in [
        "",
        "custom:https://models.example.com/v1",
        "private-provider-123",
        "https://models.example.com/v1",
        "minimax/openai",
        "accounts/fireworks/models/minimax-m3p1-flash",
        "Fireworks Inc",
        "模型供应商",
        "_other",
    ] {
        assert_eq!(Provider::of(name).as_str(), OTHER);
    }
    assert_eq!(Provider::count(), emitted.len() + 1);
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
    let sets: [(&str, Vec<&str>); 4] = [
        (
            "RetryReason",
            RetryReason::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "DecisionProvider",
            DecisionProvider::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "DecisionPurpose",
            DecisionPurpose::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "DecisionOutcome",
            DecisionOutcome::ALL.iter().map(|m| m.as_str()).collect(),
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
