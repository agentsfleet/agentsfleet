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

use afd_admission::{Admissions, Reconciled};
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
}

impl Reconcile {
    /// A reconciler over `admissions`.
    #[must_use]
    pub fn new(admissions: Admissions) -> Self {
        Self {
            admissions,
            pacing: Mutex::new(INTERVAL),
        }
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
        let reconciled = self
            .admissions
            .reconcile(clock::now(), FLEET_LIMIT, ROW_LIMIT)
            .await?;

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
