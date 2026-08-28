//! Operator-plane runner list, detail, and history reads.

mod decode;
mod events;
mod store;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::admin::RunnerEventItem;
use afd_wire::runner::{AssignedPolicy, CapabilityReport, RunnerLiveness, SelftestReport};
use sqlx::Row as _;

use crate::error::{Result, query, runner_not_found};
use crate::sql;
use crate::store::Runners;

use self::decode::{runner_detail, runner_event, runner_item};
pub use self::events::RunnerEventFilter;

/// The runner page size used when a caller omits `limit`.
pub const DEFAULT_PAGE_LIMIT: u32 = 50;
/// The largest runner page the public API accepts.
pub const MAX_PAGE_LIMIT: u32 = 100;

/// A page size already proven to be inside the public API bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimit(u32);

impl PageLimit {
    /// Builds a limit in the inclusive range 1 through 100.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 || value > MAX_PAGE_LIMIT {
            None
        } else {
            Some(Self(value))
        }
    }

    /// The checked value for response sizing.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn as_i64(self) -> i64 {
        i64::from(self.0)
    }
}

impl Default for PageLimit {
    fn default() -> Self {
        Self(DEFAULT_PAGE_LIMIT)
    }
}

/// The final composite key from a page, used to seek the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysetCursor {
    created_at: i64,
    id: Uuid7,
}

impl KeysetCursor {
    /// A cursor from a row the service already decoded.
    #[must_use]
    pub const fn new(created_at: i64, id: Uuid7) -> Self {
        Self { created_at, id }
    }

    /// The timestamp half of the database boundary.
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// The identifier half of the database boundary.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.id
    }
}

/// One operator list row. Authentication material is unrepresentable here.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerItem {
    /// Canonical runner identifier.
    id: Uuid7,
    /// Stable host identity supplied at enrolment.
    host_id: String,
    /// Assigned isolation tier spelling.
    sandbox_tier: String,
    /// Operator-controlled admission state.
    admin_state: afd_wire::admin::AdminState,
    /// Runtime state derived from heartbeat and lease rows.
    liveness: RunnerLiveness,
    /// Placement labels assigned at enrolment.
    labels: Vec<String>,
    /// Last heartbeat instant in epoch milliseconds.
    last_seen_at: i64,
    /// Enrolment instant in epoch milliseconds.
    created_at: i64,
    /// Policy currently assigned to the host.
    assigned_policy: Option<AssignedPolicy<'static>>,
    /// Capability report most recently supplied by the host.
    achievable: Option<CapabilityReport<'static>>,
    /// Whether the assigned policy exceeds the reported capability.
    degraded: bool,
    /// Stored explanation for a degraded verdict.
    degraded_reason: Option<String>,
}

impl RunnerItem {
    /// Canonical runner identifier.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.id
    }

    /// Stable host identity supplied at enrolment.
    #[must_use]
    pub fn host_id(&self) -> &str {
        &self.host_id
    }

    /// Assigned isolation tier spelling.
    #[must_use]
    pub fn sandbox_tier(&self) -> &str {
        &self.sandbox_tier
    }

    /// Operator-controlled admission state.
    #[must_use]
    pub const fn admin_state(&self) -> afd_wire::admin::AdminState {
        self.admin_state
    }

    /// Runtime state derived from heartbeat and lease rows.
    #[must_use]
    pub const fn liveness(&self) -> RunnerLiveness {
        self.liveness
    }

    /// Placement labels assigned at enrolment.
    #[must_use]
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Last heartbeat instant in epoch milliseconds.
    #[must_use]
    pub const fn last_seen_at(&self) -> i64 {
        self.last_seen_at
    }

    /// Enrolment instant in epoch milliseconds.
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// Policy currently assigned to the host.
    #[must_use]
    pub const fn assigned_policy(&self) -> Option<&AssignedPolicy<'static>> {
        self.assigned_policy.as_ref()
    }

    /// Capability report most recently supplied by the host.
    #[must_use]
    pub const fn achievable(&self) -> Option<&CapabilityReport<'static>> {
        self.achievable.as_ref()
    }

    /// Whether the assigned policy exceeds the reported capability.
    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        self.degraded
    }

    /// Stored explanation for a degraded verdict.
    #[must_use]
    pub fn degraded_reason(&self) -> Option<&str> {
        self.degraded_reason.as_deref()
    }
}

/// A keyset page of runners.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerPage {
    /// Rows in newest-first keyset order.
    items: Vec<RunnerItem>,
    /// Total runners independent of this page boundary.
    total: i64,
    /// Boundary for the next page, absent when this page is short.
    next_cursor: Option<KeysetCursor>,
}

impl RunnerPage {
    /// Rows in newest-first keyset order.
    #[must_use]
    pub fn items(&self) -> &[RunnerItem] {
        &self.items
    }

    /// Consumes the page and returns its rows.
    #[must_use]
    pub fn into_items(self) -> Vec<RunnerItem> {
        self.items
    }

    /// Total runners independent of this page boundary.
    #[must_use]
    pub const fn total(&self) -> i64 {
        self.total
    }

    /// Boundary for the next page.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<&KeysetCursor> {
        self.next_cursor.as_ref()
    }
}

/// The single-runner read with live and lifetime counters.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerDetail {
    /// The same runner shape the list returns.
    item: RunnerItem,
    /// Live leases currently held by this runner.
    active_lease_count: i64,
    /// Distinct fleets represented by those live leases.
    active_fleet_count: i64,
    /// Leases acquired over the runner's lifetime.
    leases_acquired: i64,
    /// Leases settled successfully over the runner's lifetime.
    leases_succeeded: i64,
    /// Leases settled unsuccessfully over the runner's lifetime.
    leases_failed: i64,
    /// Leases expired over the runner's lifetime.
    leases_expired: i64,
    /// Outstanding self-test request instant.
    selftest_requested_at: Option<i64>,
    /// Most recent self-test completion instant.
    selftest_completed_at: Option<i64>,
    /// Most recent complete self-test report.
    selftest: Option<SelftestReport<'static>>,
}

impl RunnerDetail {
    /// The same runner shape the list returns.
    #[must_use]
    pub const fn item(&self) -> &RunnerItem {
        &self.item
    }

    /// Live leases currently held by this runner.
    #[must_use]
    pub const fn active_lease_count(&self) -> i64 {
        self.active_lease_count
    }

    /// Distinct fleets represented by those live leases.
    #[must_use]
    pub const fn active_fleet_count(&self) -> i64 {
        self.active_fleet_count
    }

    /// Leases acquired over the runner's lifetime.
    #[must_use]
    pub const fn leases_acquired(&self) -> i64 {
        self.leases_acquired
    }

    /// Leases settled successfully over the runner's lifetime.
    #[must_use]
    pub const fn leases_succeeded(&self) -> i64 {
        self.leases_succeeded
    }

    /// Leases settled unsuccessfully over the runner's lifetime.
    #[must_use]
    pub const fn leases_failed(&self) -> i64 {
        self.leases_failed
    }

    /// Leases expired over the runner's lifetime.
    #[must_use]
    pub const fn leases_expired(&self) -> i64 {
        self.leases_expired
    }

    /// Outstanding self-test request instant.
    #[must_use]
    pub const fn selftest_requested_at(&self) -> Option<i64> {
        self.selftest_requested_at
    }

    /// Most recent self-test completion instant.
    #[must_use]
    pub const fn selftest_completed_at(&self) -> Option<i64> {
        self.selftest_completed_at
    }

    /// Most recent complete self-test report.
    #[must_use]
    pub const fn selftest(&self) -> Option<&SelftestReport<'static>> {
        self.selftest.as_ref()
    }
}

/// A keyset page of append-only runner history.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerEventPage {
    /// History rows in newest-first keyset order.
    items: Vec<RunnerEventItem<'static>>,
    /// Total history rows for this runner.
    total: i64,
    /// Boundary for the next page, absent when this page is short.
    next_cursor: Option<KeysetCursor>,
}

impl RunnerEventPage {
    /// History rows in newest-first keyset order.
    #[must_use]
    pub fn items(&self) -> &[RunnerEventItem<'static>] {
        &self.items
    }

    /// Consumes the page and returns its history rows.
    #[must_use]
    pub fn into_items(self) -> Vec<RunnerEventItem<'static>> {
        self.items
    }

    /// Total history rows for this runner.
    #[must_use]
    pub const fn total(&self) -> i64 {
        self.total
    }

    /// Boundary for the next page.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<&KeysetCursor> {
        self.next_cursor.as_ref()
    }
}

#[cfg(test)]
mod tests;
