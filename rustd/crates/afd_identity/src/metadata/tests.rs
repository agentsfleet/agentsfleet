//! Behavioural coverage for the signup metadata writeback.

use super::{MetadataMerge, ProviderMetadata, PublicMetadata};
use crate::error::MetadataUnwritten;

/// The payload the provider merges on.
///
/// Pinned as rendered bytes because the KEYS are a wire contract with an
/// external service: the session-token template projects `metadata.tenant_id`,
/// so a renamed field here does not fail a build, it mints tokens that every
/// gate refuses. `clerk_metadata_payload.zig` renders the same two keys under
/// the same object.
#[test]
fn the_payload_names_the_two_keys_the_provider_merges_on() {
    // `ok()` rather than a panic helper: this crate's tests carry no
    // `expect_used` exemption, and `None` fails the comparison just as loudly.
    let rendered = serde_json::to_string(&MetadataMerge {
        public_metadata: PublicMetadata {
            tenant_id: "01920000-0000-7000-8000-000000000000",
            scopes: "workspace:admin",
        },
    })
    .ok();

    assert_eq!(
        rendered.as_deref(),
        Some(
            r#"{"public_metadata":{"tenant_id":"01920000-0000-7000-8000-000000000000","scopes":"workspace:admin"}}"#
        )
    );
}

/// The three outcomes an operator acts on differently.
///
/// A 401 is theirs to fix and a 404 is nobody's, which is why neither is
/// folded into the general failure: an operator reading `Unreachable` waits,
/// and one reading `Unauthorized` goes and looks at `CLERK_SECRET_KEY`.
#[test]
fn every_status_class_maps_to_the_outcome_it_is_repaired_by() {
    for ok in [200_u16, 201, 204, 299] {
        assert_eq!(ProviderMetadata::classify(ok), None, "{ok} is a success");
    }
    for refused in [401_u16, 403] {
        assert_eq!(
            ProviderMetadata::classify(refused),
            Some(MetadataUnwritten::Unauthorized),
            "{refused} is this daemon's own credential"
        );
    }
    assert_eq!(
        ProviderMetadata::classify(404),
        Some(MetadataUnwritten::UnknownSubject)
    );
    for outage in [300_u16, 400, 429, 500, 502, 503] {
        assert_eq!(
            ProviderMetadata::classify(outage),
            Some(MetadataUnwritten::Unreachable),
            "{outage} has no other reading"
        );
    }
}

/// Each failure says something different, so a log line distinguishes them.
#[test]
fn each_outcome_renders_a_distinct_sentence() {
    let rendered: Vec<String> = [
        MetadataUnwritten::Unreachable,
        MetadataUnwritten::Unauthorized,
        MetadataUnwritten::UnknownSubject,
    ]
    .iter()
    .map(ToString::to_string)
    .collect();

    for sentence in &rendered {
        assert!(!sentence.is_empty());
    }
    let mut unique = rendered.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        rendered.len(),
        "each outcome reads differently"
    );
}
