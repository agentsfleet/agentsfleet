//! What the wire layer refuses.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: failing loudly on a malformed payload is the correct outcome"
)]

use afd_wire::activity::{ActivityAccepted, ActivityRequest};
use afd_wire::credentials::{MintCredentialRequest, MintCredentialResponse};
use afd_wire::lease::LeaseResponse;
use afd_wire::memory::{MemoryCaptureResponse, MemoryHydrateResponse, MemoryPushRequest};
use afd_wire::report::{
    RenewRequest, RenewResponse, ReportRequest, ReportResponse, ReportTelemetry,
};
use afd_wire::runner::{AssignedPolicy, HeartbeatRequest, HeartbeatResponse, SelfResponse};
use serde_json::Value;

/// A complete, well-formed `LeaseResponse` — the truncation cases below cut it
/// at several offsets and every cut must fail as a typed error.
const WELL_FORMED_LEASE_RESPONSE: &str = r#"{"lease":{"lease_id":"lease_id","fencing_token":504,"lease_expires_at":931,"secret_delivery":"inline","event":{"event_id":"event_id","fleet_id":"fleet_id","workspace_id":"workspace_id","actor":"actor","event_type":"chat","request_json":"request_json","created_at":426},"policy":{"network_policy":{"allow":["allow"],"read_only":true,"read_post_paths":["read_post_paths"]},"tools":["tools"],"secrets_map":{"secrets_map":"secrets_map"},"mintable":[{"name":"name","integration":"integration"}],"provider":"provider","api_key":"api_key","inference_host":"inference_host","base_url":"base_url","repository_binding":{"repositories":["repositories"],"access":"read","base_branch":"base_branch"},"http_origin_policies":[{"host":"host","credential_names":["credential_names"],"requests":[{"method":"get","path":"path","path_match":"exact","json_fields":[{"name":"name","string_value":"string_value","boolean_value":true}]}]}],"context":{"tool_window":141,"memory_checkpoint_every":26,"stage_chunk_threshold":0.75,"model":"model","context_cap_tokens":499}},"instructions":"instructions","bundle":{"content_hash":"content_hash"}},"retry_after_ms":34}"#;

/// A malformed payload must produce a typed error, never a panic and never a
/// half-built value. Truncation is the realistic shape: a connection dropped
/// mid-body yields valid JSON right up to the cut.
#[test]
fn test_wire_rejects_malformed() {
    let full = WELL_FORMED_LEASE_RESPONSE.as_bytes();

    // Parse it whole FIRST. Without this the truncation loop below asserts
    // `is_err()` on every cut and passes identically if the constant were the
    // string "garbage" — the premise it rests on ("valid JSON right up to the
    // cut") would be an unasserted claim. This line is also the only surviving
    // check that every field name and enum spelling of a full nested
    // `LeaseResponse` still deserializes, which the deleted wire corpus used to
    // own.
    assert!(
        serde_json::from_slice::<LeaseResponse<'_>>(full).is_ok(),
        "the literal above must be a well-formed LeaseResponse"
    );

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
    let ok = r#"{"input_tokens":1,"cached_input_tokens":0,"output_tokens":0}"#;
    let string = r#"{"input_tokens":"1","cached_input_tokens":0,"output_tokens":0}"#;
    let null = r#"{"input_tokens":null,"cached_input_tokens":0,"output_tokens":0}"#;
    let fractional = r#"{"input_tokens":1.5,"cached_input_tokens":0,"output_tokens":0}"#;
    let _ = serde_json::from_str::<RenewRequest>(string).unwrap_err();
    let _ = serde_json::from_str::<RenewRequest>(null).unwrap_err();
    let _ = serde_json::from_str::<RenewRequest>(fractional).unwrap_err();
    let _ = serde_json::from_str::<RenewRequest>(ok).unwrap();
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
    let negative = r#"{"input_tokens":-1,"cached_input_tokens":0,"output_tokens":0}"#;
    let _ = serde_json::from_str::<RenewRequest>(negative).unwrap_err();
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

/// Whether a body decodes into one runner-bound reply type.
type Decodes = fn(&[u8]) -> bool;

/// A runner-written body type's refusal of a body, rendered; `None` if it decoded.
type Refusal = fn(&[u8]) -> Option<String>;

/// The key a newer daemon might add; no type in this crate carries it.
const FUTURE: &str = "future";

/// Adds [`FUTURE`] to the object at `pointer` inside `document`.
fn grow(document: &mut Value, pointer: &str) {
    let object = document.pointer_mut(pointer).and_then(Value::as_object_mut);
    assert!(object.is_some(), "{pointer} names an object in the fixture");
    object.unwrap().insert(FUTURE.to_owned(), Value::from(1));
}

/// A lease grown at every level still decodes, so a daemon that adds a field to
/// any part of a lease never strands a runner built before it.
#[test]
fn test_daemon_payload_with_unknown_field_decodes() {
    let mut document: Value = serde_json::from_str(WELL_FORMED_LEASE_RESPONSE).unwrap();
    for pointer in [
        "",
        "/lease",
        "/lease/event",
        "/lease/bundle",
        "/lease/policy",
        "/lease/policy/network_policy",
        "/lease/policy/mintable/0",
        "/lease/policy/repository_binding",
        "/lease/policy/http_origin_policies/0",
        "/lease/policy/http_origin_policies/0/requests/0",
        "/lease/policy/http_origin_policies/0/requests/0/json_fields/0",
        "/lease/policy/context",
    ] {
        grow(&mut document, pointer);
    }
    let grown = serde_json::to_vec(&document).unwrap();

    let decoded = serde_json::from_slice::<LeaseResponse<'_>>(&grown).unwrap();

    assert_eq!(decoded.lease.unwrap().fencing_token, 504);
}

/// The assigned policy, as every reply that carries it spells it, with a bind
/// and a key no build knows at both levels.
const GROWN_POLICY: &str = r#"{"sandbox_tier":"landlock_full","network_policy":"allow_all",
    "registry_allowlist":[],"worker_count":1,"future":1,
    "extra_binds":[{"path":"/opt/cache","mode":"read_only","note":"cache","future":1}]}"#;

/// Every other reply the runner reads accepts a key it does not carry.
///
/// Each row is a non-capturing closure, so the table is one array of function
/// pointers rather than a test per type that drifts as the list grows.
#[test]
fn test_every_runner_bound_reply_accepts_an_unknown_field() {
    let heartbeat = format!(
        r#"{{"status":"ok","assigned_policy":{GROWN_POLICY},"degraded":false,"degraded_reason":null,"selftest_requested":false,"heartbeat_interval_ms":10000,"future":1}}"#
    );
    let own = format!(
        r#"{{"id":"r","status":"active","host_id":"h","sandbox_tier":"landlock_full","last_seen_at":1,"assigned_policy":{GROWN_POLICY},"achievable":null,"degraded":false,"degraded_reason":null,"future":1}}"#
    );
    let cases: [(&str, String, Decodes); 8] = [
        ("heartbeat", heartbeat, |b| {
            serde_json::from_slice::<HeartbeatResponse<'_>>(b).is_ok()
        }),
        ("self", own, |b| {
            serde_json::from_slice::<SelfResponse<'_>>(b).is_ok()
        }),
        (
            "hydrate",
            r#"{"memory":[{"key":"k","content":"c","category":"core","future":1}],"future":1}"#
                .to_owned(),
            |b| serde_json::from_slice::<MemoryHydrateResponse<'_>>(b).is_ok(),
        ),
        (
            "capture",
            r#"{"stored":1,"skipped":0,"evicted":7}"#.to_owned(),
            |b| serde_json::from_slice::<MemoryCaptureResponse>(b).is_ok(),
        ),
        (
            "mint",
            r#"{"token":"t","expires_at_ms":1,"future":1}"#.to_owned(),
            |b| serde_json::from_slice::<MintCredentialResponse<'_>>(b).is_ok(),
        ),
        ("activity", r#"{"ok":true,"future":1}"#.to_owned(), |b| {
            serde_json::from_slice::<ActivityAccepted>(b).is_ok()
        }),
        (
            "renew",
            r#"{"lease_expires_at":9,"future":1}"#.to_owned(),
            |b| serde_json::from_slice::<RenewResponse>(b).is_ok(),
        ),
        ("report", r#"{"ok":true,"future":1}"#.to_owned(), |b| {
            serde_json::from_slice::<ReportResponse>(b).is_ok()
        }),
    ];

    for (name, body, decodes) in &cases {
        assert!(
            decodes(body.as_bytes()),
            "the {name} reply refused a key it does not carry"
        );
    }
}

/// What the runner WRITES stays closed: an unknown key is refused on sight,
/// before serde even notices the fields that are missing.
#[test]
fn test_runner_written_bodies_still_refuse_an_unknown_field() {
    let body = br#"{"future":1}"#;
    let refusals: [(&str, Refusal); 6] = [
        ("report", |b| {
            serde_json::from_slice::<ReportRequest<'_>>(b)
                .err()
                .map(|e| e.to_string())
        }),
        ("renew", |b| {
            serde_json::from_slice::<RenewRequest>(b)
                .err()
                .map(|e| e.to_string())
        }),
        ("heartbeat", |b| {
            serde_json::from_slice::<HeartbeatRequest<'_>>(b)
                .err()
                .map(|e| e.to_string())
        }),
        ("activity", |b| {
            serde_json::from_slice::<ActivityRequest<'_>>(b)
                .err()
                .map(|e| e.to_string())
        }),
        ("memory push", |b| {
            serde_json::from_slice::<MemoryPushRequest<'_>>(b)
                .err()
                .map(|e| e.to_string())
        }),
        ("mint", |b| {
            serde_json::from_slice::<MintCredentialRequest<'_>>(b)
                .err()
                .map(|e| e.to_string())
        }),
    ];

    for (name, refusal) in &refusals {
        let reason = refusal(body);
        assert!(
            reason
                .as_deref()
                .is_some_and(|text| text.starts_with("unknown field `future`")),
            "{name}: {reason:?}"
        );
    }
}
