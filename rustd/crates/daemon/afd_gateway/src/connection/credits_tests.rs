//! Availability updates cannot grant capacity already consumed by an unseen assignment.

use std::num::NonZeroU64;

use super::SchedulingState;
use crate::Result;

fn state() -> SchedulingState {
    SchedulingState {
        ready: false,
        degraded: false,
        credits: 0,
        assignments_sent: 0,
        assignments_seen: 0,
        ready_revision: None,
        held: Vec::new(),
        last_assignment: tokio::time::Instant::now(),
        recovery_pending: None,
    }
}

#[test]
fn an_assignment_crossing_a_ready_update_does_not_create_another_idle_worker() -> Result<()> {
    let mut state = state();
    let mut revision = NonZeroU64::MIN;
    state.availability(1, revision, 0)?;
    state.issued()?;
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(1, revision, 0)?;
    assert_eq!(state.credits, 0, "the first assignment is still in flight");
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(0, revision, 1)?;
    assert_eq!(
        state.credits, 0,
        "the runner received the assignment and is busy"
    );
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(1, revision, 1)?;
    assert_eq!(state.credits, 1, "the acknowledged report freed the worker");
    Ok(())
}

#[test]
fn a_second_free_worker_can_receive_work_while_the_first_assignment_is_in_flight() -> Result<()> {
    let mut state = state();
    let first = NonZeroU64::MIN;
    state.availability(2, first, 0)?;
    state.issued()?;
    assert_eq!(
        state.credits, 1,
        "the other worker is available immediately"
    );
    let next = first
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(2, next, 0)?;
    assert_eq!(
        state.credits, 1,
        "an unseen assignment consumes only its own worker"
    );
    state.issued()?;
    assert_eq!(state.credits, 0, "both workers have an assignment");
    Ok(())
}

#[test]
fn asynchronously_completed_older_updates_cannot_overwrite_current_capacity() -> Result<()> {
    let mut state = state();
    let old = NonZeroU64::MIN;
    let current = old
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(0, current, 0)?;
    state.availability(64, old, 0)?;
    assert_eq!(state.credits, 0);
    Ok(())
}

#[test]
fn acknowledgement_cannot_name_an_assignment_the_daemon_never_sent() {
    assert!(
        matches!(state().availability(1, NonZeroU64::MIN, 1), Err(error) if error.code() == af_core::error_code::INVALID_REQUEST)
    );
}

#[test]
fn only_a_rise_in_available_workers_starts_a_connection_pass() -> Result<()> {
    let mut state = state();
    let mut revision = NonZeroU64::MIN;
    assert!(state.availability(2, revision, 0)?);
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    assert!(!state.availability(2, revision, 0)?);
    assert!(!state.availability(64, NonZeroU64::MIN, 0)?);
    state.issued()?;
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    assert!(
        !state.availability(2, revision, 0)?,
        "one assignment is still unseen"
    );
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    assert!(
        state.availability(2, revision, 1)?,
        "acknowledged work freed capacity"
    );
    Ok(())
}

#[tokio::test]
async fn recovery_from_a_policy_recheck_wakes_one_capacity_pass() -> Result<()> {
    let runner = af_core::id::Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
    let connection_id = af_core::id::Uuid7::parse("019f0000-0000-7000-8000-000000000002")?;
    let connection = super::ConnectionHandle::fixture(runner, connection_id);
    connection.snapshot.lock().degraded = true;
    connection.apply_policy(false);
    assert!(matches!(
        futures_util::poll!(Box::pin(connection.assignments.notified())),
        std::task::Poll::Ready(())
    ));
    connection.apply_policy(false);
    assert!(matches!(
        futures_util::poll!(Box::pin(connection.assignments.notified())),
        std::task::Poll::Pending
    ));
    connection.apply_policy(true);
    assert!(matches!(
        futures_util::poll!(Box::pin(connection.assignments.notified())),
        std::task::Poll::Pending
    ));
    Ok(())
}

#[test]
fn a_newer_recovery_request_survives_an_older_empty_assignment() {
    let wake = afd_bus::wake::Wake::default();
    wake.recheck();
    let first = wake.recovery();
    assert!(first.is_some());
    wake.recheck();
    let second = wake.recovery();
    let mut state = state();
    state.recovery_pending = second;
    state.recovered(first);
    assert_eq!(state.recovery_pending, second);
    state.recovered(second);
    assert_eq!(state.recovery_pending, None);
}

#[test]
fn capacity_updates_and_assignments_preserve_an_unfinished_recovery_request() -> Result<()> {
    let wake = afd_bus::wake::Wake::default();
    wake.recheck();
    let recovery = wake.recovery();
    assert!(recovery.is_some());
    let mut state = state();
    state.recovery_pending = recovery;
    state.availability(1, NonZeroU64::MIN, 0)?;
    state.issued()?;
    assert_eq!(state.credits, 0);
    assert_eq!(state.recovery_pending, recovery);
    state.recovered(None);
    assert_eq!(state.recovery_pending, recovery);
    let revision = NonZeroU64::MIN
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(1, revision, 1)?;
    assert_eq!(state.credits, 1);
    assert_eq!(state.recovery_pending, recovery);
    Ok(())
}
