//! What the wire layer refuses.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: failing loudly on a malformed payload is the correct outcome"
)]

use afd_wire::lease::{LeaseRequest, LeaseResponse};
use afd_wire::report::{RenewRequest, ReportRequest, ReportTelemetry};
use afd_wire::runner::AssignedPolicy;

/// A complete, well-formed `LeaseResponse` — the truncation cases below cut it
/// at several offsets and every cut must fail as a typed error.
const WELL_FORMED_LEASE_RESPONSE: &str = r#"{"lease":{"lease_id":"lease_id","fencing_token":504,"lease_expires_at":931,"secret_delivery":"inline","event":{"event_id":"event_id","fleet_id":"fleet_id","workspace_id":"workspace_id","actor":"actor","event_type":"chat","request_json":"request_json","created_at":426},"policy":{"network_policy":{"allow":["allow"],"read_only":true,"read_post_paths":["read_post_paths"]},"tools":["tools"],"secrets_map":{"secrets_map":"secrets_map"},"mintable":[{"name":"name","integration":"integration"}],"provider":"provider","api_key":"api_key","inference_host":"inference_host","base_url":"base_url","repository_binding":{"repositories":["repositories"],"access":"read","base_branch":"base_branch"},"http_origin_policies":[{"host":"host","credential_names":["credential_names"],"requests":[{"method":"get","path":"path","path_match":"exact","json_fields":[{"name":"name","string_value":"string_value","boolean_value":true}]}]}],"context":{"tool_window":141,"memory_checkpoint_every":26,"stage_chunk_threshold":0.75,"model":"model","context_cap_tokens":499}},"instructions":"instructions","bundle":{"content_hash":"content_hash"}},"retry_after_ms":34}"#;

/// A malformed payload must produce a typed error, never a panic and never a
/// half-built value. Truncation is the realistic shape: a connection dropped
/// mid-body yields valid JSON right up to the cut.
#[test]
fn test_wire_rejects_malformed() {
    let full = WELL_FORMED_LEASE_RESPONSE.as_bytes();

    for cut in [1, full.len() / 4, full.len() / 2, full.len() - 1] {
        let err = serde_json::from_slice::<LeaseResponse<'_>>(&full[..cut]).unwrap_err();
        assert!(
            err.is_eof() || err.is_syntax() || err.is_data(),
            "truncation at {cut} produced an unexpected error class: {err}"
        );
    }

    for garbage in [
        &b""[..],
        &b"null"[..],
        &b"[]"[..],
        &b"{"[..],
        &b"{\"lease\": }"[..],
        &b"\xff\xfe"[..],
    ] {
        assert!(
            serde_json::from_slice::<LeaseResponse<'_>>(garbage).is_err(),
            "accepted garbage: {:?}",
            String::from_utf8_lossy(garbage)
        );
    }
}

/// A field carrying the wrong JSON type is rejected rather than coerced.
#[test]
fn should_reject_a_field_of_the_wrong_type() {
    let _ = serde_json::from_str::<LeaseRequest>(r#"{"wire_version":"2"}"#).unwrap_err();
    let _ = serde_json::from_str::<LeaseRequest>(r#"{"wire_version":null}"#).unwrap_err();
    let _ = serde_json::from_str::<LeaseRequest>(r#"{"wire_version":2.5}"#).unwrap_err();
    let _ = serde_json::from_str::<LeaseRequest>(r#"{"wire_version":2}"#).unwrap();
}

/// An unknown ENUM value is refused rather than silently defaulting, which is
/// what keeps a stray stored tier or posture from resolving to something
/// permissive.
#[test]
fn should_reject_an_unknown_enum_value() {
    let policy = r#"{"sandbox_tier":"macos_seatbelt","network_policy":"allow_all",
        "registry_allowlist":[],"worker_count":1,"extra_binds":[]}"#;
    let err = serde_json::from_str::<AssignedPolicy<'_>>(policy).unwrap_err();
    assert!(err.to_string().contains("macos_seatbelt"), "{err}");

    let ok = r#"{"sandbox_tier":"landlock_full","network_policy":"allow_all",
        "registry_allowlist":[],"worker_count":1,"extra_binds":[]}"#;
    let _ = serde_json::from_str::<AssignedPolicy<'_>>(ok).unwrap();
}

/// The round-trip proves ENCODING parity but cannot prove integer WIDTH parity
/// in the widening direction: any value the Zig side emits fits a wider Rust
/// type and re-serializes identically, so a `u32` mistyped as `u64` round-trips
/// clean. This pins the declared widths directly — a value one past the maximum
/// must be refused, which fails the moment a field is widened.
#[test]
fn should_refuse_values_past_each_declared_integer_width() {
    // u16 — the lease wire version.
    let _ = serde_json::from_str::<LeaseRequest>(r#"{"wire_version":65535}"#).unwrap();
    let _ = serde_json::from_str::<LeaseRequest>(r#"{"wire_version":65536}"#).unwrap_err();

    // u32 — the cumulative token counters on renewal.
    let at_max = r#"{"input_tokens":4294967295,"cached_input_tokens":0,"output_tokens":0}"#;
    let past = r#"{"input_tokens":4294967296,"cached_input_tokens":0,"output_tokens":0}"#;
    let _ = serde_json::from_str::<RenewRequest>(at_max).unwrap();
    let _ = serde_json::from_str::<RenewRequest>(past).unwrap_err();

    // u32 alongside u64 on the same struct: the narrow field must stay narrow
    // even though its neighbour is wide.
    let telemetry = r#"{"time_to_first_token_ms":4294967296,"wall_ms":1}"#;
    let _ = serde_json::from_str::<ReportTelemetry>(telemetry).unwrap_err();
    let wide_neighbour = r#"{"time_to_first_token_ms":1,"wall_ms":18446744073709551615}"#;
    let _ = serde_json::from_str::<ReportTelemetry>(wide_neighbour).unwrap();

    // A negative value in an unsigned field is refused, not wrapped.
    let _ = serde_json::from_str::<LeaseRequest>(r#"{"wire_version":-1}"#).unwrap_err();
}

/// A required field left out is an error, not a default. Zig's wire structs
/// default only the fields explicitly marked defaulted, and a Rust type that
/// silently substituted `0` or `""` would accept payloads the daemon refuses.
#[test]
fn should_reject_a_payload_missing_a_required_field() {
    let err = serde_json::from_str::<ReportRequest<'_>>(r#"{"lease_id":"a"}"#).unwrap_err();
    assert!(err.is_data(), "{err}");
    assert!(err.to_string().contains("missing field"), "{err}");
}

/// The default lease request must ask for the CURRENT wire version.
///
/// Not cosmetic: a default of `1` would have this port asking for the
/// superseded shape it deliberately does not implement, and the daemon would
/// answer with a payload no type here can parse. The Zig struct defaults its
/// field to the version-one constant precisely because its handler treats an
/// empty body as version one — this port has no such path, so its default has
/// to be the current version and a test has to say so.
#[test]
fn should_default_a_lease_request_to_the_current_wire_version() {
    let request = LeaseRequest::default();
    assert_eq!(
        request.wire_version,
        afd_wire::paths::LEASE_WIRE_VERSION_CURRENT
    );
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        r#"{"wire_version":2}"#
    );
}
