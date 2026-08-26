//! Stored runner rows decoded into owned operator-plane values.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::RUNNER_OFFLINE_AFTER_MS;
use afd_wire::admin::{RunnerEventItem, RunnerEventType};
use afd_wire::runner::{
    AssignedPolicy, CapabilityReport, ExtraBind, RunnerLiveness, SelftestCheck, SelftestReport,
};
use serde_json::Value;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Result, admin_state_malformed, query, row_malformed, stored_json};
use crate::runner::policy::{self, AssignmentColumns, StoredVerdict};
use crate::runner::view::{RunnerDetail, RunnerItem};
use crate::sql;

const CONTEXT_RUNNER_LIST: &str = "runner list page";
const CONTEXT_RUNNER_DETAIL: &str = "runner detail";
const CONTEXT_EVENT_LIST: &str = "runner event page";
const TABLE_RUNNERS: &str = "fleet.runners";
const TABLE_EVENTS: &str = "fleet.runner_events";
const COLUMN_ID: &str = "id";

pub(super) fn runner_item(row: &PgRow, now: UnixMillis) -> Result<RunnerItem> {
    base_item(
        row,
        8,
        row.try_get(7).map_err(query(CONTEXT_RUNNER_LIST))?,
        now,
        CONTEXT_RUNNER_LIST,
    )
}

pub(super) fn runner_detail(row: &PgRow, now: UnixMillis) -> Result<RunnerDetail> {
    let column = query(CONTEXT_RUNNER_DETAIL);
    let active_lease_count = row.try_get(7).map_err(&column)?;
    Ok(RunnerDetail {
        item: base_item(row, 13, active_lease_count > 0, now, CONTEXT_RUNNER_DETAIL)?,
        active_lease_count,
        active_fleet_count: row.try_get(8).map_err(&column)?,
        leases_acquired: row.try_get(9).map_err(&column)?,
        leases_succeeded: row.try_get(10).map_err(&column)?,
        leases_failed: row.try_get(11).map_err(&column)?,
        leases_expired: row.try_get(12).map_err(&column)?,
        selftest_requested_at: row.try_get(20).map_err(&column)?,
        selftest_completed_at: row.try_get(21).map_err(&column)?,
        selftest: selftest(row, &column)?,
    })
}

fn base_item(
    row: &PgRow,
    policy_at: usize,
    has_live_lease: bool,
    now: UnixMillis,
    context: &'static str,
) -> Result<RunnerItem> {
    let column = query(context);
    let id: String = row.try_get(0).map_err(&column)?;
    let raw_admin_state: String = row.try_get(3).map_err(&column)?;
    let last_seen_at = row.try_get(5).map_err(&column)?;
    let columns = assignment(row, policy_at, &column)?;
    let capability_json: Option<String> = row.try_get(policy_at + 3).map_err(&column)?;
    let verdict = StoredVerdict {
        degraded: row.try_get(policy_at + 4).map_err(&column)?,
        reason: row.try_get(policy_at + 5).map_err(&column)?,
    };
    Ok(RunnerItem {
        id: Uuid7::parse(&id).map_err(row_malformed(TABLE_RUNNERS, COLUMN_ID))?,
        host_id: row.try_get(1).map_err(&column)?,
        sandbox_tier: columns.sandbox_tier.clone(),
        admin_state: afd_core::spelling::from_spelling(&raw_admin_state)
            .ok_or_else(admin_state_malformed)?,
        liveness: derive_liveness(last_seen_at, has_live_lease, now),
        labels: labels(&row.try_get::<String, _>(4).map_err(&column)?),
        last_seen_at,
        created_at: row.try_get(6).map_err(&column)?,
        assigned_policy: columns.decode().map(own_policy),
        achievable: policy::capability(capability_json.as_deref()).map(own_capability),
        degraded: verdict.degraded,
        degraded_reason: verdict.reason,
    })
}

fn assignment(
    row: &PgRow,
    at: usize,
    column: &impl Fn(sqlx::Error) -> crate::error::Error,
) -> Result<AssignmentColumns> {
    Ok(AssignmentColumns {
        sandbox_tier: row.try_get(2).map_err(column)?,
        network_policy: row.try_get(at).map_err(column)?,
        registry_allowlist_json: row.try_get(at + 1).map_err(column)?,
        worker_count: row.try_get(at + 2).map_err(column)?,
        extra_binds_json: row.try_get(at + 6).map_err(column)?,
    })
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

fn labels(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn selftest(
    row: &PgRow,
    column: &impl Fn(sqlx::Error) -> crate::error::Error,
) -> Result<Option<SelftestReport<'static>>> {
    let raw: Option<String> = row.try_get(22).map_err(column)?;
    let all_ok: Option<bool> = row.try_get(23).map_err(column)?;
    let tier: Option<String> = row.try_get(24).map_err(column)?;
    let network: Option<String> = row.try_get(25).map_err(column)?;
    let Some((raw, all_ok, tier, network)) = raw
        .zip(all_ok)
        .zip(tier)
        .zip(network)
        .map(|(((raw, all_ok), tier), network)| (raw, all_ok, tier, network))
    else {
        return Ok(None);
    };
    let checks = serde_json::from_str::<Vec<SelftestCheck<'_>>>(&raw)
        .ok()
        .map(|checks| checks.into_iter().map(own_check).collect());
    Ok(checks.map(|checks| SelftestReport {
        checks,
        all_ok,
        sandbox_tier: Cow::Owned(tier),
        network_policy: Cow::Owned(network),
    }))
}

fn own_check(check: SelftestCheck<'_>) -> SelftestCheck<'static> {
    SelftestCheck {
        name: Cow::Owned(check.name.into_owned()),
        ok: check.ok,
        detail: Cow::Owned(check.detail.into_owned()),
    }
}

pub(super) fn runner_event(row: &PgRow) -> Result<RunnerEventItem<'static>> {
    let column = query(CONTEXT_EVENT_LIST);
    let id: String = row.try_get(0).map_err(&column)?;
    let runner_id: String = row.try_get(1).map_err(&column)?;
    let raw_type: String = row.try_get(2).map_err(&column)?;
    Uuid7::parse(&id).map_err(row_malformed(TABLE_EVENTS, COLUMN_ID))?;
    Uuid7::parse(&runner_id).map_err(row_malformed(TABLE_EVENTS, "runner_id"))?;
    let event_type = serde_json::from_value::<RunnerEventType>(Value::String(raw_type))
        .map_err(stored_json(TABLE_EVENTS, "event_type"))?;
    let metadata_raw: String = row.try_get(4).map_err(&column)?;
    Ok(RunnerEventItem {
        id: Cow::Owned(id),
        runner_id: Cow::Owned(runner_id),
        event_type,
        occurred_at: row.try_get(3).map_err(&column)?,
        metadata: serde_json::from_str(&metadata_raw)
            .map_err(stored_json(TABLE_EVENTS, "metadata"))?,
    })
}

pub(super) fn derive_liveness(
    last_seen_at: i64,
    has_live_lease: bool,
    now: UnixMillis,
) -> RunnerLiveness {
    if last_seen_at == sql::LAST_SEEN_NEVER {
        RunnerLiveness::Registered
    } else if has_live_lease {
        RunnerLiveness::Busy
    } else if now.saturating_millis_since(UnixMillis::from_millis(last_seen_at))
        <= RUNNER_OFFLINE_AFTER_MS
    {
        RunnerLiveness::Online
    } else {
        RunnerLiveness::Offline
    }
}
