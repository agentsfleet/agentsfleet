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

use tokio::time::Instant;

use afd_core::test_util::trace::Capture;
use tokio_util::sync::CancellationToken;

use super::{EVENT_FAILED, Sweep, Swept, run};
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

    /// An hour: every test here runs on tokio's paused clock, so the wait
    /// costs nothing, and an hour is long enough that a loop which waited it
    /// out instead of selecting against cancellation shows on that clock.
    fn interval(&self) -> Duration {
        INTERVAL
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

/// `Counting`'s interval.
const INTERVAL: Duration = Duration::from_hours(1);

/// Twelve intervals and a half: enough passes that one failed pass stopping
/// the loop shows as a count of one.
const SEVERAL_INTERVALS: Duration = Duration::from_mins(12 * 60 + 30);

#[tokio::test(start_paused = true)]
async fn a_cancelled_sweeper_stops_without_waiting_out_its_interval() {
    // The property a poll loop cannot have: cancellation is selected
    // AGAINST the wait, so a sweeper with an hour's interval stops now. The
    // paused clock jumps to the next timer whenever the runtime idles, so a
    // poll loop would still return here, an hour later on that clock: the
    // elapsed-time assertion is what catches it.
    let started = Instant::now();
    let sweeper = Counting::new(false);
    let token = CancellationToken::new();
    let task = tokio::spawn(run(Arc::clone(&sweeper), token.clone()));
    tokio::task::yield_now().await;

    token.cancel();
    task.await.expect(CANCELLED_RETURNS);
    assert!(
        started.elapsed() < INTERVAL,
        "the sweeper waited out its interval before seeing the cancel: {:?}",
        started.elapsed()
    );
    // Cancelled during its first wait, so it never swept — which is also the
    // proof that the wait comes BEFORE the pass.
    assert_eq!(sweeper.passes(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_failing_pass_does_not_stop_the_sweeper() {
    // A datastore blip must not need a daemon restart to get liveness back:
    // every sweep is idempotent and bounded, so a failed pass costs nothing a
    // later pass cannot redo.
    let sweeper = Counting::new(true);
    let token = CancellationToken::new();
    let task = tokio::spawn(run(Arc::clone(&sweeper), token.clone()));

    tokio::time::sleep(SEVERAL_INTERVALS).await;
    let survived = sweeper.passes();
    token.cancel();
    task.await.expect(CANCELLED_RETURNS);

    assert_eq!(
        survived, 12,
        "one pass per interval, failed or not: {survived} passes"
    );
}

/// A failed pass is logged with its code beside a reason that does not repeat
/// it, under `reason` rather than the `error` field it used to carry.
#[tokio::test(start_paused = true)]
async fn a_failed_sweep_reason_carries_no_code_prefix() {
    // Current-thread, so the spawned loop raises its events on the thread the
    // capture listens on.
    let capture = Capture::install();
    let sweeper = Counting::new(true);
    let token = CancellationToken::new();
    let task = tokio::spawn(run(Arc::clone(&sweeper), token.clone()));

    tokio::time::sleep(SEVERAL_INTERVALS).await;
    token.cancel();
    task.await.expect(CANCELLED_RETURNS);

    let expected_code = query("counting sweep")(sqlx::Error::PoolClosed).code();
    let failed: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some(EVENT_FAILED))
        .collect();
    assert!(!failed.is_empty(), "no failed pass was logged");
    for line in failed {
        line.assert_uncoded_reason();
        assert_eq!(line.field("error_code"), Some(expected_code.as_str()));
        assert_eq!(line.field("error"), None, "the reason moved to `reason`");
        assert_eq!(line.field("sweeper"), Some("counting"));
    }
}
