//! Delivery resends the same retained report until the daemon reads it settled.

#![expect(
    clippy::indexing_slicing,
    reason = "the test compares the two sends it just waited for"
)]

use super::Fixture;
use crate::{
    client::Verb,
    halt::Halt,
    test_support::{self, Answer},
};
use afd_wire::authoring::{Outcome, Reported};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

/// Polls that may pass before the settled entry must be gone.
const REMOVAL_POLLS: usize = 1_000;
const POLL: Duration = Duration::from_millis(10);
/// Paused time: a drainer that dropped the report never sends again, and this
/// turns that silence into a failure instead of a hang.
const RESEND_WITHIN: Duration = Duration::from_secs(60);
/// Paused time: past the private retention window, so a stalled upload that
/// the expiry timer failed to interrupt turns into a failure, not a hang.
const EXPIRY_WITHIN: Duration = Duration::from_hours(48);

fn acknowledgement(settled: bool, replayed: bool) -> Answer {
    test_support::json(&Reported {
        settled,
        replayed,
        sequence: "1".to_owned(),
        state: Outcome::Failed,
    })
}

#[tokio::test(start_paused = true)]
async fn an_unsettled_acknowledgement_keeps_the_report_until_its_replay_settles() {
    let fixture = Fixture::new();
    fixture.hold().await;
    let sends = Arc::new(AtomicUsize::new(0));
    let retained = Arc::new(AtomicBool::new(false));
    let (plane, mut received) = test_support::plane({
        let (sends, retained) = (Arc::clone(&sends), Arc::clone(&retained));
        let directory = fixture.directory.clone();
        move |_call| {
            if sends.fetch_add(1, Ordering::SeqCst) == 0 {
                return acknowledgement(false, false);
            }
            retained.store(directory.exists(), Ordering::SeqCst);
            acknowledgement(true, true)
        }
    });
    let halt = Arc::new(Halt::new(CancellationToken::new()));
    let draining = tokio::spawn({
        let (spool, halt) = (fixture.spool.clone(), Arc::clone(&halt));
        async move { spool.run(&plane, &halt).await }
    });
    let mut calls = Vec::new();
    while calls.len() < 2 {
        let call = tokio::time::timeout(RESEND_WITHIN, received.recv()).await;
        calls.push(call.unwrap().unwrap());
    }
    for _ in 0..REMOVAL_POLLS {
        if !fixture.directory.exists() {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    halt.stop();
    draining.await.unwrap();
    assert!(retained.load(Ordering::SeqCst), "unsettled kept the entry");
    assert!(!fixture.directory.exists(), "settled removed the entry");
    assert_eq!(sends.load(Ordering::SeqCst), 2);
    assert_eq!(calls[0].verb, Verb::AuthoringReport);
    assert_eq!(calls[0], calls[1], "the retry is the same retained report");
}

/// What one send carried, read when the daemon received it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sent {
    text: bool,
    content_on_disk: bool,
}

/// The daemon never answers the first upload; the private-expiry timer, not
/// the daemon, ends it. The resend is the same obligation with no text left
/// to carry, and settling it removes the entry.
#[tokio::test(start_paused = true)]
async fn a_report_the_daemon_never_answers_is_resent_without_text_after_private_expiry() {
    let fixture = Fixture::new();
    fixture.hold().await;
    let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (plane, mut received) = test_support::plane({
        let (sent, directory) = (Arc::clone(&sent), fixture.directory.clone());
        move |call| {
            let body = String::from_utf8_lossy(call.body.as_deref().unwrap_or_default());
            let mut log = sent.lock().unwrap();
            log.push(Sent {
                text: body.contains(super::ANSWER),
                content_on_disk: directory.join(super::files::CONTENT).exists()
                    || directory.join(super::files::KEY).exists(),
            });
            if log.len() == 1 {
                return Answer::Stall;
            }
            acknowledgement(true, false)
        }
    });
    let halt = Arc::new(Halt::new(CancellationToken::new()));
    let draining = tokio::spawn({
        let (spool, halt) = (fixture.spool.clone(), Arc::clone(&halt));
        async move { spool.run(&plane, &halt).await }
    });
    let mut calls = Vec::new();
    while calls.len() < 2 {
        let call = tokio::time::timeout(EXPIRY_WITHIN, received.recv()).await;
        calls.push(call.unwrap().unwrap());
    }
    for _ in 0..REMOVAL_POLLS {
        if !fixture.directory.exists() {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    halt.stop();
    draining.await.unwrap();
    let sent = sent.lock().unwrap().clone();
    assert_eq!(
        sent,
        [
            Sent {
                text: true,
                content_on_disk: true
            },
            Sent {
                text: false,
                content_on_disk: false
            }
        ],
        "the stalled upload carried text; the resend after expiry finds none on disk"
    );
    let reports: Vec<afd_wire::authoring::Report> = calls
        .iter()
        .map(|call| serde_json::from_slice(call.body.as_deref().unwrap()).unwrap())
        .collect();
    assert!(matches!(
        reports[1],
        afd_wire::authoring::Report::Accounting { .. }
    ));
    let accounting =
        |report: &afd_wire::authoring::Report| serde_json::to_value(report.accounting()).unwrap();
    assert_eq!(
        accounting(&reports[0]),
        accounting(&reports[1]),
        "the resend is the same obligation, usage unchanged"
    );
    assert!(!fixture.directory.exists(), "settled removed the entry");
}
