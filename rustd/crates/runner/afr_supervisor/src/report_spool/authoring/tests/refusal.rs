//! A refusal the daemon would repeat drops its entry and never blocks the rest.

use super::{ANSWER, Fixture, LEASE, REPORT, report};
use crate::{
    client::{Call, ControlPlane, Verb},
    error,
    halt::Halt,
    test_support::{self, Answer, Calls},
};
use afd_core::{error_code, id::Uuid7, test_util::trace::Capture};
use afd_wire::authoring::{Outcome, Reported};
use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const OTHER_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8070";
const OTHER_ATTEMPT: &str = "01890a5d-ac96-774b-bcce-b302099a8071";
const CONFLICT: u16 = 409;
const UNAUTHORIZED: u16 = 401;
const UNAVAILABLE: u16 = 503;
const EVENT_REFUSED: &str = "authoring_report_refused";
const EVENT_WAITING: &str = "authoring_report_waiting";
/// The only fields a refusal may log: identifiers and a code, never text.
const REFUSAL_FIELDS: [&str; 5] = ["error_code", "lease_id", "report_id", "event", "message"];
/// Polls that may pass before a removed entry must be gone.
const REMOVAL_POLLS: usize = 1_000;
const POLL: Duration = Duration::from_millis(10);
/// Paused time: an expected send that never comes fails instead of hanging.
const SEND_WITHIN: Duration = Duration::from_secs(60);
/// Paused time long enough for many drain passes to retry, if any would.
const SETTLE: Duration = Duration::from_secs(30);

fn settled() -> Answer {
    test_support::json(&Reported {
        settled: true,
        replayed: false,
        sequence: "1".to_owned(),
        state: Outcome::Completed,
    })
}

fn refused(status: u16) -> Answer {
    let code = Some(error_code::AUTHORING_REPLAY);
    Answer::Fail(error::refused(Verb::AuthoringReport, status, code))
}

struct Drain {
    halt: Arc<Halt>,
    task: JoinHandle<()>,
    received: Calls,
    sends: Arc<AtomicUsize>,
}

impl Drain {
    fn start<F>(fixture: &Fixture, answer: F) -> Self
    where
        F: Fn(&Call, usize) -> Answer + Send + Sync + 'static,
    {
        let sends = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&sends);
        let (plane, received): (ControlPlane, _) =
            test_support::plane(move |call| answer(call, counted.fetch_add(1, Ordering::SeqCst)));
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

async fn gone(directory: &Path) -> bool {
    for _ in 0..REMOVAL_POLLS {
        if !directory.exists() {
            return true;
        }
        tokio::time::sleep(POLL).await;
    }
    false
}

async fn hold_other(fixture: &Fixture) -> std::path::PathBuf {
    let lease = Uuid7::parse(OTHER_LEASE).unwrap();
    fixture
        .spool
        .hold(&lease, OTHER_ATTEMPT, report())
        .await
        .unwrap();
    fixture.directory.with_file_name(OTHER_LEASE)
}

#[tokio::test(start_paused = true)]
async fn a_permanent_refusal_drops_the_entry_after_one_send_and_says_so() {
    let capture = Capture::install();
    let fixture = Fixture::new();
    fixture.hold().await;
    let mut drain = Drain::start(&fixture, |_call, _sent| refused(CONFLICT));
    assert_eq!(drain.next().await.verb, Verb::AuthoringReport);
    assert!(gone(&fixture.directory).await, "refused entry is dropped");
    tokio::time::sleep(SETTLE).await;
    assert_eq!(drain.stop().await, 1, "a dropped report is never resent");
    let line = capture.only(EVENT_REFUSED);
    assert_eq!(line.field("error_code"), Some("UZ-AUTHOR-005"));
    assert_eq!(line.field("lease_id"), Some(LEASE));
    assert_eq!(line.field("report_id"), Some(REPORT));
    let allowed: HashSet<&str> = REFUSAL_FIELDS.into_iter().collect();
    for field in line.fields.keys() {
        assert!(allowed.contains(field.as_str()), "unexpected field {field}");
    }
    for event in capture.events() {
        for value in event.fields.values() {
            assert!(!value.contains(ANSWER), "report text reached a log");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_retryable_failure_keeps_the_entry_and_sends_it_again() {
    let capture = Capture::install();
    let fixture = Fixture::new();
    fixture.hold().await;
    let mut drain = Drain::start(&fixture, |_call, sent| {
        if sent == 0 {
            return Answer::Fail(error::unavailable(Verb::AuthoringReport, UNAVAILABLE));
        }
        settled()
    });
    let first = drain.next().await;
    assert!(
        fixture.directory.exists(),
        "a retryable failure keeps the entry"
    );
    let second = drain.next().await;
    assert_eq!(first, second, "the retry is the same report");
    assert!(
        gone(&fixture.directory).await,
        "the settled retry removes it"
    );
    assert_eq!(drain.stop().await, 2);
    assert_eq!(
        capture.only(EVENT_WAITING).field("error_code"),
        Some(first.verb.code().as_str())
    );
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_REFUSED))
    );
}

#[tokio::test(start_paused = true)]
async fn a_refused_entry_does_not_hold_back_the_entry_behind_it() {
    let fixture = Fixture::new();
    fixture.hold().await;
    let other = hold_other(&fixture).await;
    // Scan order is the file system's, so whichever entry goes first is the
    // one refused, every time it is sent.
    let doomed = Arc::new(Mutex::new(None::<String>));
    let mut drain = Drain::start(&fixture, move |call, _sent| {
        let mut doomed = doomed.lock().unwrap();
        let first = doomed.get_or_insert_with(|| call.path.to_string());
        if *first == call.path {
            return refused(CONFLICT);
        }
        settled()
    });
    let refused_call = drain.next().await;
    let next = drain.next().await;
    assert_ne!(
        refused_call.path, next.path,
        "the next entry went in the same pass"
    );
    assert!(gone(&fixture.directory).await && gone(&other).await);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(drain.stop().await, 2);
}

#[tokio::test(start_paused = true)]
async fn a_refused_token_stops_the_drainer_and_keeps_the_entry() {
    let fixture = Fixture::new();
    fixture.hold().await;
    let mut drain = Drain::start(&fixture, |_call, _sent| refused(UNAUTHORIZED));
    drain.next().await;
    drain.task.await.unwrap();
    assert!(drain.halt.token_refused());
    assert!(
        fixture.directory.exists(),
        "a token refusal is not the report's fault"
    );
    assert_eq!(drain.sends.load(Ordering::SeqCst), 1);
}
