//! Finding accepted work whose queue entry the datastore lost.
//!
//! # The half [`Replay`](super::replay::Replay) cannot see
//!
//! The replay dispatcher walks rows that never got a receipt. A row that GOT
//! its receipt and was never delivered is invisible to it, and that is exactly
//! the row a flush destroys: the producer was told yes, the entry is gone, the
//! consumer group went with it, and the readiness mark with that. Nothing polls
//! the fleet, so without this pass nothing ever notices.
//!
//! The repair is to FORGET the receipt, which returns the row to the state the
//! replay dispatcher already scans. So this sweeper never appends — it hands
//! its work to the one that does.
//!
//! # Why its own sweeper and not another step in replay's pass
//!
//! Replay is a Postgres-only pass over one index. This one asks the DATASTORE a
//! question per fleet holding undelivered work, so it costs a round trip where
//! replay costs none, and it has no business running at replay's cadence.
//! Folding it in would also collide with replay's pacing: a full replay batch
//! comes back immediately, and dragging a fleet-sized probe sweep along behind
//! it is round trips spent proving what the last pass just proved.
//!
//! # The two pacings
//!
//! A pass that voided nothing is the steady state of a healthy deployment and
//! waits the ordinary interval. A pass that voided rows found real loss, and
//! its caps mean it probably left more behind, so it comes back at
//! [`RECOVERING_INTERVAL`] — sooner, but never immediately: the rows it voided
//! are the replay dispatcher's backlog now, and coming straight back would
//! raise that backlog faster than the sweeper draining it can keep up with.

use std::sync::Mutex;
use std::time::Duration;

use afd_admission::{Admissions, DEFAULT_REPAIR_CAPACITY, Progress, Reconciled};
use afd_core::clock;

use crate::error::Result;
use crate::sweep::{Sweep, Swept};

/// How many fleets holding undelivered work one pass examines.
///
/// Each one costs a probe to the datastore, so this is the pass's round-trip
/// bound and not a row bound. A deployment with more such fleets is examined
/// over consecutive passes.
const FLEET_LIMIT: i64 = 128;

/// How many of one lost fleet's undelivered admissions a pass repairs.
///
/// The same bound the replay dispatcher takes over its own batch, for the same
/// reason: the transaction holding these row locks is one a live producer
/// recording its receipt waits behind.
const ROW_LIMIT: i64 = 32;

/// How many fleets this sweeper remembers as mid-repair at once.
///
/// Sized to [`FLEET_LIMIT`], because one pass cannot put more fleets into
/// repair than it examined. Written as its own literal rather than cast from
/// it: the two constants have different types for good reasons — one is bound
/// into SQL, one indexes memory — and every cast between them needs a
/// truncation exception that would say less than this sentence does.
///
/// A MEMORY bound, not a round-trip budget. They were one value until a test
/// showed what that coupling did: lowering the per-pass fleet budget silently
/// shrank how much repair progress the sweeper could remember.
const REPAIR_CAPACITY: usize = 128;

/// How long between passes that found nothing to repair.
const INTERVAL: Duration = Duration::from_secs(300);

/// How long between passes after one that voided rows.
const RECOVERING_INTERVAL: Duration = Duration::from_secs(30);

// The bounds above are decidable at compile time, so they are checked there. A
// runtime test could only re-assert a constant a reader can see, and would
// report a bad edit as a red suite instead of a build that does not produce a
// binary.
const _: () = {
    assert!(FLEET_LIMIT > 0, "a pass that probes no fleet never repairs");
    // Every probe is a round trip, and this pass is not on anyone's request
    // path. A limit large enough to spend seconds in one pass would hold the
    // datastore's attention for work nobody is waiting on.
    assert!(
        // pin test: literal is the contract
        FLEET_LIMIT <= 1024,
        "a pass this wide spends its interval in round trips"
    );
    assert!(ROW_LIMIT > 0, "a pass that voids no row never recovers");
    // A reset falls back to the ledger's default, so a default that is not this
    // number would quietly shrink the set every time a lock was poisoned.
    assert!(
        REPAIR_CAPACITY == DEFAULT_REPAIR_CAPACITY,
        "the resume set's fallback capacity must be the one this sweeper asks for"
    );

    assert!(
        ROW_LIMIT <= 128,
        "a batch this large holds row locks a live producer waits behind"
    );
    // A zero interval would spin this pass against Postgres and the datastore
    // forever on an idle deployment.
    assert!(
        !INTERVAL.is_zero(),
        "an idle sweeper must wait between passes"
    );
    // Recovery is meant to be FASTER than the steady state, and still leave the
    // replay dispatcher room to drain what this pass voided.
    assert!(
        RECOVERING_INTERVAL.as_secs() > 0 && RECOVERING_INTERVAL.as_secs() < INTERVAL.as_secs(),
        "recovery paces between immediate and the steady-state interval"
    );
};

/// The lost-receipt reconciler.
#[derive(Debug)]
pub struct Reconcile {
    /// The ledger this pass walks, and the queue it probes through.
    admissions: Admissions,
    /// What the last pass concluded about when to come back.
    pacing: Mutex<Duration>,
    /// Where the next pass resumes — see [`Progress`].
    ///
    /// Held here rather than inside the ledger because it is this sweeper's
    /// place in a rotation, not a fact about the table: a second driver over
    /// the same `Admissions` keeps its own place, and two replicas rotating
    /// independently is the behaviour that spreads the probes rather than the
    /// one that duplicates them.
    progress: Mutex<Progress>,
}

impl Reconcile {
    /// A reconciler over `admissions`.
    #[must_use]
    pub fn new(admissions: Admissions) -> Self {
        Self {
            admissions,
            pacing: Mutex::new(INTERVAL),
            progress: Mutex::new(Progress::with_capacity(REPAIR_CAPACITY)),
        }
    }

    /// Takes the resume state out for the duration of a pass.
    ///
    /// Taken by value, never borrowed across the `await`. A synchronous
    /// `MutexGuard` held over a suspension point blocks every other task on
    /// this worker thread if the future is parked there, and Clippy refuses it
    /// (`await_holding_lock`) for that reason. A pass that panics loses its
    /// place in the rotation and starts over, which costs a slower pass and
    /// never a missed fleet.
    fn take_progress(&self) -> Progress {
        self.progress.lock().map_or_else(
            |_poisoned| Progress::default(),
            |mut held| std::mem::take(&mut *held),
        )
    }
}

/// How long to wait after a pass that concluded `reconciled`.
///
/// Separate from the pass and pure, so the decision is provable without a
/// database and a datastore behind it — see the module note on the two pacings.
const fn pacing_after(reconciled: Reconciled) -> Duration {
    if reconciled.is_quiet() {
        INTERVAL
    } else {
        RECOVERING_INTERVAL
    }
}

impl Sweep for Reconcile {
    fn name(&self) -> &'static str {
        "admission-reconcile"
    }

    /// What the last pass concluded — see the module note on the two pacings.
    fn interval(&self) -> Duration {
        self.pacing.lock().map_or(INTERVAL, |pacing| *pacing)
    }

    async fn sweep(&self) -> Result<Swept> {
        let mut progress = self.take_progress();
        // Put back before `?`, not after. A pass that fails partway has still
        // walked fleets and filed their resume points, and dropping that on a
        // transient database error would restart the rotation every time one
        // happened — which on a deployment where they happen regularly is the
        // starvation this cursor exists to end, reintroduced by the error path.
        let reconciled = self
            .admissions
            .reconcile(clock::now(), FLEET_LIMIT, ROW_LIMIT, &mut progress)
            .await;
        if let Ok(mut held) = self.progress.lock() {
            *held = progress;
        }
        let reconciled = reconciled?;

        if let Ok(mut pacing) = self.pacing.lock() {
            *pacing = pacing_after(reconciled);
        }

        // Fleets asked about, rows repaired. The count BETWEEN them — fleets
        // whose stream could not answer — is not dropped: the ledger logs one
        // line per lost fleet naming the fleet and the receipt, which is what
        // an operator needs and what a tally cannot carry.
        Ok(Swept {
            scanned: reconciled.probed,
            changed: reconciled.voided,
        })
    }
}

#[cfg(test)]
mod tests;
