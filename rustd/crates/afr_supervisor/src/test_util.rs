//! A healthy daemon, as this crate's own suites fake it.
//!
//! A suite outside the crate fakes the same one from here (`M-TEST-UTIL`):
//! the fixture lease, the identifiers it carries, and the reply a healthy
//! daemon gives each route.

use std::borrow::Cow;

use afd_wire::lease::LeaseResponse;
use afd_wire::memory::{MemoryCaptureResponse, MemoryHydrateResponse, MemoryRecallResponse};
use afd_wire::paths::{
    LEASE_RENEW_SUFFIX, LEASE_TOOL_CALLS_SUFFIX, RUNNER_HEARTBEATS, RUNNER_LEASES, RUNNER_MEMORY,
    RUNNER_MEMORY_RECALL_SUFFIX, RUNNER_SELF,
};
use afd_wire::report::{RenewResponse, ReportResponse};
use afd_wire::runner::{AssignedPolicy, HeartbeatResponse, HeartbeatStatus, SelfResponse};
use afd_wire::tool_detail::ToolCallRecordsStored;
use serde::Serialize;
use serde_json::Value;

use crate::client::Verb;

/// The lease every suite is granted, with its identifiers left for the suite
/// to fill.
pub const LEASE_JSON: &str = include_str!("test_support/lease.json");
/// A canonical lease identifier.
pub const LEASE_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
/// A canonical fleet identifier.
pub const FLEET_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
/// The fencing token the fixture lease carries.
pub const FENCING: u64 = 504;
/// The id a healthy daemon names this runner by.
pub const RUNNER_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8059";
/// The host a healthy daemon has this runner on.
pub const RUNNER_HOST: &str = "host-7";
/// The GET method, as a request line spells it.
const GET: &str = "GET";
/// The status a healthy daemon reports this runner in.
const ACTIVE: &str = "active";

/// What sets one healthy daemon apart from another: the posture it assigns,
/// the cadence it asks for, and when a renewal is granted until.
#[derive(Debug, Clone)]
pub struct Healthy {
    /// The assignment every beat carries.
    pub assigned: AssignedPolicy<'static>,
    /// How often to beat and to ask for a lease again, in milliseconds.
    pub interval_ms: u32,
    /// When a renewal is granted until, in Unix milliseconds.
    pub granted_until: i64,
}

impl Healthy {
    /// The reply to a `method` request for `path`: a route with no reply of
    /// its own is acknowledged.
    #[must_use]
    pub fn reply(&self, method: &str, path: &str) -> Value {
        self.to(routed(method, path))
    }

    /// The reply to `verb`, or the acknowledgement when it has none of its
    /// own.
    pub(crate) fn to(&self, verb: Option<Verb>) -> Value {
        match verb {
            Some(Verb::Heartbeat) => value(&HeartbeatResponse {
                status: HeartbeatStatus::Ok,
                assigned_policy: Some(self.assigned.clone()),
                degraded: false,
                degraded_reason: None,
                selftest_requested: false,
                release_holds: Vec::new(),
                heartbeat_interval_ms: self.interval_ms,
            }),
            Some(Verb::Lease) => value(&LeaseResponse {
                lease: None,
                retry_after_ms: Some(self.interval_ms),
            }),
            Some(Verb::Renew) => value(&RenewResponse {
                lease_expires_at: self.granted_until,
            }),
            Some(Verb::Hydrate) => value(&MemoryHydrateResponse {
                memory: Vec::new(),
                shared: Vec::new(),
                publish: false,
            }),
            Some(Verb::Capture) => value(&MemoryCaptureResponse {
                stored: 1,
                skipped: 0,
            }),
            Some(Verb::Recall) => value(&MemoryRecallResponse {
                memory: Vec::new(),
                shared: Vec::new(),
            }),
            Some(Verb::Records) => value(&ToolCallRecordsStored {
                stored_count: 1,
                skipped_count: 0,
            }),
            Some(Verb::Me) => value(&SelfResponse {
                id: RUNNER_ID.into(),
                status: ACTIVE.into(),
                host_id: RUNNER_HOST.into(),
                sandbox_tier: Cow::Owned(
                    value(&self.assigned.sandbox_tier)
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                ),
                last_seen_at: 0,
                assigned_policy: None,
                achievable: None,
                degraded: false,
                degraded_reason: None,
            }),
            Some(
                Verb::Activity
                | Verb::Report
                | Verb::Bundle
                | Verb::Mint
                | Verb::ScheduleCreate
                | Verb::ScheduleList
                | Verb::ScheduleUpdate
                | Verb::ScheduleDelete
                | Verb::ScheduleRun
                | Verb::ScheduleRuns
                | Verb::Message,
            )
            | None => value(&ReportResponse { ok: true }),
        }
    }
}

/// The verb a `method` request for `path` is, among those a healthy daemon
/// answers with a reply of its own; none for a route it only acknowledges.
fn routed(method: &str, path: &str) -> Option<Verb> {
    let path = path.split_once('?').map_or(path, |(path, _query)| path);
    let leased = path
        .strip_prefix(RUNNER_LEASES)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.split_once('/'))
        .map(|(_lease, suffix)| suffix);
    let memory = path.starts_with(RUNNER_MEMORY);
    match (path, leased) {
        (RUNNER_HEARTBEATS, _) => Some(Verb::Heartbeat),
        (RUNNER_LEASES, _) => Some(Verb::Lease),
        (RUNNER_SELF, _) => Some(Verb::Me),
        (_, Some(LEASE_RENEW_SUFFIX)) => Some(Verb::Renew),
        (_, Some(LEASE_TOOL_CALLS_SUFFIX)) => Some(Verb::Records),
        _ if memory && path.ends_with(RUNNER_MEMORY_RECALL_SUFFIX) => Some(Verb::Recall),
        _ if memory && method == GET => Some(Verb::Hydrate),
        _ if memory => Some(Verb::Capture),
        _ => None,
    }
}

/// `reply` as the JSON a daemon sends.
fn value(reply: &impl Serialize) -> Value {
    serde_json::to_value(reply)
        .unwrap_or_else(|never| unreachable!("a wire reply always encodes: {never}"))
}

#[cfg(test)]
#[path = "test_util_tests.rs"]
mod tests;
