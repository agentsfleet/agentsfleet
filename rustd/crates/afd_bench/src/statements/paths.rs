//! Which part of a chat each statement belongs to (`M241_001` §4).
//!
//! The product's statement constants are private to their crates, so the
//! bench cannot ask which file a `pg_stat_statements` row came from. It
//! matches instead: each entry below is a fragment distinctive enough that
//! only one product statement's normalized text contains it. The first
//! matching entry decides, so a statement two paths share counts once, in the
//! earlier path. A statement no entry matches is counted under
//! [`Path::Other`], never dropped, so the paths always sum to the window's
//! total and a new product statement shows up as a rising `other`.

use std::collections::BTreeMap;

use super::queries::QueryCost;

/// One part of a chat's cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Path {
    /// Finding and claiming the fleet's head admission.
    Claim,
    /// Admission and renewal gates: policy, money, approvals, grants and the
    /// provider and key a lease is delivered with.
    Gates,
    /// The model price catalogue, read by the estimate, every renewal and the
    /// report alike.
    Pricing,
    /// Building and writing the lease: config, memory, secret, lease row.
    Issue,
    /// The host row's capacity lock and census, shared by issue, renewal and
    /// report, so it is its own path rather than the first caller's.
    Host,
    /// Metering a lease's slice: the renewal's and the report's shared
    /// statement, its usage lines and the lease read before it.
    Meter,
    /// Authorizing a live-tail post.
    Activity,
    /// Settling the run: result, slot release, obligation.
    Report,
    /// Statements every path sends: tenant context and foreign-key checks.
    Shared,
    /// Maintenance a sweeper ran inside the window.
    Sweep,
    /// Anything no fragment matches.
    Other,
}

impl Path {
    /// Every path, in report order.
    pub const ALL: [Self; 11] = [
        Self::Claim,
        Self::Gates,
        Self::Pricing,
        Self::Issue,
        Self::Host,
        Self::Meter,
        Self::Activity,
        Self::Report,
        Self::Shared,
        Self::Sweep,
        Self::Other,
    ];

    /// The suffix a result key carries for this path.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Gates => "gates",
            Self::Pricing => "pricing",
            Self::Issue => "issue",
            Self::Host => "host",
            Self::Meter => "meter",
            Self::Activity => "activity",
            Self::Report => "report",
            Self::Shared => "shared",
            Self::Sweep => "sweep",
            Self::Other => "other",
        }
    }
}

/// Fragments of normalized statement text, first match wins.
///
/// Read off a `bench-lease` drain on `main` `14539a73e` (`M241_001` §4): each
/// fragment is the shortest text that names one product statement. When a
/// statement changes, its row lands in `other` until its fragment follows.
const FRAGMENTS: &[(&str, Path)] = &[
    // Claim: the candidate scan, the slot-and-head claim, the received event
    // and the delivery stamp.
    ("eligible AS MATERIALIZED", Path::Claim),
    ("FOR UPDATE OF s SKIP LOCKED", Path::Claim),
    ("INSERT INTO fleet.events (fleet_id, event_id", Path::Claim),
    ("UPDATE fleet.admissions SET delivered_at", Path::Claim),
    // Gates.
    ("SELECT r.policy_semantics FROM fleet.fleets", Path::Gates),
    ("SELECT w.balance_nanos, w.exhausted_at", Path::Gates),
    ("WITH floors AS", Path::Gates),
    (
        "SELECT tenant_id::text FROM identity.workspaces WHERE id",
        Path::Gates,
    ),
    ("SELECT action_id FROM fleet.approval_gates", Path::Gates),
    (
        "SELECT service FROM connector.integration_grants",
        Path::Gates,
    ),
    ("SELECT p.provider, m.model_name AS model", Path::Gates),
    (
        "FROM model.platform_defaults p JOIN vault.secrets",
        Path::Gates,
    ),
    // Pricing.
    ("SELECT p.id::text AS id, p.effective_from", Path::Pricing),
    (
        "SELECT resource_version FROM model.catalogue_version",
        Path::Pricing,
    ),
    // Issue.
    ("r.definition::text AS definition, f.status", Path::Issue),
    ("SELECT r.definition::text FROM fleet.fleets", Path::Issue),
    (
        "memory_reads_workspace, memory_publishes_workspace FROM fleet.fleets",
        Path::Issue,
    ),
    ("FROM memory.entries WHERE fleet_id", Path::Issue),
    ("SELECT dek_ciphertext, dek_nonce", Path::Issue),
    ("WITH held AS ( UPDATE runner.fleet_slots", Path::Issue),
    ("INSERT INTO runner.events (id, runner_id", Path::Issue),
    // Host: the capacity lock, its census and the lease-type record.
    ("SELECT worker_count, status, degraded_reason", Path::Host),
    (
        "SELECT instant.checked_at, (SELECT count(*) FROM runner.leases",
        Path::Host,
    ),
    ("UPDATE runner.runners SET lease_version", Path::Host),
    // Meter.
    (
        "s.fencing_token, s.lease_expires_at FROM runner.fleet_slots s JOIN runner.leases",
        Path::Meter,
    ),
    ("INSERT INTO billing.usage_lines", Path::Meter),
    (
        "posture, provider, model, status FROM runner.leases WHERE id",
        Path::Meter,
    ),
    // Activity: authorizing one live-tail post.
    (
        "SELECT l.fleet_id::text AS fleet_id, e.event_id, l.created_at, l.event_created_at",
        Path::Activity,
    ),
    // Report.
    (
        "UPDATE runner.fleet_slots SET lease_expires_at = clock_timestamp() - interval",
        Path::Report,
    ),
    (
        "DELETE FROM fleet.tool_calls t USING fleet.events",
        Path::Report,
    ),
    (
        "INSERT INTO fleet.sessions (fleet_id, context_json",
        Path::Report,
    ),
    (
        "l.tenant_id::text AS tenant_id, e.event_id, e.actor",
        Path::Report,
    ),
    ("WITH slot AS MATERIALIZED", Path::Report),
    (
        "SELECT COALESCE((SELECT k.event_count FROM fleet.activity_counters",
        Path::Report,
    ),
    ("WITH closed AS ( UPDATE fleet.events", Path::Report),
    (
        "actor, event_type, status, token_count, wall_ms",
        Path::Report,
    ),
    ("SELECT COALESCE(sum(input_token_count)", Path::Report),
    (
        "SELECT reply_provider, reply_address FROM fleet.admissions",
        Path::Report,
    ),
    // Shared.
    ("SELECT set_config(", Path::Shared),
    ("FOR KEY SHARE OF x", Path::Shared),
    ("INSERT INTO fleet.activity_counters", Path::Shared),
    // Sweep: the backlog sampler.
    ("SELECT count(*) AS waiting", Path::Sweep),
];

/// The path one statement's text belongs to.
#[must_use]
pub fn classify(query: &str) -> Path {
    classify_with(FRAGMENTS, query)
}

/// [`classify`] over a given table, so the matching rule is testable apart
/// from the table's contents.
#[must_use]
pub fn classify_with(fragments: &[(&str, Path)], query: &str) -> Path {
    let normalized = query.split_whitespace().collect::<Vec<_>>().join(" ");
    fragments
        .iter()
        .find(|(fragment, _)| normalized.contains(fragment))
        .map_or(Path::Other, |(_, path)| *path)
}

/// Calls per path, every path present, using the given table.
#[must_use]
pub fn by_path_with(fragments: &[(&str, Path)], costs: &[QueryCost]) -> BTreeMap<Path, u64> {
    let mut totals: BTreeMap<Path, u64> = Path::ALL.iter().map(|path| (*path, 0)).collect();
    for cost in costs {
        let path = classify_with(fragments, &cost.query);
        *totals.entry(path).or_default() += cost.calls;
    }
    totals
}

/// Calls per path with the product table.
#[must_use]
pub fn by_path(costs: &[QueryCost]) -> BTreeMap<Path, u64> {
    by_path_with(FRAGMENTS, costs)
}

/// Each path's calls per lease, or nothing when no lease was issued: a ratio
/// over zero leases describes nothing.
#[must_use]
pub fn per_lease(totals: &BTreeMap<Path, u64>, leases: u64) -> Option<BTreeMap<Path, f64>> {
    if leases == 0 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "statement counts in one window stay far below 2^52"
    )]
    let per = totals
        .iter()
        .map(|(path, calls)| (*path, *calls as f64 / leases as f64))
        .collect();
    Some(per)
}

/// The `n` statements with the most calls, most first.
#[must_use]
pub fn costliest(costs: &[QueryCost], n: usize) -> Vec<&QueryCost> {
    let mut sorted: Vec<&QueryCost> = costs.iter().collect();
    sorted.sort_by_key(|cost| core::cmp::Reverse(cost.calls));
    sorted.truncate(n);
    sorted
}
