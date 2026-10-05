#![expect(
    clippy::unwrap_used,
    reason = "test module: a channel the test holds both ends of cannot be closed"
)]

use std::time::Duration;

use afr_executor::{Ending, Process, ProcessEvent, ProcessId, Stream};
use bytes::Bytes;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::time::Instant;

use super::{Collected, budget, error_code, exit_code, status, with_line};
use crate::runtime::ToolErrorCode;

/// Output a process prints and keeps running after.
const STILL_GOING: &str = "still going";

/// A process whose channel already carries `events`, and the sender that
/// keeps the channel open.
fn process(events: Vec<ProcessEvent>) -> (Process, UnboundedSender<ProcessEvent>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    for event in events {
        sender.send(event).unwrap();
    }
    let process = Process {
        id: ProcessId::new(1),
        events: receiver,
    };
    (process, sender)
}

fn said(text: &str) -> ProcessEvent {
    ProcessEvent::Output {
        stream: Stream::Stdout,
        data: Bytes::copy_from_slice(text.as_bytes()),
    }
}

fn ended(ending: Ending, omitted_bytes: u64) -> ProcessEvent {
    ProcessEvent::Ended {
        ending,
        omitted_bytes,
    }
}

/// What `events`, read to their end, leave collected, and how they ended.
async fn read(events: Vec<ProcessEvent>) -> (Collected, Ending) {
    let (mut process, sender) = process(events);
    drop(sender);
    let mut collected = Collected::default();
    let ending = collected.read_to_end(&mut process).await;
    (collected, ending)
}

#[tokio::test]
async fn should_keep_output_whole_and_in_order_when_it_fits() {
    let (collected, ending) = read(vec![
        said("one\n"),
        said("two\n"),
        ended(Ending::Exited(0), 0),
    ])
    .await;

    assert_eq!(ending, Ending::Exited(0));
    assert_eq!(collected.text(8), "one\ntwo\n");
}

#[tokio::test]
async fn should_cut_the_middle_and_count_what_it_dropped() {
    let (collected, _ending) = read(vec![said("abcdefghij"), ended(Ending::Exited(0), 0)]).await;

    assert_eq!(collected.text(4), "ab\n... 6 bytes omitted ...\nij");
}

#[tokio::test]
async fn should_cut_on_character_boundaries() {
    // Five two-byte characters: no budget may split one.
    let (collected, _ending) = read(vec![said("ééééé"), ended(Ending::Exited(0), 0)]).await;

    assert_eq!(collected.text(5), "é\n... 6 bytes omitted ...\né");
}

#[tokio::test]
async fn should_count_the_bytes_the_executor_dropped_with_its_own() {
    let (collected, _ending) = read(vec![
        said("head"),
        said("tail"),
        ended(Ending::Exited(0), 7),
    ])
    .await;

    assert_eq!(
        collected.text(100),
        "headtail\n... 7 bytes omitted ...",
        "output that fits carries the executor's count last"
    );
    assert_eq!(
        collected.text(4),
        "he\n... 11 bytes omitted ...\nil",
        "both cuts are counted"
    );
}

#[tokio::test]
async fn should_leave_only_the_marker_for_a_zero_budget() {
    let (collected, _ending) = read(vec![said("abc"), ended(Ending::Exited(0), 0)]).await;

    assert_eq!(collected.text(0), "... 3 bytes omitted ...");
}

#[tokio::test]
async fn should_end_interrupted_when_the_channel_closes_without_an_ending() {
    let (collected, ending) = read(vec![said("partial")]).await;

    assert_eq!(ending, Ending::Interrupted);
    assert_eq!(collected.text(100), "partial");
}

#[tokio::test(start_paused = true)]
async fn should_answer_none_once_the_deadline_passes_while_the_process_runs() {
    let (mut process, _sender) = process(vec![said(STILL_GOING)]);
    let mut collected = Collected::default();
    let started = Instant::now();

    let ended = collected
        .until(&mut process, started + Duration::from_millis(250))
        .await;

    assert_eq!(ended, None);
    assert_eq!(started.elapsed(), Duration::from_millis(250));
    assert_eq!(collected.text(100), STILL_GOING);
}

#[tokio::test(start_paused = true)]
async fn should_answer_an_ending_before_the_deadline_without_waiting_it_out() {
    let (mut process, _sender) = process(vec![said("done"), ended(Ending::Exited(2), 0)]);
    let mut collected = Collected::default();
    let started = Instant::now();

    let ended = collected
        .until(&mut process, started + Duration::from_secs(10))
        .await;

    assert_eq!(ended, Some(Ending::Exited(2)));
    assert_eq!(started.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn should_answer_interrupted_when_the_channel_closes_before_the_deadline() {
    let (mut process, sender) = process(Vec::new());
    drop(sender);

    let ended = Collected::default()
        .until(&mut process, Instant::now() + Duration::from_secs(1))
        .await;

    assert_eq!(ended, Some(Ending::Interrupted));
}

#[test]
fn should_take_only_what_already_arrived() {
    let (mut process, sender) = process(vec![said("a")]);
    let mut collected = Collected::default();

    assert_eq!(collected.arrived(&mut process), None, "still running");
    sender.send(ended(Ending::Exited(0), 0)).unwrap();
    assert_eq!(collected.arrived(&mut process), Some(Ending::Exited(0)));
    assert_eq!(collected.text(100), "a");
    drop(sender);
    assert_eq!(
        Collected::default().arrived(&mut process),
        Some(Ending::Interrupted),
        "a channel the executor closed"
    );
}

#[test]
fn should_put_a_line_on_a_line_of_its_own() {
    assert_eq!(with_line(String::new(), "end"), "end");
    assert_eq!(with_line("out".to_owned(), "end"), "out\nend");
    assert_eq!(with_line("out\n".to_owned(), "end"), "out\nend");
}

#[test]
fn should_budget_four_bytes_a_token() {
    assert_eq!(budget(None), 40_000);
    assert_eq!(budget(Some(3)), 12);
    assert_eq!(
        budget(Some(usize::MAX)),
        usize::MAX,
        "saturates, never wraps"
    );
}

#[test]
fn should_word_each_ending_as_codex_does() {
    // pin test: literal is the contract
    assert_eq!(status(Ending::Exited(3)), "Process exited with code 3");
    assert_eq!(status(Ending::Signaled(9)), "Process killed by signal 9");
    assert_eq!(status(Ending::TimedOut), "Process timed out");
    assert_eq!(
        status(Ending::Interrupted),
        "Process interrupted before its ending arrived"
    );
}

#[test]
fn should_report_a_signal_as_a_shell_does_and_no_status_without_one() {
    assert_eq!(exit_code(Ending::Exited(3)), Some(3));
    assert_eq!(exit_code(Ending::Signaled(9)), Some(137));
    assert_eq!(
        exit_code(Ending::Signaled(i32::MAX)),
        Some(i32::MAX),
        "saturates, never wraps"
    );
    assert_eq!(exit_code(Ending::TimedOut), None);
    assert_eq!(exit_code(Ending::Interrupted), None);
}

#[test]
fn should_fail_only_a_process_that_did_not_end_by_itself() {
    assert_eq!(error_code(Ending::Exited(1)), None);
    assert_eq!(error_code(Ending::Signaled(9)), None);
    assert_eq!(error_code(Ending::TimedOut), Some(ToolErrorCode::TimedOut));
    assert_eq!(
        error_code(Ending::Interrupted),
        Some(ToolErrorCode::Interrupted)
    );
}
