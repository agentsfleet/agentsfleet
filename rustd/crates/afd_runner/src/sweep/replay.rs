//! Re-appending admissions the queue never confirmed.
//!
//! # Why this exists at all
//!
//! A producer commits its ledger row, appends the entry, then records the
//! receipt. Three steps and two stores, so a crash between the first and the
//! third leaves a row that was ACCEPTED and never queued: the producer was
//! told yes, and no runner will ever see the work. This pass is what closes
//! that window, and it is the whole reason acceptance could move off the
//! queue in the first place.
//!
//! # The age floor is what stops it racing live admissions
//!
//! A row younger than [`MIN_AGE`] may belong to a producer that is about to
//! record its own receipt, and re-appending it would put a second physical
//! entry on the stream for no reason. The floor is generous relative to the
//! two statements and one append it covers.
//!
//! # The three pacings
//!
//! A full batch means more work is waiting, so the next pass follows
//! immediately. A pass the queue refused waits the ordinary interval, because
//! coming back sooner would find the same queue still refusing. An empty pass
//! — the steady state of a healthy deployment — waits the same interval.

use std::sync::Mutex;
use std::time::Duration;

use afd_admission::Admissions;
use afd_core::clock;
use afd_observability::producers;

use crate::error::Result;
use crate::sweep::{Sweep, Swept};

/// How long a row is left for its own producer before this pass takes it.
const MIN_AGE: Duration = Duration::from_secs(30);

/// How many rows one pass re-appends.
///
/// Bounded so a backlog cannot monopolise a pass, and small enough that the
/// transaction holding the batch's row locks is short — a producer recording
/// its own receipt waits behind exactly this many appends, never a whole
/// backlog.
const BATCH_LIMIT: i64 = 32;

/// How long between ordinary passes.
const INTERVAL: Duration = Duration::from_secs(30);

// The three bounds above are decidable at compile time, so they are checked
// there. A runtime test could only ever re-assert a constant a reader can see,
// and would report a bad edit as a red suite instead of a build that does not
// produce a binary.
const _: () = {
    // A producer between its commit and its receipt runs two statements and one
    // append. A floor near that duration would make this pass race live
    // admissions and put a second physical entry on the stream every time it
    // won, which the lease path then has to drop.
    assert!(
        MIN_AGE.as_secs() >= 10,
        "a floor this low races the producer it is meant to wait for"
    );
    // The transaction holding the batch blocks any producer trying to record a
    // receipt for one of those rows, so the batch is the bound on how long that
    // producer waits. Unbounded here would mean a backlog stalling live
    // acceptance, which is the opposite of what the ledger is for.
    assert!(BATCH_LIMIT > 0, "a pass that takes no rows never drains");
    assert!(
        BATCH_LIMIT <= 128,
        "a batch this large holds row locks a live producer waits behind"
    );
    // A zero interval would make an idle deployment spin this pass against
    // Postgres forever, which is the cost the `is_clean` gate exists to avoid
    // paying except when there is genuinely more work.
    assert!(
        !INTERVAL.is_zero(),
        "an idle sweeper must wait between passes"
    );
};

/// The unfinished-admission dispatcher.
#[derive(Debug)]
pub struct Replay {
    /// The ledger this pass walks.
    admissions: Admissions,
    /// How long a row is left for its own producer — [`MIN_AGE`] on the
    /// interval loop, zero for a rebuild. See [`Replay::for_rebuild`].
    min_age: Duration,
    /// What the last pass concluded about when to come back.
    pacing: Mutex<Duration>,
}

impl Replay {
    /// A dispatcher over `admissions`, leaving young rows to their producers.
    #[must_use]
    pub fn new(admissions: Admissions) -> Self {
        Self {
            admissions,
            min_age: MIN_AGE,
            pacing: Mutex::new(INTERVAL),
        }
    }

    /// A dispatcher for a rebuild, which leaves no row to its producer.
    ///
    /// The age floor exists so a pass does not race a producer that is about
    /// to record its own receipt and put a second entry on the stream for no
    /// reason. After a flush there is no reason left: the entry that producer
    /// appended is already gone, and if it appends again now, its receipt
    /// write is guarded on `receipt IS NULL` and the extra entry is dropped at
    /// lease — the same two guards that make the interval loop's floor a
    /// courtesy rather than a correctness condition. A rebuild owes every
    /// unreceipted row immediately, and waiting thirty seconds per round for
    /// rows the cache just lost would be the floor protecting nothing.
    #[must_use]
    pub fn for_rebuild(admissions: Admissions) -> Self {
        Self {
            min_age: Duration::ZERO,
            ..Self::new(admissions)
        }
    }
}

impl Sweep for Replay {
    fn name(&self) -> &'static str {
        "admission-replay"
    }

    /// What the last pass concluded — see the module note on the three
    /// pacings.
    fn interval(&self) -> Duration {
        self.pacing.lock().map_or(INTERVAL, |pacing| *pacing)
    }

    async fn sweep(&self) -> Result<Swept> {
        let now = clock::now();
        let replayed = self
            .admissions
            .replay(now, self.min_age, BATCH_LIMIT)
            .await?;
        // Counted whole after the pass, not from the batch: the batch is
        // capped, and a capped count published as the backlog would read
        // "thirty-two" over a hundred thousand.
        let backlog = self.admissions.backlog(now).await?;
        producers::fleet::admission_backlog_observed(
            backlog.rows,
            backlog.oldest_age.map_or(0, |age| age.as_secs()),
        );

        // A full batch that the queue took every entry of means more is
        // waiting; anything else waits. A pass the queue refused partway is
        // NOT full-batch pacing, because the next pass would find the same
        // queue still refusing.
        let full = replayed.scanned >= u64::try_from(BATCH_LIMIT).unwrap_or(u64::MAX);
        let next = if full && replayed.is_clean() {
            Duration::ZERO
        } else {
            INTERVAL
        };
        if let Ok(mut pacing) = self.pacing.lock() {
            *pacing = next;
        }
        Ok(Swept {
            scanned: replayed.scanned,
            changed: replayed.appended,
        })
    }
}
