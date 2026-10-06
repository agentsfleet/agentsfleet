use std::time::Duration;

use afr_executor::{EDGE_BYTES, Ending, Events, Feed, Process, ProcessId, Stream};
use bytes::Bytes;
use tokio::time::Instant;

use super::{Collected, budget, error_code, exit_code, status, with_line};
use crate::runtime::ToolErrorCode;

/// Output a process prints and keeps running after.
const STILL_GOING: &str = "still going";
/// Output a process prints and then ends.
const DONE: &str = "done";
/// One kibibyte.
const KIB: usize = 1024;

/// A process that already said `output`, and the feed that keeps it open.
fn process(output: &[&str]) -> (Process, Feed) {
    let (feed, events) = Events::channel();
    for text in output {
        feed.output(Stream::Stdout, Bytes::copy_from_slice(text.as_bytes()));
    }
    let process = Process {
        id: ProcessId::new(1),
        events,
    };
    (process, feed)
}

/// What `output` leaves collected once read to its end, which is `ending`
/// or, for none, events that finished without one.
async fn read(output: &[&str], ending: Option<Ending>) -> (Collected, Ending) {
    let (mut process, feed) = process(output);
    match ending {
        Some(ending) => feed.end(ending, false),
        None => drop(feed),
    }
    let mut collected = Collected::default();
    let ending = collected.read_to_end(&mut process).await;
    (collected, ending)
}

#[tokio::test]
async fn should_keep_output_whole_and_in_order_when_it_fits() {
    let (collected, ending) = read(&["one\n", "two\n"], Some(Ending::Exited(0))).await;

    assert_eq!(ending, Ending::Exited(0));
    assert_eq!(collected.text(8), "one\ntwo\n");
}

#[tokio::test]
async fn should_cut_the_middle_and_count_what_it_dropped() {
    let (collected, _ending) = read(&["abcdefghij"], Some(Ending::Exited(0))).await;

    assert_eq!(collected.text(4), "ab\n... 6 bytes omitted ...\nij");
}

#[tokio::test]
async fn should_cut_on_character_boundaries() {
    // Five two-byte characters: no budget may split one.
    let (collected, _ending) = read(&["ééééé"], Some(Ending::Exited(0))).await;

    assert_eq!(collected.text(5), "é\n... 6 bytes omitted ...\né");
}

#[tokio::test]
async fn should_count_the_bytes_dropped_unread_with_its_own() {
    // Read only at the end, so the middle of three edges is dropped before
    // the call sees it.
    let (a, b) = ("a".repeat(EDGE_BYTES), "b".repeat(2 * EDGE_BYTES));
    let (collected, _ending) = read(&[&a, &b], Some(Ending::Exited(0))).await;

    let hidden = a.len() + b.len() - 4;
    assert_eq!(
        collected.text(4),
        format!("aa\n... {hidden} bytes omitted ...\nbb"),
        "every byte is shown or counted"
    );
}

#[tokio::test]
async fn should_leave_only_the_marker_for_a_zero_budget() {
    let (collected, _ending) = read(&["abc"], Some(Ending::Exited(0))).await;

    assert_eq!(collected.text(0), "... 3 bytes omitted ...");
}

#[tokio::test]
async fn should_end_interrupted_when_the_events_finish_without_an_ending() {
    let (collected, ending) = read(&["partial"], None).await;

    assert_eq!(ending, Ending::Interrupted);
    assert_eq!(collected.text(100), "partial");
}

#[tokio::test(start_paused = true)]
async fn should_answer_none_once_the_deadline_passes_while_the_process_runs() {
    let (mut process, _feed) = process(&[STILL_GOING]);
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
    let (mut process, feed) = process(&[DONE]);
    feed.end(Ending::Exited(2), false);
    let mut collected = Collected::default();
    let started = Instant::now();

    let ended = collected
        .until(&mut process, started + Duration::from_secs(10))
        .await;

    assert_eq!(ended, Some(Ending::Exited(2)));
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert_eq!(collected.text(100), DONE);
}

#[tokio::test(start_paused = true)]
async fn should_answer_interrupted_when_the_events_finish_before_the_deadline() {
    let (mut process, feed) = process(&[]);
    drop(feed);

    let ended = Collected::default()
        .until(&mut process, Instant::now() + Duration::from_secs(1))
        .await;

    assert_eq!(ended, Some(Ending::Interrupted));
}

#[tokio::test(start_paused = true)]
async fn should_hold_at_most_the_edges_of_what_arrived_while_it_waited() {
    let (mut process, feed) = process(&[]);
    let mut collected = Collected::default();
    let deadline = Instant::now() + Duration::from_millis(250);
    let flood = tokio::spawn(async move {
        for _ in 0..64 {
            feed.output(Stream::Stdout, Bytes::from(vec![b'y'; 64 * KIB]));
            tokio::task::yield_now().await;
        }
        feed
    });

    let ended = collected.until(&mut process, deadline).await;

    assert_eq!(ended, None);
    assert_eq!(
        collected.bytes.len(),
        2 * EDGE_BYTES,
        "four MiB said, one kept"
    );
    assert_eq!(
        collected.gaps.iter().map(|(_at, bytes)| bytes).sum::<u64>(),
        (6 * EDGE_BYTES) as u64,
        "the six edges between"
    );
    drop(flood.await);
}

#[test]
fn should_take_only_what_already_arrived() {
    let (mut process, feed) = process(&["a"]);
    let mut collected = Collected::default();

    assert_eq!(collected.arrived(&mut process), None, "still running");
    feed.end(Ending::Exited(0), false);
    assert_eq!(collected.arrived(&mut process), Some(Ending::Exited(0)));
    assert_eq!(collected.text(100), "a");
    assert_eq!(
        Collected::default().arrived(&mut process),
        Some(Ending::Interrupted),
        "events that finished, their ending read"
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
        2 * EDGE_BYTES,
        "capped at the two edges a call can hold"
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

/// Output that fits shows a marker where each gap fell; output that does not
/// joins every gap into the one marker between its halves.
#[test]
fn should_put_a_marker_where_each_gap_fell_when_the_output_fits() {
    let collected = Collected {
        bytes: b"headtail".to_vec(),
        gaps: vec![(4, 7)],
        abandoned: false,
    };

    assert_eq!(collected.text(100), "head\n... 7 bytes omitted ...\ntail");
    assert_eq!(collected.text(4), "he\n... 11 bytes omitted ...\nil");
    let opens_on_a_gap = Collected {
        bytes: b"tail".to_vec(),
        gaps: vec![(0, 7)],
        abandoned: false,
    };
    assert_eq!(opens_on_a_gap.text(100), "... 7 bytes omitted ...\ntail");
    let ends_on_a_gap = Collected {
        bytes: b"head".to_vec(),
        gaps: vec![(4, 7)],
        abandoned: false,
    };
    assert_eq!(ends_on_a_gap.text(100), "head\n... 7 bytes omitted ...\n");
    let two = Collected {
        bytes: b"abc".to_vec(),
        gaps: vec![(1, 2), (2, 3)],
        abandoned: false,
    };
    assert_eq!(
        two.text(100),
        "a\n... 2 bytes omitted ...\nb\n... 3 bytes omitted ...\nc"
    );
}

#[test]
fn should_say_last_that_output_was_left_behind() {
    let collected = Collected {
        bytes: b"out\n".to_vec(),
        gaps: Vec::new(),
        abandoned: true,
    };

    assert_eq!(
        collected.text(100),
        "out\n... the process ended with its output still open; what was written after is not shown ..."
    );
}
