//! Work that was handed to a runner nobody can reach any more.
//!
//! An event delivered to a consumer that has stopped reading — a retired daemon
//! instance, a legacy throwaway consumer name — sits in that consumer's pending
//! list forever. `XREADGROUP >` only ever hands out entries nobody has seen, so
//! nothing re-delivers it and the work simply stops. This sweeper claims those
//! entries away, into a consumer that is alive, where the lease path's own
//! pending read finds them on the next poll.
//!
//! # It is also the readiness index's backstop
//!
//! The readiness index is a HINT and the streams are the system of record. A
//! mark that failed at ingress, or an index that was evicted or flushed, leaves
//! a fleet holding deliverable work that no candidate scan will look at. Each
//! pass re-marks any fleet that still holds work, so a lost mark heals itself.
//!
//! The sweep only ever re-marks and never clears, and the asymmetry is
//! deliberate: a false positive costs one wasted candidate check, and a false
//! negative strands an event until somebody notices.
//!
//! # Why the cursor exists
//!
//! Recovery has to be bounded by the fleet COUNT, not by a flat interval. A
//! pass reaches at most one batch, so a strand outside the current batch waits
//! the claim idle-time plus however many passes it takes the cursor to come
//! round. Without the cursor that bound is infinite — the pass would re-read
//! the same head of the population every time and never reach the tail.

use std::time::Duration;

use afd_core::clock::{self, UnixMillis};
use afd_db::Db;
use afd_dragonfly::Redis;
use afd_dragonfly::streams::FleetStreams;
use afd_observability::producers;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::sql;
use crate::sweep::{Sweep, Swept};

/// Statement name, for the context a query failure carries.
const CONTEXT_FLEETS: &str = "reclaim active fleets";

/// Statement name, for the context the ledger question's failure carries.
const CONTEXT_STRANDED: &str = "reclaim stranded leases";

/// The fleet status whose streams are worth sweeping.
pub(crate) const STATUS_ACTIVE: &str = "active";

/// How many fleets one pass reaches, on either of its questions.
///
/// Public so a proof can size its envelope one past it and exercise the wrap,
/// rather than at a literal this constant can drift past unnoticed.
pub const BATCH_LIMIT: i64 = 100;

/// How many entries one pass claims per fleet.
///
/// A bound so one pathological stream cannot monopolise a pass; the next pass
/// continues where this one stopped. It terminates for a second reason too: a
/// claimed entry's idle clock resets, so a re-encountered entry is no longer
/// eligible.
const CLAIM_LIMIT: usize = 10;

/// How often a pass runs.
const INTERVAL: Duration = Duration::from_secs(30);

/// The identifier every fleet's `(updated_at, id)` cursor starts below.
///
/// The nil UUID, which sorts below every version-7 identifier. Never stored —
/// only ever compared against.
const CURSOR_START_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Where a pass stopped, so the next one resumes rather than restarts.
///
/// An `Option` of the pair, where `reclaim_sweeper.zig` carries a fixed
/// `[36]u8` buffer, a length, and an `afterId()` that substitutes a nil-UUID
/// constant when the length is zero. The `None` IS that substitution, and
/// `rewind` is `= None` rather than three field assignments that have to agree.
#[derive(Debug, Clone, Default)]
struct Cursor(Option<(i64, String)>);

impl Cursor {
    /// The instant this cursor resumes after.
    const fn after_updated_at(&self) -> i64 {
        match &self.0 {
            Some((updated_at, _id)) => *updated_at,
            None => 0,
        }
    }

    /// The identifier this cursor resumes after.
    fn after_id(&self) -> &str {
        match &self.0 {
            Some((_updated_at, id)) => id,
            None => CURSOR_START_ID,
        }
    }
}

/// The reclaim pass, over the fleet streams and the fleets that own them.
#[derive(Debug, Clone)]
pub struct Reclaim {
    /// Where the active fleets are listed.
    database: Db,
    /// The streams entries are claimed on.
    streams: FleetStreams,
    /// The readiness index a deliverable fleet is re-marked in.
    ready: afd_dragonfly::ready::ReadyIndex,
    /// This instance's stable consumer name, which claimed entries land in.
    consumer: Box<str>,
    /// Where the last pass stopped.
    ///
    /// Owned by the sweeper task and touched by nothing else, but the trait's
    /// `sweep` takes `&self` — so the interior mutability is a `Mutex` rather
    /// than a `&mut`, and it is never contended.
    cursor: std::sync::Arc<tokio::sync::Mutex<Cursor>>,
    /// Where the ledger question's last page ended: a fleet id, or `None`
    /// for the start. Its own cursor, because the two questions walk two
    /// populations — every active fleet, and the fleets holding an expired
    /// lease — that advance and wrap independently.
    stranded_after: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
}

impl Reclaim {
    /// A sweeper claiming into `consumer`.
    #[must_use]
    pub fn new(database: Db, queue: Redis, consumer: impl Into<Box<str>>) -> Self {
        Self {
            database,
            streams: FleetStreams::new(queue.clone()),
            ready: afd_dragonfly::ready::ReadyIndex::new(queue),
            consumer: consumer.into(),
            cursor: std::sync::Arc::new(tokio::sync::Mutex::new(Cursor::default())),
            stranded_after: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// The next page of active fleets, advancing the cursor past it.
    ///
    /// A SHORT page means the population is exhausted, so the cursor rewinds
    /// and the next pass starts from the beginning — which is what makes the
    /// scan cyclic rather than terminal.
    async fn next_page(&self) -> Result<Vec<String>> {
        let mut cursor = self.cursor.lock().await;
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::sweep::SELECT_ACTIVE_FLEETS_AFTER)
            .bind(STATUS_ACTIVE)
            .bind(cursor.after_updated_at())
            .bind(cursor.after_id())
            .bind(BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_FLEETS))?;

        let page = rows
            .iter()
            .map(|row| {
                let id: String = row.try_get(0).map_err(query(CONTEXT_FLEETS))?;
                let updated_at: i64 = row.try_get(1).map_err(query(CONTEXT_FLEETS))?;
                Ok((id, updated_at))
            })
            .collect::<Result<Vec<_>>>()?;

        *cursor = match page.last() {
            Some((id, updated_at)) if page.len() >= usize::try_from(BATCH_LIMIT).unwrap_or(0) => {
                Cursor(Some((*updated_at, id.clone())))
            }
            // A short page, or none at all: the cursor reached the end of the
            // active set and the next pass starts over.
            _exhausted => Cursor::default(),
        };
        Ok(page.into_iter().map(|(id, _updated_at)| id).collect())
    }

    /// Claims what one fleet has stranded, up to the per-pass bound.
    ///
    /// A Redis failure collapses to "claimed nothing" rather than failing the
    /// pass: every other fleet in the batch is still worth sweeping, and this
    /// one is retried on the next pass.
    async fn claim_strays(&self, fleet_id: &str) -> u64 {
        let mut claimed = 0;
        for _attempt in 0..CLAIM_LIMIT {
            match self.streams.autoclaim(fleet_id, &self.consumer).await {
                Ok(Some(_entry)) => claimed += 1,
                Ok(None) => break,
                Err(failure) => {
                    tracing::warn!(
                        fleet_id,
                        error = %failure,
                        event = "reclaim_claim_failed",
                        "a stranded entry could not be claimed; the next pass retries it"
                    );
                    break;
                }
            }
        }
        claimed
    }

    /// Re-marks a fleet that still holds work a runner could pick up.
    ///
    /// `claimed_any` short-circuits the probe: an entry just claimed into this
    /// instance's pending list is deliverable by definition, so there is
    /// nothing left to ask Redis.
    async fn remark_if_deliverable(&self, fleet_id: &str, claimed_any: bool) -> bool {
        if !claimed_any {
            match self.streams.has_deliverable(fleet_id).await {
                Ok(false) => return false,
                Ok(true) => {}
                Err(failure) => {
                    // Loud, because this probe IS the recovery path's backstop:
                    // a silent failure leaves it inert while looking exactly
                    // like an idle system.
                    tracing::warn!(
                        fleet_id,
                        error = %failure,
                        event = "deliverable_probe_failed",
                        "a fleet's deliverable state could not be read"
                    );
                    return false;
                }
            }
        }
        self.mark_ready(fleet_id).await
    }

    /// Marks one fleet ready, answering whether the mark was written.
    ///
    /// The one path both of this sweeper's writers share — the stream probe
    /// above and the ledger question below — so a mark that fails is counted
    /// and logged the same way whichever question raised it.
    async fn mark_ready(&self, fleet_id: &str) -> bool {
        match self.ready.mark(fleet_id, &self.consumer).await {
            Ok(_token) => true,
            Err(failure) => {
                producers::fleet::ready_write_failed();
                tracing::warn!(
                    fleet_id,
                    error = %failure,
                    event = "ready_remark_failed",
                    "a deliverable fleet could not be re-marked ready"
                );
                false
            }
        }
    }

    /// Fleets the ledger says still owe work that nothing in the datastore
    /// will surface: an `active` lease past its expiry, held by a runner
    /// nobody can reach, on a stream that may no longer exist.
    ///
    /// Read from PostgreSQL in the sweeper and nowhere near the poll path,
    /// whose zero-PostgreSQL property is deliberate. Bounded by the same
    /// fleet count as the stream walk, because each fleet marked here is a
    /// claim the pool will see on the next poll.
    async fn stranded_fleets(&self, now: UnixMillis) -> Result<Vec<String>> {
        let mut after = self.stranded_after.lock().await;
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::sweep::SELECT_FLEETS_HOLDING_EXPIRED_LEASES)
            .bind(vec![sql::LEASE_STATUS_ACTIVE])
            .bind(now.as_millis())
            .bind(BATCH_LIMIT)
            .bind(after.as_deref().unwrap_or(CURSOR_START_ID))
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_STRANDED))?;
        let page = rows
            .iter()
            .map(|row| row.try_get(0).map_err(query(CONTEXT_STRANDED)))
            .collect::<Result<Vec<String>>>()?;
        // A full page may have more behind it; a short one reached the end,
        // and the next pass starts over — the same rule the fleet walk uses.
        *after = match page.last() {
            Some(last) if page.len() >= usize::try_from(BATCH_LIMIT).unwrap_or(0) => {
                Some(last.clone())
            }
            _exhausted => None,
        };
        Ok(page)
    }
}

impl Sweep for Reclaim {
    fn name(&self) -> &'static str {
        "reclaim"
    }

    fn interval(&self) -> Duration {
        INTERVAL
    }

    async fn sweep(&self) -> Result<Swept> {
        let fleets = self.next_page().await?;
        let mut swept = Swept {
            scanned: u64::try_from(fleets.len()).unwrap_or(u64::MAX),
            changed: 0,
        };
        for fleet_id in &fleets {
            let claimed = self.claim_strays(fleet_id).await;
            swept.changed += claimed;
            if self.remark_if_deliverable(fleet_id, claimed > 0).await {
                swept.changed += 1;
            }
        }
        // The ledger's answer, after the stream's: a fleet the stream could
        // not vouch for may still hold a lease a dead runner never finished.
        for fleet_id in self.stranded_fleets(clock::now()).await? {
            if self.mark_ready(&fleet_id).await {
                swept.changed += 1;
                tracing::info!(
                    fleet_id,
                    event = "stranded_lease_surfaced",
                    "a fleet holding an expired lease was marked ready, so the next claim re-leases its work from the ledger"
                );
            }
        }
        Ok(swept)
    }
}

#[cfg(test)]
mod tests;
