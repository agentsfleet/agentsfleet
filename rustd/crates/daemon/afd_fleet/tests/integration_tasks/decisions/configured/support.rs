//! Policy edits use the native runner API; fixture time advances expose the next poll.
use super::super::super::fixture::Case;
use afd_fleet::lease::{Acquired, Billed, Issued};
use sqlx::Connection as _;

pub(in super::super) async fn set(case: &Case, enabled: bool) {
    let runners = case.held.fixtures.runners();
    let row = runners
        .self_record(&case.held.runner)
        .await
        .expect("saved assignment");
    let mut policy = row.assignment.decode().expect("valid saved assignment");
    policy.durable_tasks_enabled = enabled;
    runners
        .edit(
            &case.held.runner,
            afd_runner::RunnerEdit {
                assigned_policy: Some(&policy),
                labels: None,
            },
            af_core::clock::now(),
        )
        .await
        .expect("native durable policy edit");
}

pub(super) async fn expire(case: &Case) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("fixture clock");
    sqlx::query("UPDATE runner.fleet_slots SET lease_expires_at=clock_timestamp()-interval '1 second',hold_expires_at=NULL WHERE fleet_id=$1")
        .bind(&case.held.fleet).execute(&mut *connection).await.expect("slot elapsed");
    sqlx::query("UPDATE runner.leases SET status='expired' WHERE id=$1")
        .bind(&case.held.lease_key)
        .execute(&mut *connection)
        .await
        .expect("prior lease expired");
}

pub(super) async fn selected(case: &Case) -> Option<Acquired> {
    crate::seed::select_fleet(&case.held.leases, &case.held.runner, &case.held.fleet).await
}

pub(super) async fn issue(case: &Case, acquired: &Acquired) -> Option<Issued> {
    case.held
        .leases
        .issue(
            &case.held.runner,
            acquired,
            Billed {
                tenant_id: &case.held.tenant,
                posture: crate::seed::POSTURE,
                provider: crate::seed::PROVIDER,
                model: crate::seed::MODEL,
            },
            af_core::clock::now(),
        )
        .await
        .expect("final issue verdict")
}

pub(super) async fn claimable(case: &Case) -> bool {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("direct claim");
    let mut tx = connection.begin().await.expect("rollback-only claim probe");
    let claim = sqlx::query(afd_fleet::lease::sql::queue::CLAIM_HEAD)
        .bind(&case.held.fleet)
        .bind(&case.held.runner)
        .bind(std::time::Duration::from_secs(60))
        .bind(std::time::Duration::from_secs(30))
        .bind("active")
        .bind("active")
        .bind("expired")
        .bind(af_wire::task::CAPABILITY)
        .bind(af_wire::task::TaskState::Ready.as_str())
        .bind(af_wire::task::TaskState::Running.as_str())
        .bind(afd_fleet::task::DECISION_POLICY_PATH)
        .bind(afd_fleet::lease::sql::FLEET_STATUS_ACTIVE)
        .bind(af_core::event::status::RECEIVED)
        .bind(afd_admission::Producer::Steer.as_str())
        .fetch_optional(&mut *tx)
        .await
        .expect("real admission claim statement");
    tx.rollback()
        .await
        .expect("probe leaves no slot or admission write");
    claim.is_some()
}
