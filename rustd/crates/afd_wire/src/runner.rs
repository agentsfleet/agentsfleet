//! What the control plane assigns to a runner, and what the host reports back.
//!
//! Configuration flows DOWN: the assignment rides the runner's identity on the
//! enrollment read and on every heartbeat reply, so a dashboard change reaches
//! the host within one beat and nobody visits the host. The capability report
//! and the self-test verdict flow UP, and are unauthenticated self-assertion —
//! a compromised host can lie, so placement trust stays operator-assigned.
//!
//! Every bound an enrolment, an assignment or a capability report must meet
//! is declared on its type with garde; the caps and the two grammar rules live
//! in [`rules`](self::rules), and a reader proves them by validating.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

mod rules;
mod selftest;

pub use self::rules::{
    BIND_NOTE_MAX_BYTES, BIND_PATH_MAX_BYTES, BIND_PATH_MIN_BYTES, CONTROLLER_NAME_MAX_BYTES,
    EXTRA_BINDS_MAX, HOST_ID_MAX_BYTES, LABEL_MAX_BYTES, LABELS_MAX, REGISTRY_ENTRIES_MAX,
    REGISTRY_ENTRY_MAX_BYTES, REGISTRY_PORT_MAX_DIGITS, REPORT_CONTROLLERS_MAX,
};
use self::rules::{bind_path, registry_entry};
pub use self::selftest::{
    CHECK_DETAIL_MAX_BYTES, CHECK_NAME_MAX_BYTES, SELFTEST_CHECKS_MAX, SELFTEST_POLICY_MAX_BYTES,
    SelftestCheck, SelftestReport,
};

/// The isolation strength assigned to a runner.
//
// Only tiers with real enforcement are members: a tier that cannot be applied
// must not be assignable.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxTier {
    /// Full kernel-enforced filesystem isolation.
    LandlockFull,
    /// Nested container isolation.
    ContainerNested,
    /// No isolation. Development only.
    DevNone,
}

/// Egress posture assigned per runner, named so the behaviour reads off the value.
// Published as `RunnerNetworkPolicy`, the name the hand-written contract gave
// it: `policy::NetworkPolicy` is a different shape with the same Rust name,
// and utoipa keys components by name alone, so the one registered second
// silently replaced the other in the document.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = RunnerNetworkPolicy))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// Everything outbound allowed, re-sharing the host network namespace.
    /// Opt-in only, never a fallback.
    AllowAll,
    /// No outbound traffic at all.
    DenyAllEgress,
    /// Outbound only to permitted destinations through the enforced boundary.
    AllowListEgress,
}

/// The posture an unset, missing or unrecognized policy resolves to.
///
/// Never `AllowAll`: a malformed policy must not silently open egress.
pub const FAIL_CLOSED_DEFAULT: NetworkPolicy = NetworkPolicy::AllowListEgress;

/// Whether an operator-added bind is writable.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindMode {
    /// Visible but not writable.
    ReadOnly,
    /// Visible and writable.
    ReadWrite,
}

/// One host path bound into every lease's sandbox, in addition to the baseline.
///
/// An operator may ADD a path a host needs; never remove or re-mode one the
/// sandbox depends on.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct ExtraBind<'a> {
    /// Host path to bind.
    #[serde(borrow)]
    #[garde(
        length(bytes, min = BIND_PATH_MIN_BYTES, max = BIND_PATH_MAX_BYTES),
        custom(bind_path)
    )]
    pub path: Cow<'a, str>,
    /// Whether the bind is writable.
    #[garde(skip)]
    pub mode: BindMode,
    /// Operator note explaining why the bind exists.
    #[serde(borrow)]
    #[garde(length(bytes, max = BIND_NOTE_MAX_BYTES))]
    pub note: Cow<'a, str>,
}

/// The isolation, egress and concurrency settings assigned to one runner.
//
// Everything a host was once told through its environment, now delivered with
// its identity. The host never declares policy.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct AssignedPolicy<'a> {
    /// Isolation strength to apply.
    #[garde(skip)]
    pub sandbox_tier: SandboxTier,
    /// Egress posture to apply.
    #[garde(skip)]
    pub network_policy: NetworkPolicy,
    /// Operator registry baseline merged into each lease's egress allowlist.
    /// Empty means the runner substitutes its own default registry set.
    #[serde(borrow)]
    #[garde(
        length(max = REGISTRY_ENTRIES_MAX),
        inner(
            length(bytes, min = 1, max = REGISTRY_ENTRY_MAX_BYTES),
            custom(registry_entry)
        )
    )]
    pub registry_allowlist: Vec<Cow<'a, str>>,
    /// Concurrent workers the runner may start. Clamped on both sides.
    // Clamped, never refused, so it carries no bound: `WorkerCount::clamping`.
    #[garde(skip)]
    pub worker_count: u32,
    /// Extra host paths bound into every lease's sandbox.
    #[serde(borrow)]
    #[garde(length(max = EXTRA_BINDS_MAX), dive)]
    pub extra_binds: Vec<ExtraBind<'a>>,
}

/// What this host can actually enforce.
//
// Probed at startup and refreshed per beat. Each field is one enforcement
// mechanism a degraded reason can name.
///
// The flags stay separate booleans rather than collapsing into a bitset or an
// enum: each names a distinct mechanism a degraded reason quotes back to an
// operator, and the shape is the peer's, not this crate's to choose.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[expect(
    clippy::struct_excessive_bools,
    reason = "wire shape fixed by the peer; each flag is a separately reported mechanism"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CapabilityReport<'a> {
    /// Filesystem isolation is available.
    #[garde(skip)]
    pub landlock: bool,
    /// System-call filtering is available.
    #[garde(skip)]
    pub seccomp: bool,
    /// Controllers present in the delegated cgroup's subtree control.
    // Stored and re-read by the runner page, so bounded: past either cap the
    // whole report reads as "nothing reported this beat".
    #[serde(borrow)]
    #[garde(
        length(max = REPORT_CONTROLLERS_MAX),
        inner(length(bytes, min = 1, max = CONTROLLER_NAME_MAX_BYTES))
    )]
    pub cgroup_controllers: Vec<Cow<'a, str>>,
    /// The sandbox launcher is available.
    #[garde(skip)]
    pub bubblewrap: bool,
    /// Kernel-enforced egress allowlisting is available.
    #[garde(skip)]
    pub egress_enforcement: bool,
}

/// Derived runtime liveness, computed by the fleet read and NEVER stored —
/// storing it would drift from the values it is derived from.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerLiveness {
    /// Minted, never connected.
    Registered,
    /// Holds a live lease. Takes precedence over `Offline`.
    Busy,
    /// Heartbeat fresh, no live lease.
    Online,
    /// Heartbeat stale beyond the lapse threshold.
    Offline,
}

/// Heartbeat reply status.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatStatus {
    /// Keep working.
    Ok,
    /// Finish current work, take no more.
    Drain,
    /// Stop now.
    Stop,
}

/// `POST /v1/runners` request. Authorized by an existing operator credential,
/// not an enrollment token. The operator ASSIGNS the policy; the host never
/// declares one.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest<'a> {
    /// Stable identifier for the host being enrolled.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = HOST_ID_MAX_BYTES))]
    pub host_id: Cow<'a, str>,
    /// The policy the operator assigns to it.
    #[serde(borrow)]
    #[garde(dive)]
    pub assigned_policy: AssignedPolicy<'a>,
    /// Operator labels for placement and filtering.
    #[serde(borrow)]
    #[garde(length(max = LABELS_MAX), inner(length(bytes, max = LABEL_MAX_BYTES)))]
    pub labels: Vec<Cow<'a, str>>,
}

/// `POST /v1/runners` reply: the durable identity plus its bearer token.
///
/// The token is returned ONCE — the daemon stores only its hash.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterResponse<'a> {
    /// The runner's durable identifier.
    #[serde(borrow)]
    pub runner_id: Cow<'a, str>,
    /// The minted bearer token. Secret — revealed once, never re-readable.
    #[serde(borrow)]
    pub runner_token: Cow<'a, str>,
    /// The assignment AS STORED, with the worker count clamped, so the
    /// enrolling operator sees exactly what the host will apply.
    #[serde(borrow)]
    pub assigned_policy: AssignedPolicy<'a>,
}

/// `POST /v1/runners/me/heartbeats` request.
///
/// The capability report rides the first beat and any beat where the probe
/// result changed. Both fields default to absent so an older runner's empty body
/// still parses.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatRequest<'a> {
    /// What this host can enforce, when the probe result is being reported.
    #[serde(borrow)]
    pub capability_report: Option<CapabilityReport<'a>>,
    /// A verdict produced since the last beat, by request or by startup probe.
    #[serde(borrow)]
    pub selftest: Option<SelftestReport<'a>>,
}

/// `POST /v1/runners/me/heartbeats` reply.
///
/// Carries the current assignment on EVERY beat, so a dashboard change reaches
/// the host within one interval. A null assignment means a row predating the
/// policy columns: the runner then fails closed and refuses to lease.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatResponse<'a> {
    /// Whether to keep working, drain, or stop.
    pub status: HeartbeatStatus,
    /// The runner's current assignment.
    #[serde(borrow)]
    pub assigned_policy: Option<AssignedPolicy<'a>>,
    /// Whether the row reads degraded.
    pub degraded: bool,
    /// Why it reads degraded.
    #[serde(borrow)]
    pub degraded_reason: Option<Cow<'a, str>>,
    /// An operator asked this runner to self-test. Rides the beat like the
    /// assignment does — one interval, no second endpoint, no host visit.
    pub selftest_requested: bool,
    /// How long the runner waits before its next beat, in milliseconds.
    ///
    /// Required rather than optional. The daemon owns
    /// [`afd_core::timing::RUNNER_OFFLINE_AFTER_MS`], so it owns the cadence
    /// that keeps a host ahead of it. A runner carrying its own copy could
    /// not be told when that threshold moved. The assertion beside those
    /// constants is what keeps the served value under the threshold.
    pub heartbeat_interval_ms: u32,
}

/// `GET /v1/runners/me` reply — the runner's own registration row, read-only.
///
/// Reading this does NOT bump liveness, so inspecting a host can never mask a
/// dead runner.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfResponse<'a> {
    /// The runner's identifier.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Operator-facing state.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// The host it runs on.
    #[serde(borrow)]
    pub host_id: Cow<'a, str>,
    /// The tier it was assigned.
    #[serde(borrow)]
    pub sandbox_tier: Cow<'a, str>,
    /// Epoch milliseconds of the last beat; zero when never seen.
    pub last_seen_at: i64,
    /// The policy currently assigned to it.
    #[serde(borrow)]
    pub assigned_policy: Option<AssignedPolicy<'a>>,
    /// What the host reported it can actually enforce.
    #[serde(borrow)]
    pub achievable: Option<CapabilityReport<'a>>,
    /// Whether the row reads degraded.
    pub degraded: bool,
    /// Why it reads degraded.
    #[serde(borrow)]
    pub degraded_reason: Option<Cow<'a, str>>,
}
