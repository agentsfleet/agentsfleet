//! The drain's reshape of a one-post report an older replica refused for its
//! shape: the plain copy is posted, and a copy the spool cannot move or write
//! is left for the boot sweep rather than lost.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use af_core::error_code;
use af_core::test_util::trace::Capture;
use af_wire::report::ReportResponse;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::Drainer;
use crate::client::{ControlPlane, Verb};
use crate::error;
use crate::halt::Halt;
use crate::holds::Holds;
use crate::report_spool::{Delivery, ReportSpool, reshaped};
use crate::storage_home::StorageHome;
use crate::test_support::{Answer, Calls, LEASE_ID, clock, json, plane};

/// A one-post report a lease left held, its memory and segments beside the
/// rest of the body.
const ONE_POST_REPORT: &str = r#"{"lease_id":"x","memory":[{"key":"k","content":"v","category":"core"}],"telemetry":{"wall_ms":5,"segments":{"settle_ms":2}}}"#;

/// The code a spool file that cannot be moved, written or removed carries.
const SPOOL_FAILED: &str = error_code::INTERNAL_OPERATION_FAILED.as_str();

fn one_post() -> serde_json::Value {
    serde_json::from_str(ONE_POST_REPORT).unwrap()
}

/// That report as the older replica reads it: no memory, no segments.
fn plain_report() -> serde_json::Value {
    serde_json::json!({"lease_id": "x", "telemetry": {"wall_ms": 5}})
}

/// The refusal an older replica gives a body shaped past what it reads.
fn shape_refusal() -> crate::Error {
    error::refused(Verb::Report, 400, Some(error_code::INVALID_REQUEST))
}

fn taken() -> Answer {
    json(&ReportResponse { ok: true })
}

/// A fresh storage home whose spool holds [`ONE_POST_REPORT`] under `name`, as
/// a process that died with it held leaves it.
fn home_holding(root: &Path, name: &str) -> StorageHome {
    let home = StorageHome::open(root).unwrap();
    fs::write(
        home.spool().join(name).with_extension("json"),
        ONE_POST_REPORT,
    )
    .unwrap();
    home
}

/// Where the spool moves `name`'s report when the daemon refuses it.
fn aside(spool: &Path, name: &str) -> PathBuf {
    spool.join(name).with_extension("rejected")
}

/// Every file in the spool directory, whatever its extension.
fn files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect()
}

/// Whether the spool holds no file at all. Pending reports alone read empty in
/// the instant a refused report has been moved aside and its reshaped copy not
/// yet written; the moved copy stays until the reshaped one is answered.
fn emptied(files: &[PathBuf]) -> bool {
    files.is_empty()
}

/// A delivery as a value a test compares: its outcome and the code it carries.
fn named(delivered: &crate::Result<Delivery>) -> (&'static str, Option<&'static str>) {
    match delivered {
        Ok(Delivery::Settled) => ("settled", None),
        Ok(Delivery::Superseded) => ("superseded", None),
        Ok(Delivery::Kept(failure)) => ("kept", Some(failure.code().as_str())),
        Ok(Delivery::Rejected(refusal)) => ("rejected", Some(refusal.code().as_str())),
        Err(failure) => ("failed", Some(failure.code().as_str())),
    }
}

/// A drain's parts over one storage home, against a daemon answering its
/// `n`th report post with `answer(n, spool)`.
struct Parts {
    spool: ReportSpool,
    plane: ControlPlane,
    calls: Calls,
    shutdown: CancellationToken,
    halt: Halt,
    held: Notify,
    holds: Holds,
}

impl Parts {
    fn new<F>(home: &StorageHome, answer: F) -> Self
    where
        F: Fn(usize, &Path) -> Answer + Send + Sync + 'static,
    {
        let posts = AtomicUsize::new(0);
        let dir = home.spool();
        let (plane, calls) = plane(move |_call| answer(posts.fetch_add(1, Ordering::SeqCst), &dir));
        let shutdown = CancellationToken::new();
        Self {
            spool: ReportSpool::new(home),
            plane,
            calls,
            halt: Halt::new(shutdown.clone()),
            shutdown,
            held: Notify::new(),
            holds: Holds::start(clock()),
        }
    }

    fn drainer(&self) -> Drainer<'_> {
        Drainer {
            spool: &self.spool,
            plane: &self.plane,
            halt: &self.halt,
            held: &self.held,
            holds: &self.holds,
        }
    }

    /// Every report body posted so far.
    fn bodies(&mut self) -> Vec<serde_json::Value> {
        crate::test_support::drain(&mut self.calls)
            .iter()
            .map(|call| serde_json::from_slice(call.body.as_ref().unwrap()).unwrap())
            .collect()
    }
}

/// Drains `home`'s spool through `parts` until `done(files)` holds or ten
/// virtual minutes pass, and returns the bodies posted and the files left.
///
/// The deadline is read inside the loop, never set as a `timeout`: the paused
/// clock jumps to the next timer whenever every task waits on blocking file
/// work, and a `timeout` would be that timer.
async fn drain(
    home: &StorageHome,
    mut parts: Parts,
    done: impl Fn(&[PathBuf]) -> bool,
) -> (Vec<serde_json::Value>, Vec<PathBuf>) {
    let dir = home.spool();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    let ((), ()) = tokio::join!(parts.drainer().run(), async {
        while tokio::time::Instant::now() < deadline && !done(&files(&dir)) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        parts.shutdown.cancel();
    });
    (parts.bodies(), files(&dir))
}

/// A report the drain posts to an older replica, which refuses its shape, is
/// posted again without the run's memory and segments, and nothing is left in
/// the spool: the answer is kept though the memory is not.
#[tokio::test(start_paused = true)]
async fn test_drain_reposts_a_shape_refused_report_without_its_memory() {
    let root = tempfile::tempdir().unwrap();
    let home = home_holding(root.path(), LEASE_ID);
    let capture = Capture::install();
    let parts = Parts::new(&home, |post, _spool| {
        if post == 0 {
            Answer::Fail(shape_refusal())
        } else {
            taken()
        }
    });

    let drained = drain(&home, parts, emptied).await;

    assert_eq!(drained, (vec![one_post(), plain_report()], Vec::new()));
    let dropped = capture.only("report_memory_dropped");
    assert_eq!(
        (dropped.field("lease_id"), dropped.field("cause_error_code")),
        (Some(LEASE_ID), Some("UZ-REQ-001"))
    );
}

/// A reshaped report the daemon keeps, a 503 mid-deploy, stays held and is
/// posted again on the drain's backoff rather than waiting on an unrelated
/// wake; the spool empties once the daemon takes it.
#[tokio::test(start_paused = true)]
async fn test_a_reshaped_report_the_daemon_keeps_is_retried() {
    let root = tempfile::tempdir().unwrap();
    let home = home_holding(root.path(), LEASE_ID);
    let parts = Parts::new(&home, |post, _spool| match post {
        0 => Answer::Fail(shape_refusal()),
        1 => Answer::Fail(error::unavailable(Verb::Report, 503)),
        _ => taken(),
    });

    let drained = drain(&home, parts, emptied).await;

    assert_eq!(
        drained,
        (vec![one_post(), plain_report(), plain_report()], Vec::new())
    );
}

/// A refused report the spool cannot move aside stays held, the failure is
/// logged with its code, and the report is posted again on the backoff.
#[tokio::test(start_paused = true)]
async fn test_a_report_that_cannot_be_set_aside_is_posted_again() {
    let root = tempfile::tempdir().unwrap();
    let home = home_holding(root.path(), LEASE_ID);
    let capture = Capture::install();
    let parts = Parts::new(&home, |post, spool| {
        let aside = aside(spool, LEASE_ID);
        if post == 0 {
            // A directory where the refused copy would go: the move fails.
            fs::create_dir(&aside).unwrap();
            Answer::Fail(error::refused(Verb::Report, 400, None))
        } else {
            fs::remove_dir(&aside).unwrap();
            taken()
        }
    });

    let drained = drain(&home, parts, emptied).await;

    let failed = capture.only("report_spool_replay_failed");
    assert_eq!(
        (drained, failed.field("error_code")),
        (
            (vec![one_post(), one_post()], Vec::new()),
            Some(SPOOL_FAILED)
        )
    );
}

/// A shape-refused report under a file no lease id names is set aside as
/// refused and never reshaped: there is no lease to spool its plain copy under.
#[tokio::test(start_paused = true)]
async fn test_a_shape_refused_report_no_lease_names_is_set_aside() {
    const NAMELESS: &str = "left-by-hand";
    let root = tempfile::tempdir().unwrap();
    let home = home_holding(root.path(), NAMELESS);
    let refused = aside(&home.spool(), NAMELESS);
    let parts = Parts::new(&home, |_post, _spool| Answer::Fail(shape_refusal()));

    let drained = drain(&home, parts, |files| files == [refused.clone()]).await;

    assert_eq!(drained, (vec![one_post()], vec![refused]));
}

/// A reshaped copy the spool cannot hold durably posts nothing and answers the
/// refusal, so the pass sets the report aside for the boot sweep instead of
/// posting a copy no file backs.
#[tokio::test(start_paused = true)]
async fn test_a_reshaped_report_the_spool_cannot_hold_posts_nothing() {
    let root = tempfile::tempdir().unwrap();
    let home = home_holding(root.path(), LEASE_ID);
    let mut parts = Parts::new(&home, |_post, _spool| taken());
    let spooled = parts.spool.pending().await.unwrap().remove(0);
    let capture = Capture::install();

    // Write and search without read: the copy is written and renamed into
    // place, then the directory cannot be opened to sync the rename.
    fs::set_permissions(home.spool(), Permissions::from_mode(0o300)).unwrap();
    let outcome = parts.drainer().reshape(&spooled, shape_refusal()).await;
    fs::set_permissions(home.spool(), Permissions::from_mode(0o700)).unwrap();

    let unspooled = capture.only("report_reshape_unspooled");
    assert_eq!(
        (
            named(&outcome),
            parts.bodies(),
            unspooled.field("error_code"),
            unspooled.field("cause_error_code"),
            unspooled.field("lease_id"),
        ),
        (
            ("rejected", Some("UZ-REQ-001")),
            Vec::new(),
            Some(SPOOL_FAILED),
            Some("UZ-REQ-001"),
            Some(LEASE_ID),
        )
    );
}

/// A refused copy the spool cannot remove once its reshaped copy is taken is
/// left for the boot sweep, and the report still counts as settled.
#[tokio::test(start_paused = true)]
async fn test_a_refused_copy_that_will_not_go_is_left_for_the_boot_sweep() {
    let root = tempfile::tempdir().unwrap();
    let home = home_holding(root.path(), LEASE_ID);
    // A directory where the refused copy is: removing it as a file fails.
    fs::create_dir(aside(&home.spool(), LEASE_ID)).unwrap();
    let mut parts = Parts::new(&home, |_post, _spool| taken());
    let spooled = parts.spool.pending().await.unwrap().remove(0);
    let capture = Capture::install();

    let outcome = parts.drainer().reshape(&spooled, shape_refusal()).await;

    let kept = capture.only("report_spool_rejected_kept");
    assert_eq!(
        (
            named(&outcome),
            parts.bodies(),
            kept.field("error_code"),
            kept.field("lease_id"),
            files(&home.spool()),
        ),
        (
            ("settled", None),
            vec![plain_report()],
            Some(SPOOL_FAILED),
            Some(LEASE_ID),
            Vec::new(),
        )
    );
}

/// A refusal of any other shape is quarantined as before, never reshaped.
#[test]
fn test_only_a_shape_refused_one_post_body_is_reshaped() {
    let other = error::refused(Verb::Report, 400, None);
    let plain = br#"{"lease_id":"x","telemetry":{"wall_ms":5}}"#;
    assert_eq!(
        (
            reshaped(&other, ONE_POST_REPORT.as_bytes()),
            reshaped(&shape_refusal(), plain),
            reshaped(&shape_refusal(), b"not json"),
        ),
        (None, None, None)
    );
}
