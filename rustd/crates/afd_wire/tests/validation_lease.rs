//! A lease's sandbox size: each bound at its limit and one past it, and the
//! field's decoding when a daemon sends none.
//!
//! The rule of `validation.rs` holds: the bounds are enumerable, so each row
//! is the pair at the limit and one past it. The runner's call to `validate`
//! is the runner's own test (`afr_supervisor`'s `lease_loop::workspace`).
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a lease that will not decode is an unmet precondition"
)]

use afd_wire::lease::{
    LeasePayload, SANDBOX_CPU_MILLIS_MAX, SANDBOX_CPU_MILLIS_MIN, SANDBOX_DISK_BYTES_MAX,
    SANDBOX_DISK_BYTES_MIN, SANDBOX_MEMORY_BYTES_MAX, SANDBOX_MEMORY_BYTES_MIN, SandboxLimits,
};
use garde::Validate as _;

/// One gibibyte.
const GIB: u64 = 1 << 30;

/// The smallest size every bound admits.
const SMALLEST: SandboxLimits = SandboxLimits {
    cpu_millis: SANDBOX_CPU_MILLIS_MIN,
    memory_bytes: SANDBOX_MEMORY_BYTES_MIN,
    disk_bytes: SANDBOX_DISK_BYTES_MIN,
};

/// The largest size every bound admits.
const LARGEST: SandboxLimits = SandboxLimits {
    cpu_millis: SANDBOX_CPU_MILLIS_MAX,
    memory_bytes: SANDBOX_MEMORY_BYTES_MAX,
    disk_bytes: SANDBOX_DISK_BYTES_MAX,
};

#[test]
fn a_size_at_every_bound_is_accepted() {
    SMALLEST.validate().unwrap();
    LARGEST.validate().unwrap();
}

#[test]
fn a_size_one_past_any_bound_is_refused() {
    let past = [
        SandboxLimits {
            cpu_millis: SANDBOX_CPU_MILLIS_MIN - 1,
            ..SMALLEST
        },
        SandboxLimits {
            cpu_millis: SANDBOX_CPU_MILLIS_MAX + 1,
            ..LARGEST
        },
        SandboxLimits {
            memory_bytes: SANDBOX_MEMORY_BYTES_MIN - 1,
            ..SMALLEST
        },
        SandboxLimits {
            memory_bytes: SANDBOX_MEMORY_BYTES_MAX + 1,
            ..LARGEST
        },
        SandboxLimits {
            disk_bytes: SANDBOX_DISK_BYTES_MIN - 1,
            ..SMALLEST
        },
        SandboxLimits {
            disk_bytes: SANDBOX_DISK_BYTES_MAX + 1,
            ..LARGEST
        },
    ];
    for size in past {
        assert!(size.validate().is_err(), "{size:?} must be refused");
    }
}

/// The lease the daemon sends today, before any fleet names a size, plus or
/// minus the `limits` key.
fn lease(limits: Option<&str>) -> String {
    let limits = limits.map_or_else(String::new, |value| format!(r#","limits":{value}"#));
    format!(
        r#"{{"lease_id":"l","fencing_token":1,"lease_expires_at":0,"secret_delivery":"inline",
        "event":{{"event_id":"e","fleet_id":"f","workspace_id":"w","actor":"a",
        "event_type":"chat","request_json":"{{}}","created_at":0}},
        "policy":{{"network_policy":{{"allow":[],"read_only":true,"read_post_paths":[]}},
        "tools":[],"secrets_map":null,"mintable":[],"provider":"p","api_key":"k",
        "inference_host":"h","base_url":null,"repository_binding":null,
        "http_origin_policies":[],"context":{{"tool_window":1,"memory_checkpoint_every":1,
        "stage_chunk_threshold":0.5,"model":"m","context_cap_tokens":0}}}},
        "instructions":"","bundle":null{limits}}}"#
    )
}

/// A daemon that predates the field sends no key; the lease still decodes,
/// sizeless, and re-encodes the null the field now writes.
#[test]
fn a_lease_without_a_size_decodes_as_none() {
    for text in [lease(None), lease(Some("null"))] {
        let payload: LeasePayload<'_> = serde_json::from_str(&text).unwrap();
        assert_eq!(payload.limits, None);
        let encoded = serde_json::to_value(&payload).unwrap();
        assert_eq!(encoded["limits"], serde_json::Value::Null);
    }
}

#[test]
fn a_lease_with_a_size_carries_it() {
    let text = lease(Some(
        r#"{"cpu_millis":4000,"memory_bytes":8589934592,"disk_bytes":21474836480}"#,
    ));
    let payload: LeasePayload<'_> = serde_json::from_str(&text).unwrap();
    assert_eq!(
        payload.limits,
        Some(SandboxLimits {
            cpu_millis: 4_000,
            memory_bytes: 8 * GIB,
            disk_bytes: 20 * GIB,
        })
    );
}

/// The published schema states the same bounds garde enforces. utoipa takes
/// only literals for them, so this is what keeps the two spellings one.
#[cfg(feature = "openapi")]
#[test]
fn the_published_bounds_are_the_enforced_ones() {
    let schema = serde_json::to_value(<SandboxLimits as utoipa::PartialSchema>::schema()).unwrap();
    for (field, min, max) in [
        (
            "cpu_millis",
            u64::from(SANDBOX_CPU_MILLIS_MIN),
            u64::from(SANDBOX_CPU_MILLIS_MAX),
        ),
        (
            "memory_bytes",
            SANDBOX_MEMORY_BYTES_MIN,
            SANDBOX_MEMORY_BYTES_MAX,
        ),
        ("disk_bytes", SANDBOX_DISK_BYTES_MIN, SANDBOX_DISK_BYTES_MAX),
    ] {
        let property = &schema["properties"][field];
        assert_eq!(property["minimum"].as_u64(), Some(min), "{field} minimum");
        assert_eq!(property["maximum"].as_u64(), Some(max), "{field} maximum");
    }
}

/// A lease from a daemon that predates the field decodes with no turns, and
/// re-encodes the empty list the field now always writes.
#[test]
fn test_lease_history_defaults_empty() {
    let text = lease(None);
    let payload: LeasePayload<'_> = serde_json::from_str(&text).unwrap();
    assert_eq!(payload.history, [] as [afd_wire::lease::Turn<'_>; 0]);
    let encoded = serde_json::to_value(&payload).unwrap();
    assert_eq!(encoded["history"], serde_json::json!([]));
}
