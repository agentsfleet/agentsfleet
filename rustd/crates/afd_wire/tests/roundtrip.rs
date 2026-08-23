//! Byte-for-byte round-trip against the fixtures the Zig emitter produced.
//!
//! Each fixture is parsed into its Rust type and re-serialized, and the OUTPUT
//! BYTES are compared to the input bytes. That is a stronger claim than
//! field-by-field equality: it also pins field ORDER, optional-emission policy,
//! number spelling and enum spelling — every way two JSON encoders can agree on
//! a value while disagreeing on its encoding.
//!
//! The direction is one-way on purpose. Zig writes the fixtures; Rust conforms.
//! A disagreement is fixed by changing Rust or regenerating from Zig, never by
//! editing a fixture.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: a missing or malformed fixture is an unmet \
              precondition, and failing loudly on it is the correct outcome"
)]

use std::path::PathBuf;

use afd_wire::{activity, admin, credentials, event, lease, memory, policy, report, runner};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join("samples/fixtures/wire-v2")
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(format!("{name}.json"));
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e} — run `make wire-fixtures`",
            path.display()
        )
    })
}

/// Compares bytes and, on a mismatch, names the first differing offset with
/// context. A bare `assert_eq!` on two long JSON documents prints two walls of
/// text and leaves the reader to diff them by eye.
fn assert_bytes_identical(name: &str, expected: &[u8], actual: &[u8]) {
    if expected == actual {
        return;
    }
    let at = expected
        .iter()
        .zip(actual)
        .position(|(a, b)| a != b)
        .unwrap_or(expected.len().min(actual.len()));
    let window = |bytes: &[u8]| {
        let start = at.saturating_sub(40);
        let end = (at + 40).min(bytes.len());
        String::from_utf8_lossy(&bytes[start..end]).into_owned()
    };
    panic!(
        "{name}: re-serialized bytes differ from the fixture at offset {at}\n  \
         zig  ...{}...\n  rust ...{}...\n  (lengths {} vs {})",
        window(expected),
        window(actual),
        expected.len(),
        actual.len()
    );
}

/// Declares the fixture roster and generates one test per entry.
///
/// A test per fixture rather than a loop over a table: the failing type is then
/// the failing test's NAME, and the borrow of the parsed value stays local to
/// the fixture bytes it borrows from — which a table of function pointers
/// cannot express, since each type's lifetime depends on its own input.
macro_rules! wire_roundtrip {
    ($($case:ident / $unknown:ident => $ty:ty : $name:literal),+ $(,)?) => {
        /// Every fixture this suite claims to cover.
        const ROSTER: &[&str] = &[$($name),+];

        $(
            #[test]
            fn $case() {
                let bytes = read_fixture($name);
                let value: $ty = serde_json::from_slice(&bytes)
                    .unwrap_or_else(|e| panic!("{}: parse failed: {e}", $name));
                let actual = serde_json::to_vec(&value).unwrap();
                assert_bytes_identical($name, &bytes, &actual);
            }

            #[test]
            fn $unknown() {
                let Some(probed) = with_unknown_field($name) else {
                    return; // an enum fixture is an array; nothing to inject into
                };
                let accepted = serde_json::from_slice::<$ty>(&probed).is_ok();
                assert_policy_holds($name, accepted);
            }
        )+
    };
}

wire_roundtrip! {
activity_activityframe / activity_activityframe_unknown_field => activity::ActivityFrame<'_> : "activity.ActivityFrame",
activity_activityrequest / activity_activityrequest_unknown_field => activity::ActivityRequest<'_> : "activity.ActivityRequest",
event_envelope_eventtype / event_envelope_eventtype_unknown_field => Vec<event::EventType> : "event_envelope.EventType",
execution_policy_contextbudget / execution_policy_contextbudget_unknown_field => policy::ContextBudget<'_> : "execution_policy.ContextBudget",
execution_policy_executionpolicy / execution_policy_executionpolicy_unknown_field => policy::ExecutionPolicy<'_> : "execution_policy.ExecutionPolicy",
execution_policy_httpjsonfieldrule / execution_policy_httpjsonfieldrule_unknown_field => policy::HttpJsonFieldRule<'_> : "execution_policy.HttpJsonFieldRule",
execution_policy_httpmethod / execution_policy_httpmethod_unknown_field => Vec<policy::HttpMethod> : "execution_policy.HttpMethod",
execution_policy_httporiginpolicy / execution_policy_httporiginpolicy_unknown_field => policy::HttpOriginPolicy<'_> : "execution_policy.HttpOriginPolicy",
execution_policy_httppathmatch / execution_policy_httppathmatch_unknown_field => Vec<policy::HttpPathMatch> : "execution_policy.HttpPathMatch",
execution_policy_httprequestrule / execution_policy_httprequestrule_unknown_field => policy::HttpRequestRule<'_> : "execution_policy.HttpRequestRule",
execution_policy_mintable / execution_policy_mintable_unknown_field => policy::Mintable<'_> : "execution_policy.Mintable",
execution_policy_networkpolicy / execution_policy_networkpolicy_unknown_field => policy::NetworkPolicy<'_> : "execution_policy.NetworkPolicy",
execution_policy_repositoryaccess / execution_policy_repositoryaccess_unknown_field => Vec<policy::RepositoryAccess> : "execution_policy.RepositoryAccess",
execution_policy_repositorybinding / execution_policy_repositorybinding_unknown_field => policy::RepositoryBinding<'_> : "execution_policy.RepositoryBinding",
execution_result_executionresult / execution_result_executionresult_unknown_field => report::ExecutionResult<'_> : "execution_result.ExecutionResult",
execution_result_failureclass / execution_result_failureclass_unknown_field => Vec<report::FailureClass> : "execution_result.FailureClass",
protocol_adminstate / protocol_adminstate_unknown_field => Vec<admin::AdminState> : "protocol.AdminState",
protocol_assignedpolicy / protocol_assignedpolicy_unknown_field => runner::AssignedPolicy<'_> : "protocol.AssignedPolicy",
protocol_bindmode / protocol_bindmode_unknown_field => Vec<runner::BindMode> : "protocol.BindMode",
protocol_bundlemanifest / protocol_bundlemanifest_unknown_field => lease::BundleManifest<'_> : "protocol.BundleManifest",
protocol_capabilityreport / protocol_capabilityreport_unknown_field => runner::CapabilityReport<'_> : "protocol.CapabilityReport",
protocol_extrabind / protocol_extrabind_unknown_field => runner::ExtraBind<'_> : "protocol.ExtraBind",
protocol_heartbeatrequest / protocol_heartbeatrequest_unknown_field => runner::HeartbeatRequest<'_> : "protocol.HeartbeatRequest",
protocol_heartbeatresponse / protocol_heartbeatresponse_unknown_field => runner::HeartbeatResponse<'_> : "protocol.HeartbeatResponse",
protocol_heartbeatstatus / protocol_heartbeatstatus_unknown_field => Vec<runner::HeartbeatStatus> : "protocol.HeartbeatStatus",
protocol_leasepayload / protocol_leasepayload_unknown_field => lease::LeasePayload<'_> : "protocol.LeasePayload",
protocol_leaserequest / protocol_leaserequest_unknown_field => lease::LeaseRequest : "protocol.LeaseRequest",
protocol_leaseresponse / protocol_leaseresponse_unknown_field => lease::LeaseResponse<'_> : "protocol.LeaseResponse",
protocol_memorydelta / protocol_memorydelta_unknown_field => memory::MemoryDelta<'_> : "protocol.MemoryDelta",
protocol_memoryhydrateresponse / protocol_memoryhydrateresponse_unknown_field => memory::MemoryHydrateResponse<'_> : "protocol.MemoryHydrateResponse",
protocol_memorypushrequest / protocol_memorypushrequest_unknown_field => memory::MemoryPushRequest<'_> : "protocol.MemoryPushRequest",
protocol_mintcredentialrequest / protocol_mintcredentialrequest_unknown_field => credentials::MintCredentialRequest<'_> : "protocol.MintCredentialRequest",
protocol_mintcredentialresponse / protocol_mintcredentialresponse_unknown_field => credentials::MintCredentialResponse<'_> : "protocol.MintCredentialResponse",
protocol_networkpolicy / protocol_networkpolicy_unknown_field => Vec<runner::NetworkPolicy> : "protocol.NetworkPolicy",
protocol_outcome / protocol_outcome_unknown_field => Vec<report::Outcome> : "protocol.Outcome",
protocol_registerrequest / protocol_registerrequest_unknown_field => runner::RegisterRequest<'_> : "protocol.RegisterRequest",
protocol_registerresponse / protocol_registerresponse_unknown_field => runner::RegisterResponse<'_> : "protocol.RegisterResponse",
protocol_renewrequest / protocol_renewrequest_unknown_field => report::RenewRequest : "protocol.RenewRequest",
protocol_renewresponse / protocol_renewresponse_unknown_field => report::RenewResponse : "protocol.RenewResponse",
protocol_reportcheckpoint / protocol_reportcheckpoint_unknown_field => report::ReportCheckpoint<'_> : "protocol.ReportCheckpoint",
protocol_reportrequest / protocol_reportrequest_unknown_field => report::ReportRequest<'_> : "protocol.ReportRequest",
protocol_reportresponse / protocol_reportresponse_unknown_field => report::ReportResponse : "protocol.ReportResponse",
protocol_reporttelemetry / protocol_reporttelemetry_unknown_field => report::ReportTelemetry : "protocol.ReportTelemetry",
protocol_runneradminaction / protocol_runneradminaction_unknown_field => Vec<admin::RunnerAdminAction> : "protocol.RunnerAdminAction",
protocol_runneradminpatchrequest / protocol_runneradminpatchrequest_unknown_field => admin::RunnerAdminPatchRequest<'_> : "protocol.RunnerAdminPatchRequest",
protocol_runneradminpatchresponse / protocol_runneradminpatchresponse_unknown_field => admin::RunnerAdminPatchResponse<'_> : "protocol.RunnerAdminPatchResponse",
protocol_runnerchildinput / protocol_runnerchildinput_unknown_field => lease::RunnerChildInput<'_> : "protocol.RunnerChildInput",
protocol_runnereventitem / protocol_runnereventitem_unknown_field => admin::RunnerEventItem<'_> : "protocol.RunnerEventItem",
protocol_runnereventtype / protocol_runnereventtype_unknown_field => Vec<admin::RunnerEventType> : "protocol.RunnerEventType",
protocol_runnereventsresponse / protocol_runnereventsresponse_unknown_field => admin::RunnerEventsResponse<'_> : "protocol.RunnerEventsResponse",
protocol_runnerliveness / protocol_runnerliveness_unknown_field => Vec<runner::RunnerLiveness> : "protocol.RunnerLiveness",
protocol_sandboxtier / protocol_sandboxtier_unknown_field => Vec<runner::SandboxTier> : "protocol.SandboxTier",
protocol_secretdelivery / protocol_secretdelivery_unknown_field => Vec<lease::SecretDelivery> : "protocol.SecretDelivery",
protocol_selfresponse / protocol_selfresponse_unknown_field => runner::SelfResponse<'_> : "protocol.SelfResponse",
protocol_selftestcheck / protocol_selftestcheck_unknown_field => runner::SelftestCheck<'_> : "protocol.SelftestCheck",
protocol_selftestreport / protocol_selftestreport_unknown_field => runner::SelftestReport<'_> : "protocol.SelftestReport",}

/// Adds a field the type cannot know about, or `None` when the fixture is an
/// array (an enum vocabulary, which has no object to inject into).
fn with_unknown_field(name: &str) -> Option<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_slice(&read_fixture(name)).unwrap();
    let mut object = value.as_object()?.clone();
    object.insert(
        "field_a_future_daemon_added".to_owned(),
        serde_json::Value::from(1),
    );
    Some(serde_json::to_vec(&serde_json::Value::Object(object)).unwrap())
}

/// Asserts the observed leniency matches what the emitter recorded for the type.
///
/// The policy is per call site in the Zig daemon, not global — some parsers pass
/// `ignore_unknown_fields`, most do not — so neither a blanket
/// `deny_unknown_fields` nor serde's permissive default is right. The manifest
/// is what makes "mirrors the Zig parser" checkable instead of asserted.
fn assert_policy_holds(name: &str, accepted: bool) {
    let manifest = manifest();
    let declared = manifest
        .types
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("{name} is not in the manifest"));
    match declared.unknown_fields.as_str() {
        "ignore" => assert!(
            accepted,
            "{name}: the Zig parser ignores unknown fields here, but this type rejected one"
        ),
        "reject" => assert!(
            !accepted,
            "{name}: the Zig parser rejects unknown fields here, but this type accepted one \
             (serde ignores them unless the type carries `deny_unknown_fields`)"
        ),
        other => panic!("{name}: unrecognized unknown-field policy {other:?}"),
    }
}

/// The manifest the emitter wrote beside the fixtures.
#[derive(serde::Deserialize)]
struct Manifest {
    wire_version: u16,
    types: Vec<ManifestType>,
    excluded: Vec<Exclusion>,
}

#[derive(serde::Deserialize)]
struct ManifestType {
    name: String,
    file: String,
    unknown_fields: String,
}

#[derive(serde::Deserialize)]
struct Exclusion {
    module: String,
    why: String,
}

fn manifest() -> Manifest {
    let bytes = read_fixture("manifest");
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("manifest is unreadable: {e}"))
}

/// Catches a wire type the port never learned about, a fixture nothing parses,
/// and — the reason this test is worth its own rubric row — the version-one
/// lease being re-admitted through the emitter's type enumeration.
#[test]
fn test_fixture_set_complete() {
    let manifest = manifest();

    let declared: std::collections::BTreeSet<&str> =
        manifest.types.iter().map(|t| t.name.as_str()).collect();
    let covered: std::collections::BTreeSet<&str> = ROSTER.iter().copied().collect();

    let unported: Vec<&&str> = declared.difference(&covered).collect();
    assert!(
        unported.is_empty(),
        "the Zig emitter exports wire types this port does not round-trip: {unported:?}"
    );
    let phantom: Vec<&&str> = covered.difference(&declared).collect();
    assert!(
        phantom.is_empty(),
        "this suite claims fixtures the emitter does not produce: {phantom:?}"
    );

    // Every declared fixture is on disk, and nothing else is.
    let on_disk: std::collections::BTreeSet<String> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            std::path::Path::new(n)
                .extension()
                .is_some_and(|e| e == "json")
        })
        .filter(|n| n != "manifest.json")
        .collect();
    let expected: std::collections::BTreeSet<String> =
        manifest.types.iter().map(|t| t.file.clone()).collect();
    assert_eq!(
        on_disk, expected,
        "the fixture directory and the manifest disagree — regenerate with `make wire-fixtures`"
    );

    // The exclusion is asserted, not assumed. A superseded shape re-admitted to
    // the emitter's enumeration would otherwise arrive here as a new fixture and
    // grow a serde type to match it — compatibility through the back door.
    let superseded: Vec<&str> = manifest
        .excluded
        .iter()
        .filter(|e| e.why == "superseded")
        .map(|e| e.module.as_str())
        .collect();
    assert_eq!(
        superseded,
        vec!["protocol_lease_v1"],
        "the superseded-shape exclusion list changed; the port implements the current shape only"
    );
}

/// Catches the two constants drifting apart. Not tautological: the left side is
/// a number the ZIG emitter wrote into the manifest, the right side is this
/// crate's own constant, and nothing else makes them agree.
#[test]
fn test_wire_version_matches_fixture() {
    assert_eq!(
        manifest().wire_version,
        afd_wire::paths::LEASE_WIRE_VERSION_CURRENT,
        "the Zig wire version and this port's constant disagree"
    );
}
