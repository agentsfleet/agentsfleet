//! Refresh races must never resurrect completed work or undo confirmed cancellation and expiry.

use super::{
    LeaseCache,
    snapshot::{LeaseRecord, LeaseSnapshot},
};
use crate::Result;
use af_core::id::Uuid7;
use afd_fleet::lease::Offered;
use jiff::Timestamp;

fn offer() -> Result<Offered> {
    Ok(Offered {
        lease_id: Uuid7::parse("019a3c00-0000-7000-8000-000000000001")?,
        expires_at: Timestamp::UNIX_EPOCH,
        target: None,
        reported: None,
    })
}

#[test]
fn an_old_refresh_cannot_resurrect_a_reported_lease() -> Result<()> {
    let cache = LeaseCache::default();
    let offered = offer()?;
    let id = offered.lease_id.clone();
    cache.insert(offered.clone())?;
    let revision = cache.0.read().revision;
    let snapshot = LeaseSnapshot::from_records([LeaseRecord::from(offered)]);
    cache.forget(&id);
    cache.apply(snapshot, revision, &[]);
    assert!(!cache.0.read().records.contains_key(&id));
    Ok(())
}

#[test]
fn an_old_refresh_cannot_shorten_a_confirmed_renewal() -> Result<()> {
    let cache = LeaseCache::default();
    let offered = offer()?;
    let id = offered.lease_id.clone();
    cache.insert(offered.clone())?;
    let revision = cache.0.read().revision;
    let snapshot = LeaseSnapshot::from_records([LeaseRecord::from(offered)]);
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
    let cache = LeaseCache::default();
    let offered = offer()?;
    let id = offered.lease_id.clone();
    cache.insert(offered.clone())?;
    let revision = cache.0.read().revision;
    let mut record = LeaseRecord::from(offered);
    record.cancelled = true;
    let snapshot = LeaseSnapshot::from_records([record]);
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
