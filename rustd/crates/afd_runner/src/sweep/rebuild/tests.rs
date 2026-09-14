//! What a rebuild promises, against passes that only record.
//!
//! No datastore takes part. What [`rebuild`] owns is the LOOP — that every
//! pass runs every round, in the order given, that the tallies are the sum,
//! and that a failed pass ends the rebuild with its error rather than being
//! skipped — and a real sweeper would only make those slower to prove.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{Rebuilt, rebuild};
use crate::error::{Result, query};
use crate::sweep::{Sweep, Swept};

/// The order every pass ran in, shared by all of them.
type Trace = Arc<Mutex<Vec<&'static str>>>;

/// A sweeper that records its turn and answers what it was built to.
#[derive(Debug)]
struct Recording {
    name: &'static str,
    trace: Trace,
    passes: AtomicU64,
    /// What each pass reports; `None` makes the pass fail instead.
    answer: Option<Swept>,
}

impl Recording {
    fn new(name: &'static str, trace: &Trace, answer: Option<Swept>) -> Arc<Self> {
        Arc::new(Self {
            name,
            trace: Arc::clone(trace),
            passes: AtomicU64::new(0),
            answer,
        })
    }

    fn passes(&self) -> u64 {
        self.passes.load(Ordering::SeqCst)
    }
}

// Implemented as a `Sweep`, not a `Pass`, so the blanket impl is what gets
// exercised: a rebuild over real sweepers goes through exactly this path.
impl Sweep for Arc<Recording> {
    fn name(&self) -> &'static str {
        self.name
    }

    fn interval(&self) -> Duration {
        Duration::ZERO
    }

    fn sweep(&self) -> impl Future<Output = Result<Swept>> + Send {
        self.passes.fetch_add(1, Ordering::SeqCst);
        self.trace.lock().expect("trace lock").push(self.name);
        std::future::ready(
            self.answer
                .ok_or_else(|| query("recording sweep")(sqlx::Error::PoolClosed)),
        )
    }
}

fn swept(scanned: u64, changed: u64) -> Swept {
    Swept { scanned, changed }
}

#[tokio::test]
async fn every_pass_runs_every_round_in_the_order_given() {
    let trace: Trace = Arc::default();
    let reconcile = Recording::new("reconcile", &trace, Some(swept(3, 1)));
    let replay = Recording::new("replay", &trace, Some(swept(5, 2)));
    let reclaim = Recording::new("reclaim", &trace, Some(swept(7, 7)));

    let tally = rebuild(&[&reconcile, &replay, &reclaim], 2)
        .await
        .expect("no pass fails");

    assert_eq!(
        *trace.lock().expect("trace lock"),
        [
            "reconcile",
            "replay",
            "reclaim",
            "reconcile",
            "replay",
            "reclaim"
        ],
        "a round is the passes in the order given, and a rebuild is the rounds in order"
    );
    assert_eq!(
        tally,
        Rebuilt {
            rounds: 2,
            scanned: 2 * (3 + 5 + 7),
            changed: 2 * (1 + 2 + 7),
        },
        "the tally is the sum over every pass of every round"
    );
}

#[tokio::test]
async fn a_failing_pass_ends_the_rebuild_with_its_error() {
    let trace: Trace = Arc::default();
    let first = Recording::new("first", &trace, Some(swept(1, 1)));
    let broken = Recording::new("broken", &trace, None);
    let never = Recording::new("never", &trace, Some(swept(1, 1)));

    let outcome = rebuild(&[&first, &broken, &never], 3).await;

    assert!(
        outcome.is_err(),
        "a skipped pass is a cache refilled from part of the ledger"
    );
    assert_eq!(
        first.passes(),
        1,
        "the rebuild stopped inside its first round"
    );
    assert_eq!(
        broken.passes(),
        1,
        "the failing pass was tried exactly once"
    );
    assert_eq!(
        never.passes(),
        0,
        "nothing after the failure ran, in that round or any later one"
    );
}

#[tokio::test]
async fn zero_rounds_runs_nothing_and_says_so() {
    let trace: Trace = Arc::default();
    let pass = Recording::new("pass", &trace, Some(swept(9, 9)));

    let tally = rebuild(&[&pass], 0)
        .await
        .expect("nothing ran, nothing failed");

    assert_eq!(pass.passes(), 0);
    assert_eq!(
        tally,
        Rebuilt::default(),
        "zeros, not a tally from a pass that never ran"
    );
}

#[tokio::test]
async fn a_pass_that_changes_nothing_still_runs_every_round() {
    // The reason this loop counts rounds and not quiescence: a pass that
    // reports zero is not thereby finished, and one that reports nonzero is
    // not thereby unfinished. Both run the rounds they were given.
    let trace: Trace = Arc::default();
    let quiet = Recording::new("quiet", &trace, Some(swept(4, 0)));
    let busy = Recording::new("busy", &trace, Some(swept(4, 4)));

    let tally = rebuild(&[&quiet, &busy], 5).await.expect("no pass fails");

    assert_eq!(quiet.passes(), 5, "zero changed is not a stop condition");
    assert_eq!(
        busy.passes(),
        5,
        "nonzero changed is not a continue condition either"
    );
    assert_eq!(tally.changed, 20);
}
