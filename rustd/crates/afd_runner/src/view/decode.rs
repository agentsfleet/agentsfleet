//! Stored runner rows decoded into owned operator-plane values.
//!
//! Each statement's row is a named `FromRow` struct, so a column is found by
//! the name the statement gives it and never by a position a later column
//! could shift. Liveness is derived at the `checked_at` the statement answered
//! with — the database's clock, the one the live-lease test inside it used.

use std::borrow::Cow;

use afd_api_wire::admin::{RunnerEventItem, RunnerEventType};
use afd_core::clock;
use afd_core::id::Uuid7;
use afd_core::timing::RUNNER_OFFLINE_AFTER;
use afd_wire::runner::{
    AssignedPolicy, CapabilityReport, ExtraBind, RunnerLiveness, SelftestCheck, SelftestReport,
};
use jiff::Timestamp;
use serde_json::Value;

use crate::error::{Result, admin_state_malformed, row_malformed, stored_json};
use crate::policy::{self, AssignmentColumns};
use crate::sql;
use crate::view::{RunnerDetail, RunnerItem};

const TABLE_RUNNERS: &str = "runner.runners";
const TABLE_EVENTS: &str = "runner.events";
const COLUMN_ID: &str = "id";
const COLUMN_RUNNER_ID: &str = "runner_id";

/// The columns every runner read selects, by name.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct ItemRow {
    id: String,
    host_id: String,
    sandbox_tier: String,
    status: String,
    labels: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    last_seen_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    checked_at: Timestamp,
    live_lease: bool,
    network_policy: String,
    registry_allowlist: String,
    worker_count: i32,
    capability_report: Option<String>,
    degraded_reason: Option<String>,
    sandbox_config: Option<String>,
}

/// The detail read: the item's columns, its counts and its last self-test.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct DetailRow {
    #[sqlx(flatten)]
    item: ItemRow,
    active_lease_count: i64,
    active_fleet_count: i64,
    leases_acquired: i64,
    leases_succeeded: i64,
    leases_failed: i64,
    leases_expired: i64,
    selftest_requested_at: Option<jiff_sqlx::Timestamp>,
    selftest_completed_at: Option<jiff_sqlx::Timestamp>,
    selftest_checks: Option<String>,
    selftest_sandbox_tier: Option<String>,
    selftest_network_policy: Option<String>,
}

/// One history row, by name.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct EventRow {
    id: String,
    runner_id: String,
    event_type: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    metadata: String,
}

impl ItemRow {
    /// The list row, its identifier, status and stored JSON proven.
    pub(super) fn parse(self) -> Result<RunnerItem> {
        let columns = AssignmentColumns {
            sandbox_tier: self.sandbox_tier,
            network_policy: self.network_policy,
            registry_allowlist_json: self.registry_allowlist,
            worker_count: self.worker_count,
            sandbox_config_json: self.sandbox_config,
        };
        Ok(RunnerItem {
            id: Uuid7::parse(&self.id).map_err(row_malformed(TABLE_RUNNERS, COLUMN_ID))?,
            host_id: self.host_id,
            sandbox_tier: columns.sandbox_tier.clone(),
            status: afd_core::spelling::from_spelling(&self.status)
                .ok_or_else(admin_state_malformed)?,
            liveness: derive_liveness(self.last_seen_at, self.live_lease, self.checked_at),
            labels: labels(&self.labels)?,
            last_seen_at: self.last_seen_at,
            created_at: self.created_at,
            assigned_policy: own_policy(columns.decode()?),
            achievable: policy::capability(self.capability_report.as_deref()).map(own_capability),
            degraded: self.degraded_reason.is_some(),
            degraded_reason: self.degraded_reason,
        })
    }
}

impl DetailRow {
    /// The detail row, built on the list row's decode.
    pub(super) fn parse(self) -> Result<RunnerDetail> {
        let selftest = selftest(
            self.selftest_checks,
            self.selftest_sandbox_tier,
            self.selftest_network_policy,
        );
        Ok(RunnerDetail {
            item: self.item.parse()?,
            active_lease_count: self.active_lease_count,
            active_fleet_count: self.active_fleet_count,
            leases_acquired: self.leases_acquired,
            leases_succeeded: self.leases_succeeded,
            leases_failed: self.leases_failed,
            leases_expired: self.leases_expired,
            selftest_requested_at: self
                .selftest_requested_at
                .map(jiff_sqlx::Timestamp::to_jiff),
            selftest_completed_at: self
                .selftest_completed_at
                .map(jiff_sqlx::Timestamp::to_jiff),
            selftest,
        })
    }
}

impl EventRow {
    /// The history item, its identifiers, type and metadata proven.
    pub(super) fn parse(self) -> Result<RunnerEventItem<'static>> {
        Uuid7::parse(&self.id).map_err(row_malformed(TABLE_EVENTS, COLUMN_ID))?;
        Uuid7::parse(&self.runner_id).map_err(row_malformed(TABLE_EVENTS, COLUMN_RUNNER_ID))?;
        let event_type = serde_json::from_value::<RunnerEventType>(Value::String(self.event_type))
            .map_err(stored_json(TABLE_EVENTS, "event_type"))?;
        Ok(RunnerEventItem {
            id: Cow::Owned(self.id),
            runner_id: Cow::Owned(self.runner_id),
            event_type,
            occurred_at: self.created_at,
            metadata: serde_json::from_str(&self.metadata)
                .map_err(stored_json(TABLE_EVENTS, "metadata"))?,
        })
    }
}

fn own_policy(policy: AssignedPolicy<'_>) -> AssignedPolicy<'static> {
    AssignedPolicy {
        sandbox_tier: policy.sandbox_tier,
        network_policy: policy.network_policy,
        registry_allowlist: policy
            .registry_allowlist
            .into_iter()
            .map(|host| Cow::Owned(host.into_owned()))
            .collect(),
        worker_count: policy.worker_count,
        extra_binds: policy.extra_binds.into_iter().map(own_bind).collect(),
    }
}

fn own_bind(bind: ExtraBind<'_>) -> ExtraBind<'static> {
    ExtraBind {
        path: Cow::Owned(bind.path.into_owned()),
        mode: bind.mode,
        note: Cow::Owned(bind.note.into_owned()),
    }
}

fn own_capability(report: CapabilityReport<'_>) -> CapabilityReport<'static> {
    CapabilityReport {
        build_id: report.build_id.map(|value| Cow::Owned(value.into_owned())),
        landlock: report.landlock,
        seccomp: report.seccomp,
        cgroup_controllers: report
            .cgroup_controllers
            .into_iter()
            .map(|controller| Cow::Owned(controller.into_owned()))
            .collect(),
        bubblewrap: report.bubblewrap,
        egress_enforcement: report.egress_enforcement,
    }
}

fn labels(raw: &str) -> Result<Vec<String>> {
    serde_json::from_str(raw).map_err(stored_json(TABLE_RUNNERS, "labels"))
}

/// The last complete self-test, when the row holds one.
///
/// Whether it passed is not a column: it is read from the checks, which the
/// acceptor held to the host's own summary before they were stored. Checks
/// that no longer parse read as no self-test, the way they always have.
pub(super) fn selftest(
    checks: Option<String>,
    sandbox_tier: Option<String>,
    network_policy: Option<String>,
) -> Option<SelftestReport<'static>> {
    let (raw, (tier, network)) = checks.zip(sandbox_tier.zip(network_policy))?;
    let checks: Vec<SelftestCheck<'static>> = serde_json::from_str::<Vec<SelftestCheck<'_>>>(&raw)
        .ok()?
        .into_iter()
        .map(own_check)
        .collect();
    Some(SelftestReport {
        all_ok: checks.iter().all(|check| check.ok),
        checks,
        sandbox_tier: Cow::Owned(tier),
        network_policy: Cow::Owned(network),
    })
}

fn own_check(check: SelftestCheck<'_>) -> SelftestCheck<'static> {
    SelftestCheck {
        name: Cow::Owned(check.name.into_owned()),
        ok: check.ok,
        detail: Cow::Owned(check.detail.into_owned()),
    }
}

/// What an operator reads a runner as, judged at `checked_at`.
///
/// Fresh is inclusive: a runner whose last beat is exactly the offline
/// threshold old is still online, the same edge the liveness sweep keeps. A
/// beat stamped after `checked_at` — a skewed clock — reads as fresh.
pub(super) fn derive_liveness(
    last_seen_at: Timestamp,
    has_live_lease: bool,
    checked_at: Timestamp,
) -> RunnerLiveness {
    if last_seen_at == sql::NEVER_SEEN {
        RunnerLiveness::Registered
    } else if has_live_lease {
        RunnerLiveness::Busy
    } else if last_seen_at >= clock::before(checked_at, RUNNER_OFFLINE_AFTER) {
        RunnerLiveness::Online
    } else {
        RunnerLiveness::Offline
    }
}

#[cfg(test)]
mod tests {
    use super::{labels, selftest};

    #[test]
    fn malformed_stored_labels_are_not_silently_erased() {
        let result = labels(r#"{"not":"a label list"}"#);
        let code = result.as_ref().err().map(|error| error.code().as_str());

        assert_eq!(code, Some("UZ-INTERNAL-002"));
    }

    #[test]
    fn a_self_test_passes_only_when_every_stored_check_did() {
        let tier = || Some("dev_none".to_owned());
        let network = || Some("deny_all_egress".to_owned());
        let passed = r#"[{"name":"landlock","ok":true,"detail":""}]"#;
        let failed = r#"[{"name":"landlock","ok":true,"detail":""},{"name":"seccomp","ok":false,"detail":"absent"}]"#;

        let passed = selftest(Some(passed.to_owned()), tier(), network());
        let failed = selftest(Some(failed.to_owned()), tier(), network());

        assert_eq!(passed.map(|report| report.all_ok), Some(true));
        assert_eq!(failed.map(|report| report.all_ok), Some(false));
    }

    #[test]
    fn a_row_with_no_complete_self_test_reads_as_none() {
        let checks = Some("[]".to_owned());

        assert!(selftest(checks.clone(), None, Some("deny_all_egress".to_owned())).is_none());
        assert!(selftest(None, Some("dev_none".to_owned()), None).is_none());
        assert!(
            selftest(
                Some("not-json".to_owned()),
                Some("t".to_owned()),
                Some("n".to_owned())
            )
            .is_none()
        );
    }
}
