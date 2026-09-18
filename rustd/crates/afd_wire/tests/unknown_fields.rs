//! Which wire types tolerate a field this build has never heard of.
//!
//! # Why this is its own suite
//!
//! `deny_unknown_fields` is not decoration and it is not uniform. The daemon
//! and the runner ship and upgrade separately, so a runner one release behind
//! sends the shape it was built with. On a type that tolerates the unknown, a
//! field added by a newer peer is ignored and the request still serves; on a
//! type that refuses, the same field fails the whole request. Both are correct
//! answers to different questions — an ASSIGNMENT flowing down to a host should
//! refuse what it cannot fully honour, while a REPORT flowing up should not be
//! rejected because the sender learned a new field.
//!
//! That split used to be graded. The wire corpus emitted one
//! `<type>_unknown_field` case per roster entry, probing each document with an
//! extra key and comparing the outcome against a per-type policy recorded in
//! the fixture manifest. The corpus was deleted with the Zig emitter when
//! `afd_wire` became the source of truth, and the byte-parity half of it went
//! with no loss — but the policy half was never about Zig, and nothing replaced
//! it. This is the replacement, Rust-only.
//!
//! # Why the payloads are built rather than written
//!
//! Each case constructs a real value, serialises it, injects one unknown key at
//! the top level and parses it back. A hand-written JSON payload per type would
//! be a second spelling of the struct, free to drift from it silently — which
//! is the failure the deleted corpus actually suffered when three types lost
//! their `pub` and quietly stopped being exported. Here the compiler owns the
//! shape: add a required field and this file stops compiling, which is the
//! correct moment to notice.
//!
//! The probe goes at the top level only. A nested unknown is the nested type's
//! own policy, and that type appears in this table on its own row where it is
//! reachable.

use std::borrow::Cow;

use afd_wire::lease::{BundleManifest, LeaseRequest};
use afd_wire::memory::{MemoryDelta, MemoryPushRequest};
use afd_wire::report::{RenewRequest, RenewResponse, ReportTelemetry};
use afd_wire::runner::{
    AssignedPolicy, BindMode, CapabilityReport, ExtraBind, HeartbeatRequest, HeartbeatResponse,
    HeartbeatStatus, NetworkPolicy, RegisterRequest, RegisterResponse, SandboxTier, SelftestCheck,
    SelftestReport,
};

/// The key injected into every probe.
///
/// Deliberately not a plausible field name: a collision with a real field would
/// turn this suite green for the wrong reason, and a name nobody would ever add
/// cannot collide by accident.
const UNKNOWN_KEY: &str = "__field_this_build_has_never_heard_of__";

/// What a type does with [`UNKNOWN_KEY`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Policy {
    /// Parses anyway — a newer peer may add fields without breaking this one.
    Tolerates,
    /// Refuses, naming the field. Carries `#[serde(deny_unknown_fields)]`.
    Refuses,
}

/// Asserts one type's policy, building the payload from the value itself.
///
/// A macro rather than a generic function, and the reason is the wire's own
/// design: every text field is `Cow<'a, str>` behind `#[serde(borrow)]`, so
/// these types are `Deserialize<'de>` and never `DeserializeOwned` — a generic
/// helper cannot name the lifetime, because the string being parsed is a local
/// the helper itself created. Expanded at the call site the type is concrete
/// and the local outlives the parse, which is exactly what borrowing needs.
///
/// Three steps per case: round-trip the value untouched, so a failure below
/// reads as a policy change rather than a payload built wrong; inject
/// [`UNKNOWN_KEY`] at the top level; parse the result and classify.
macro_rules! assert_policy {
    ($ty:ty, $value:expr, $expected:expr $(,)?) => {{
        let label = stringify!($ty);
        let value: $ty = $value;

        let encoded = serde_json::to_string(&value)
            .unwrap_or_else(|error| panic!("{label}: could not serialise: {error}"));
        serde_json::from_str::<$ty>(&encoded)
            .unwrap_or_else(|error| panic!("{label}: will not round-trip unprobed: {error}"));

        let mut document = match serde_json::to_value(&value) {
            Ok(serde_json::Value::Object(map)) => map,
            Ok(other) => panic!("{label}: expected a JSON object, got {other}"),
            Err(error) => panic!("{label}: could not serialise: {error}"),
        };
        assert!(
            document
                .insert(UNKNOWN_KEY.to_owned(), serde_json::Value::from(1))
                .is_none(),
            "{label}: {UNKNOWN_KEY} collided with a real field"
        );
        let probed = serde_json::Value::Object(document).to_string();

        let policy = match serde_json::from_str::<$ty>(&probed) {
            Ok(_accepted) => Policy::Tolerates,
            Err(error) => {
                assert!(
                    error.to_string().contains(UNKNOWN_KEY),
                    "{label}: refused for a reason other than the unknown field: {error}"
                );
                Policy::Refuses
            }
        };

        assert_eq!(policy, $expected, "{label}");
    }};
}

fn extra_bind() -> ExtraBind<'static> {
    ExtraBind {
        path: Cow::Borrowed("/srv/cache"),
        mode: BindMode::ReadOnly,
        note: Cow::Borrowed("shared build cache"),
    }
}

fn assigned_policy() -> AssignedPolicy<'static> {
    AssignedPolicy {
        sandbox_tier: SandboxTier::LandlockFull,
        network_policy: NetworkPolicy::AllowListEgress,
        registry_allowlist: vec![Cow::Borrowed("ghcr.io")],
        worker_count: 4,
        extra_binds: vec![extra_bind()],
    }
}

fn capability_report() -> CapabilityReport<'static> {
    CapabilityReport {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec![Cow::Borrowed("cpu"), Cow::Borrowed("memory")],
        bubblewrap: false,
        egress_enforcement: true,
    }
}

fn selftest_check() -> SelftestCheck<'static> {
    SelftestCheck {
        name: Cow::Borrowed("landlock"),
        ok: true,
        detail: Cow::Borrowed("ruleset applied"),
    }
}

fn selftest_report() -> SelftestReport<'static> {
    SelftestReport {
        checks: vec![selftest_check()],
        all_ok: true,
        sandbox_tier: Cow::Borrowed("landlock_full"),
        network_policy: Cow::Borrowed("allow_list_egress"),
    }
}

fn memory_delta() -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Borrowed("incident-4711"),
        content: Cow::Borrowed("escalated once"),
        category: Cow::Borrowed("core"),
    }
}

/// Everything a runner SENDS UP tolerates an unknown field.
///
/// These are the types a host one release ahead of the control plane puts on
/// the wire. Refusing here would mean a runner upgrade takes the fleet down
/// until the daemon catches up, which is the wrong failure: the daemon reads
/// the fields it knows and ignores the rest.
#[test]
fn every_report_from_a_runner_tolerates_a_field_the_daemon_does_not_know() {
    assert_policy!(
        LeaseRequest,
        LeaseRequest { wire_version: 2 },
        Policy::Tolerates,
    );
    assert_policy!(
        RenewRequest,
        RenewRequest {
            input_tokens: 512,
            cached_input_tokens: 128,
            output_tokens: 256,
        },
        Policy::Tolerates,
    );
    assert_policy!(ExtraBind, extra_bind(), Policy::Tolerates);
    assert_policy!(AssignedPolicy, assigned_policy(), Policy::Tolerates);
    assert_policy!(CapabilityReport, capability_report(), Policy::Tolerates);
    assert_policy!(SelftestCheck, selftest_check(), Policy::Tolerates);
    assert_policy!(SelftestReport, selftest_report(), Policy::Tolerates);
    assert_policy!(
        HeartbeatRequest,
        HeartbeatRequest {
            capability_report: Some(capability_report()),
            selftest: Some(selftest_report()),
        },
        Policy::Tolerates,
    );
    assert_policy!(
        MemoryPushRequest,
        MemoryPushRequest {
            lease_id: Cow::Borrowed("lease_7"),
            fencing_token: 91,
            memory: vec![memory_delta()],
        },
        Policy::Tolerates,
    );
}

/// The strict half, so the lenient half above is a choice rather than the
/// absence of one.
///
/// A suite that only asserted tolerance would still pass if every
/// `deny_unknown_fields` in the crate were deleted. These rows fail in that
/// case, which is what makes the split above mean something.
#[test]
fn the_types_that_refuse_an_unknown_field_still_refuse_it() {
    assert_policy!(
        BundleManifest,
        BundleManifest {
            content_hash: Cow::Borrowed("cafebabe"),
        },
        Policy::Refuses,
    );
    assert_policy!(
        RenewResponse,
        RenewResponse {
            lease_expires_at: 1_700_000_000,
        },
        Policy::Refuses,
    );
    assert_policy!(
        ReportTelemetry,
        ReportTelemetry {
            time_to_first_token_ms: 180,
            wall_ms: 4_200,
        },
        Policy::Refuses,
    );
    assert_policy!(MemoryDelta, memory_delta(), Policy::Refuses);
    assert_policy!(
        RegisterRequest,
        RegisterRequest {
            host_id: Cow::Borrowed("host_1"),
            assigned_policy: assigned_policy(),
            labels: vec![Cow::Borrowed("arm64")],
        },
        Policy::Refuses,
    );
    assert_policy!(
        RegisterResponse,
        RegisterResponse {
            runner_id: Cow::Borrowed("runner_1"),
            runner_token: Cow::Borrowed("token"),
            assigned_policy: assigned_policy(),
        },
        Policy::Refuses,
    );
    assert_policy!(
        HeartbeatResponse,
        HeartbeatResponse {
            status: HeartbeatStatus::Ok,
            assigned_policy: Some(assigned_policy()),
            degraded: false,
            degraded_reason: None,
            selftest_requested: false,
        },
        Policy::Refuses,
    );
}

/// The probe distinguishes the two outcomes, rather than reporting whichever
/// one it always reports.
///
/// Without this, an `assert_policy!` that silently classified everything as
/// `Tolerates` would leave the first test green and prove nothing. These two
/// rows sit in the same module one screen apart, one with the attribute and one
/// without, so a probe that cannot tell them apart fails here.
#[test]
fn the_probe_distinguishes_the_two_outcomes() {
    assert_policy!(
        LeaseRequest,
        LeaseRequest { wire_version: 2 },
        Policy::Tolerates
    );
    assert_policy!(
        BundleManifest,
        BundleManifest {
            content_hash: Cow::Borrowed("cafebabe"),
        },
        Policy::Refuses,
    );
}
