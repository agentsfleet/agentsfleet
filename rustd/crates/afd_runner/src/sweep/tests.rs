//! What the shared sweeper loop promises, against a sweeper that counts.
//!
//! No datastore takes part: what [`run`] owns is the LOOP — that it waits, that
//! cancellation interrupts the wait rather than being noticed after it, and
//! that a failed pass is not a stopped sweeper. All three are properties of the
//! driver, and a real sweep would only make them slower to prove.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{Sweep, Swept, run};
use crate::error::{Result, query};

/// A sweeper that records how often it ran and answers what it was told to.
#[derive(Debug)]
struct Counting {
    /// Passes performed.
    passes: AtomicU64,
    /// Whether each pass fails.
    fails: bool,
}

impl Counting {
    /// A sweeper answering `fails` on every pass.
    fn new(fails: bool) -> Arc<Self> {
        Arc::new(Self {
            passes: AtomicU64::new(0),
            fails,
        })
    }

    /// How many passes it has performed.
    fn passes(&self) -> u64 {
        self.passes.load(Ordering::SeqCst)
    }
}

impl Sweep for Arc<Counting> {
    fn name(&self) -> &'static str {
        "counting"
    }

    /// Short enough that a test observes several passes without waiting, and
    /// long enough that the runtime is not spinning.
    fn interval(&self) -> Duration {
        Duration::from_millis(5)
    }

    fn sweep(&self) -> impl Future<Output = Result<Swept>> + Send {
        self.passes.fetch_add(1, Ordering::SeqCst);
        // Not an `async fn`: this stand-in awaits nothing, and writing one would
        // be an `async` the compiler correctly points out is decorative. What
        // the trait asks for is a future, and a ready one is a future.
        std::future::ready(if self.fails {
            Err(query("counting sweep")(sqlx::Error::PoolClosed))
        } else {
            Ok(Swept {
                scanned: 1,
                changed: 1,
            })
        })
    }
}

/// What a sweeper's join answers once its token is cancelled.
///
/// Declared once (RULE UFS): both cancellation cases assert the same thing, and
/// two spellings of it read as two different expectations in a failure log.
const CANCELLED_RETURNS: &str = "a cancelled sweeper returns";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_sweeper_stops_without_waiting_out_its_interval() {
    // The property the Zig's 100ms poll cannot have: cancellation is selected
    // AGAINST the wait, so a sweeper with a ten-minute interval stops now. If
    // this were a poll loop the join below would hang until the interval
    // elapsed, and the test would time out rather than fail.
    let sweeper = Counting::new(false);
    let token = CancellationToken::new();
    let task = tokio::spawn(run(Arc::clone(&sweeper), token.clone()));

    token.cancel();
    task.await.expect(CANCELLED_RETURNS);
    // Cancelled during its first wait, so it never swept — which is also the
    // proof that the wait comes BEFORE the pass.
    assert_eq!(sweeper.passes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_pass_does_not_stop_the_sweeper() {
    // A datastore blip must not need a daemon restart to get liveness back:
    // every sweep is idempotent and bounded, so a failed pass costs nothing a
    // later pass cannot redo.
    let sweeper = Counting::new(true);
    let token = CancellationToken::new();
    let task = tokio::spawn(run(Arc::clone(&sweeper), token.clone()));

    // Long enough for several intervals to elapse.
    tokio::time::sleep(Duration::from_millis(60)).await;
    let survived = sweeper.passes();
    token.cancel();
    task.await.expect(CANCELLED_RETURNS);

    assert!(
        survived > 1,
        "the sweeper stopped after a failed pass: {survived} passes"
    );
}
