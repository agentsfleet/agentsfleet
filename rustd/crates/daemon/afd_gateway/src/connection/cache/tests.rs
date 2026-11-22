//! Refresh races must never resurrect completed work or undo confirmed cancellation and expiry.

use super::{
    RunCache,
    snapshot::{RunRecord, RunSnapshot},
};
use crate::Result;
use af_core::id::Uuid7;
use af_wire::protocol::RUNS_MAX;
use afd_fleet::run::AssignedRun;
use jiff::Timestamp;

fn assigned_run() -> Result<AssignedRun> {
    Ok(AssignedRun {
        run_id: Uuid7::parse("019a3c00-0000-7000-8000-000000000001")?,
        expires_at: Timestamp::UNIX_EPOCH,
        target: None,
        reported: None,
        draft: None,
    })
}

#[test]
fn an_old_refresh_cannot_resurrect_a_reported_run() -> Result<()> {
    let cache = RunCache::default();
    let assigned = assigned_run()?;
    let id = assigned.run_id.clone();
    cache.insert(assigned.clone(), Timestamp::UNIX_EPOCH)?;
    let revision = cache.0.read().revision;
    let snapshot = RunSnapshot::from_records([RunRecord::from(assigned)]);
    cache.forget(&id);
    cache.apply(snapshot, revision, &[]);
    assert!(!cache.0.read().records.contains_key(&id));
    Ok(())
}

#[test]
fn an_old_refresh_cannot_shorten_a_confirmed_renewal() -> Result<()> {
    let cache = RunCache::default();
    let assigned = assigned_run()?;
    let id = assigned.run_id.clone();
    cache.insert(assigned.clone(), Timestamp::UNIX_EPOCH)?;
    let revision = cache.0.read().revision;
    let snapshot = RunSnapshot::from_records([RunRecord::from(assigned)]);
    cache.renew(&id, Timestamp::MAX);
    cache.apply(snapshot, revision, &[]);
    assert_eq!(
        cache
            .0
            .read()
            .records
            .get(&id)
            .map(|record| record.expires_at),
        Some(Timestamp::MAX)
    );
    Ok(())
}

#[test]
fn a_durable_cancel_still_applies_when_another_update_wins_the_refresh() -> Result<()> {
    let cache = RunCache::default();
    let assigned = assigned_run()?;
    let id = assigned.run_id.clone();
    cache.insert(assigned.clone(), Timestamp::UNIX_EPOCH)?;
    let revision = cache.0.read().revision;
    let mut record = RunRecord::from(assigned);
    record.cancelled = true;
    let snapshot = RunSnapshot::from_records([record]);
    cache.renew(&id, Timestamp::MAX);
    cache.apply(snapshot, revision, std::slice::from_ref(&id));
    assert_eq!(
        cache
            .0
            .read()
            .records
            .get(&id)
            .map(|record| record.cancelled),
        Some(true)
    );
    Ok(())
}

/// A recheck can read the issue transaction's commit before the assignment records
/// the run; the assignment's own insert must not end the connection as a protocol error.
#[test]
fn an_assignment_records_a_run_a_concurrent_refresh_already_loaded() -> Result<()> {
    let cache = RunCache::default();
    let assigned = assigned_run()?;
    let id = assigned.run_id.clone();
    let revision = cache.0.read().revision;
    let mut loaded = RunRecord::from(assigned.clone());
    loaded.cancelled = true;
    loaded.expires_at = Timestamp::MAX;
    cache.apply(RunSnapshot::from_records([loaded]), revision, &[]);
    cache.insert(assigned, Timestamp::UNIX_EPOCH)?;
    let state = cache.0.read();
    assert_eq!(state.records.len(), 1);
    let kept = state
        .records
        .get(&id)
        .map(|record| (record.cancelled, record.expires_at));
    assert_eq!(
        kept,
        Some((true, Timestamp::MAX)),
        "a loaded durable cancel and the later deadline survive the assignment"
    );
    Ok(())
}

/// A run that expires at `expires_at`, the `index`th this host holds.
fn held(index: usize, expires_at: Timestamp) -> Result<AssignedRun> {
    Ok(AssignedRun {
        run_id: Uuid7::parse(&format!("019a3c00-0000-7000-8000-{index:012x}"))?,
        expires_at,
        target: None,
        reported: None,
        draft: None,
    })
}

/// A host at its worker bound gets a run back after one lapses; the lapsed
/// record, which the next refresh would drop, must not end the connection.
#[test]
fn a_full_host_records_a_new_run_once_a_lapsed_one_is_dropped() -> Result<()> {
    let now = Timestamp::UNIX_EPOCH + jiff::SignedDuration::from_hours(1);
    let live = now + jiff::SignedDuration::from_secs(35);
    let cache = RunCache::default();
    for index in 1..RUNS_MAX {
        cache.insert(held(index, live)?, now)?;
    }
    let lapsed = held(RUNS_MAX, now)?;
    cache.insert(lapsed.clone(), Timestamp::UNIX_EPOCH)?;
    let reissued = held(RUNS_MAX + 1, live)?;
    cache.insert(reissued.clone(), now)?;
    let state = cache.0.read();
    assert_eq!(state.records.len(), RUNS_MAX);
    assert!(!state.records.contains_key(&lapsed.run_id));
    assert!(state.records.contains_key(&reissued.run_id));
    Ok(())
}

/// The bound itself stands: live runs are never dropped to admit another.
#[test]
fn a_full_host_of_live_runs_refuses_one_more() -> Result<()> {
    let now = Timestamp::UNIX_EPOCH + jiff::SignedDuration::from_hours(1);
    let live = now + jiff::SignedDuration::from_secs(35);
    let cache = RunCache::default();
    for index in 1..=RUNS_MAX {
        cache.insert(held(index, live)?, now)?;
    }
    let refused = cache.insert(held(RUNS_MAX + 1, live)?, now);
    assert_eq!(
        refused.err().map(|error| error.code()),
        Some(af_core::error_code::INVALID_REQUEST)
    );
    assert_eq!(cache.0.read().records.len(), RUNS_MAX);
    Ok(())
}

/// A refresh that lost the race still carries a durable cancel, but a run the
/// connection already forgot stays forgotten: the cancel names nothing to mark.
#[test]
fn a_late_cancel_for_a_forgotten_run_leaves_it_forgotten() -> Result<()> {
    let cache = RunCache::default();
    let forgotten = assigned_run()?;
    let kept = held(2, Timestamp::MAX)?;
    let id = forgotten.run_id.clone();
    cache.insert(forgotten.clone(), Timestamp::UNIX_EPOCH)?;
    cache.insert(kept.clone(), Timestamp::UNIX_EPOCH)?;
    let revision = cache.0.read().revision;
    let mut cancelled = RunRecord::from(forgotten);
    cancelled.cancelled = true;
    let snapshot = RunSnapshot::from_records([cancelled, RunRecord::from(kept.clone())]);
    cache.forget(&id);
    cache.apply(snapshot, revision, std::slice::from_ref(&id));
    let state = cache.0.read();
    assert!(!state.records.contains_key(&id));
    assert_eq!(
        state
            .records
            .get(&kept.run_id)
            .map(|record| record.cancelled),
        Some(false)
    );
    Ok(())
}

/// A refresh no local update raced replaces the snapshot whole: a run it no
/// longer lists lapses, a run it lists again takes the durable deadline.
#[test]
fn an_unraced_refresh_replaces_the_whole_snapshot() -> Result<()> {
    let cache = RunCache::default();
    let kept = held(1, Timestamp::UNIX_EPOCH)?;
    let dropped = held(2, Timestamp::UNIX_EPOCH)?;
    cache.insert(kept.clone(), Timestamp::UNIX_EPOCH)?;
    cache.insert(dropped.clone(), Timestamp::UNIX_EPOCH)?;
    let revision = cache.0.read().revision;
    let mut renewed = RunRecord::from(kept.clone());
    renewed.expires_at = Timestamp::MAX;
    let added = held(3, Timestamp::MAX)?;
    let snapshot = RunSnapshot::from_records([renewed, RunRecord::from(added.clone())]);
    cache.apply(snapshot, revision, &[]);
    let state = cache.0.read();
    assert_eq!(state.records.len(), 2);
    assert!(!state.records.contains_key(&dropped.run_id));
    assert_eq!(
        state
            .records
            .get(&kept.run_id)
            .map(|record| record.expires_at),
        Some(Timestamp::MAX)
    );
    assert!(state.records.contains_key(&added.run_id));
    Ok(())
}
