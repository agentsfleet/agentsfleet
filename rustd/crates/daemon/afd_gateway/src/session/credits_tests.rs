//! Availability updates cannot grant capacity already consumed by an unseen offer.

use std::num::NonZeroU64;

use super::SchedulingState;
use crate::Result;

fn state() -> SchedulingState {
    SchedulingState {
        ready: false,
        degraded: false,
        credits: 0,
        offers_sent: 0,
        offers_seen: 0,
        ready_revision: None,
        held: Vec::new(),
        last_offer: tokio::time::Instant::now(),
    }
}

#[test]
fn an_offer_crossing_a_ready_update_does_not_create_another_idle_worker() -> Result<()> {
    let mut state = state();
    let mut revision = NonZeroU64::MIN;
    state.availability(1, revision, 0)?;
    state.issued()?;
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(1, revision, 0)?;
    assert_eq!(state.credits, 0, "the first offer is still in flight");
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(0, revision, 1)?;
    assert_eq!(
        state.credits, 0,
        "the runner received the offer and is busy"
    );
    revision = revision
        .checked_add(1)
        .ok_or(crate::error::ErrorKind::Protocol)?;
    state.availability(1, revision, 1)?;
    assert_eq!(state.credits, 1, "the acknowledged report freed the worker");
    Ok(())
}

#[test]
fn a_second_free_worker_can_receive_work_while_the_first_offer_is_in_flight() -> Result<()> {
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
        "an unseen offer consumes only its own worker"
    );
    state.issued()?;
    assert_eq!(state.credits, 0, "both workers have an offer");
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
fn acknowledgement_cannot_name_an_offer_the_daemon_never_sent() {
    assert!(
        matches!(state().availability(1, NonZeroU64::MIN, 1), Err(error) if error.code() == af_core::error_code::INVALID_REQUEST)
    );
}

#[test]
fn only_a_rise_in_available_workers_starts_a_session_pass() -> Result<()> {
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
        "one offer is still unseen"
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
