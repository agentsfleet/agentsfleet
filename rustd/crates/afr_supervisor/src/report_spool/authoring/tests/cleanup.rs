//! A private-content cleanup that fails is reported and retried, and nothing
//! is built or sent over expired bytes that are still on disk.

use super::{ANSWER, ATTEMPT, Fixture, LEASE, report};
use crate::{
    client::{Call, Verb},
    halt::Halt,
    report_spool::authoring::{PRIVATE_TTL, files},
    test_support::{self, Answer, Calls},
};
use afd_core::{id::Uuid7, test_util::trace::Capture};
use afd_wire::authoring::{Outcome, Report, Reported};
use std::{
    os::unix::fs::PermissionsExt as _,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const EVENT_CLEANUP_FAILED: &str = "authoring_spool_cleanup_failed";
/// A second, healthy lease beside the fixture's.
const HEALTHY_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8090";
/// Readable and searchable, so the scan reaches the entry, but no file in it
/// can be removed.
const UNREMOVABLE: u32 = 0o500;
const RESTORED: u32 = 0o700;
/// Paused time long enough for many failing drain passes.
const SETTLE: Duration = Duration::from_secs(30);
/// Paused time: an expected send that never comes fails instead of hanging.
const SEND_WITHIN: Duration = Duration::from_secs(60);
/// Polls that may pass before the settled entry must be gone.
const REMOVAL_POLLS: usize = 1_000;
const POLL: Duration = Duration::from_millis(10);

fn settled() -> Answer {
    test_support::json(&Reported {
        settled: true,
        replayed: false,
        sequence: "1".to_owned(),
        state: Outcome::Completed,
    })
}

fn mode(directory: &Path, mode: u32) {
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// The online drainer over a daemon that settles every report it is sent.
struct Drain {
    halt: Arc<Halt>,
    task: JoinHandle<()>,
    received: Calls,
    sends: Arc<AtomicUsize>,
}

impl Drain {
    fn start(fixture: &Fixture) -> Self {
        let sends = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&sends);
        let (plane, received) = test_support::plane(move |_call| {
            counted.fetch_add(1, Ordering::SeqCst);
            settled()
        });
        let halt = Arc::new(Halt::new(CancellationToken::new()));
        let task = tokio::spawn({
            let (spool, halt) = (fixture.spool.clone(), Arc::clone(&halt));
            async move { spool.run(&plane, &halt).await }
        });
        Self {
            halt,
            task,
            received,
            sends,
        }
    }

    fn sends(&self) -> usize {
        self.sends.load(Ordering::SeqCst)
    }

    async fn next(&mut self) -> Call {
        let call = tokio::time::timeout(SEND_WITHIN, self.received.recv()).await;
        call.unwrap().unwrap()
    }

    async fn stop(self) -> usize {
        self.halt.stop();
        self.task.await.unwrap();
        self.sends.load(Ordering::SeqCst)
    }
}

/// The report `call` carried is content-free accounting.
fn assert_accounting_only(call: &Call) {
    assert_eq!(call.verb, Verb::AuthoringReport);
    let body = call.body.clone().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        report.get("kind").and_then(serde_json::Value::as_str),
        Some("accounting"),
        "expired text is never sent"
    );
    assert!(!String::from_utf8_lossy(&body).contains(ANSWER));
}

/// Polls until `directory` is gone or the poll budget is spent.
async fn wait_removed(directory: &Path) {
    let mut polls = 0;
    loop {
        tokio::time::sleep(POLL).await;
        polls += 1;
        if !directory.exists() || polls == REMOVAL_POLLS {
            break;
        }
    }
}

/// How many cleanup failures were logged, for `lease` or, given `None`, for
/// the spool itself.
fn failures(capture: &Capture, lease: Option<&str>) -> usize {
    capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some(EVENT_CLEANUP_FAILED))
        .filter(|event| event.field("lease_id") == lease)
        .count()
}

fn held(capture: &Capture, lease: &str) -> usize {
    failures(capture, Some(lease))
}

/// Holds a fresh report under [`HEALTHY_LEASE`] and answers its directory.
async fn hold_healthy(fixture: &Fixture) -> std::path::PathBuf {
    let lease = Uuid7::parse(HEALTHY_LEASE).unwrap();
    fixture.spool.hold(&lease, ATTEMPT, report()).await.unwrap();
    fixture.directory.with_file_name(HEALTHY_LEASE)
}

#[tokio::test(start_paused = true)]
async fn a_failed_cleanup_is_reported_and_holds_the_entry_until_cleanup_succeeds() {
    let capture = Capture::install();
    let fixture = Fixture::new();
    fixture.hold().await;
    fixture.clock.advance(PRIVATE_TTL);
    mode(&fixture.directory, UNREMOVABLE);
    let content = fixture.directory.join(files::CONTENT);
    let swept = fixture.spool.sweep().await;
    assert!(swept.is_ok(), "one held entry never fails the spool");
    assert!(held(&capture, LEASE) > 0, "the held entry is reported");
    let loaded = fixture.spool.load(fixture.directory.clone()).await;
    assert!(loaded.is_err(), "no report is built over expired content");
    assert!(
        content.exists(),
        "the fault must hold: run as a user the directory mode binds"
    );
    let mut drain = Drain::start(&fixture);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(drain.sends(), 0, "nothing sent while cleanup fails");
    assert!(
        held(&capture, LEASE) > 2,
        "each failed pass is reported, then retried"
    );
    mode(&fixture.directory, RESTORED);
    assert_accounting_only(&drain.next().await);
    wait_removed(&fixture.directory).await;
    assert!(!fixture.directory.exists(), "the settled entry is removed");
    assert_eq!(drain.stop().await, 1);
}

#[tokio::test(start_paused = true)]
async fn one_unremovable_entry_never_holds_back_a_healthy_one() {
    let capture = Capture::install();
    let fixture = Fixture::new();
    fixture.hold().await;
    fixture.clock.advance(PRIVATE_TTL);
    let healthy = hold_healthy(&fixture).await;
    mode(&fixture.directory, UNREMOVABLE);
    let mut drain = Drain::start(&fixture);
    let first = drain.next().await;
    assert!(first.path.contains(HEALTHY_LEASE), "{}", first.path);
    wait_removed(&healthy).await;
    assert!(!healthy.exists(), "the healthy entry settles and goes");
    tokio::time::sleep(SETTLE).await;
    assert_eq!(drain.sends(), 1, "the held entry is never sent");
    assert!(
        fixture.directory.join(files::CONTENT).exists(),
        "privacy fails closed"
    );
    assert!(held(&capture, LEASE) > 0);
    assert_eq!(held(&capture, HEALTHY_LEASE), 0);
    mode(&fixture.directory, RESTORED);
    let second = drain.next().await;
    assert!(second.path.contains(LEASE), "{}", second.path);
    assert_accounting_only(&second);
    wait_removed(&fixture.directory).await;
    assert_eq!(drain.stop().await, 2);
}

#[tokio::test(start_paused = true)]
async fn an_unwritable_spool_fails_the_whole_pass_and_reports_it() {
    let capture = Capture::install();
    let fixture = Fixture::new();
    fixture.hold().await;
    let root = fixture.directory.parent().unwrap().to_path_buf();
    mode(&root, UNREMOVABLE);
    assert!(
        fixture.spool.sweep().await.is_err(),
        "the clock cannot be saved"
    );
    let mut drain = Drain::start(&fixture);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(drain.sends(), 0);
    assert!(failures(&capture, None) > 1, "each failed pass is reported");
    mode(&root, RESTORED);
    assert_eq!(drain.next().await.verb, Verb::AuthoringReport);
    wait_removed(&fixture.directory).await;
    assert_eq!(drain.stop().await, 1);
}

#[tokio::test]
async fn stray_items_are_removed_and_an_accounting_only_report_seals_no_content() {
    let fixture = Fixture::new();
    let accounting = Report::Accounting {
        accounting: report().accounting().clone(),
    };
    let lease = Uuid7::parse(LEASE).unwrap();
    fixture
        .spool
        .hold(&lease, ATTEMPT, accounting)
        .await
        .unwrap();
    let names: Vec<_> = std::fs::read_dir(&fixture.directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from(files::ACCOUNTING)]);
    let root = fixture.directory.parent().unwrap();
    let stray = root.join("stray.tmp");
    std::fs::write(&stray, b"left by a crash").unwrap();
    let empty = root.join(HEALTHY_LEASE);
    std::fs::create_dir(&empty).unwrap();
    fixture.spool.sweep().await.unwrap();
    assert!(!stray.exists(), "a file that is no entry is removed");
    assert!(!empty.exists(), "an entry with no accounting is removed");
    assert!(fixture.directory.exists(), "the real entry stays");
}
