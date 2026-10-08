#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use rustix::process::{Pid, Signal};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Level;

use super::super::files::Workspace;
use super::super::launch::{Inherit, Placement, Plan};
use super::{
    DRAIN_BYTES_MAX, DRAIN_GRACE, EVENT_OUTPUT_ABANDONED, EVENT_PROCESS_COMPLETED,
    EVENT_PROCESS_FAILED, EVENT_SIGNAL_MISSED, Group, ProcessRun, drain, report,
};
use crate::api::{Ending, Stream};
use crate::edges::Chunk;
use crate::protocol::{NOTIFY_EXITED, READ_CHUNK_BYTES, SpawnParams};

/// A program that writes for as long as its output is taken.
pub(super) const YES: &str = "/usr/bin/yes";
/// The shell the scripted processes run under.
const SH: &str = "/bin/sh";
/// The lines the writer's queue holds in the backpressure test.
pub(super) const QUEUED: usize = 2;
/// Longer than any wait below takes when it works.
pub(super) const PATIENCE: Duration = Duration::from_secs(10);
/// Long enough for a driver that should stay blocked to show that it has.
const GLANCE: Duration = Duration::from_millis(200);

/// A started process running `argv` with no environment, and the workspace
/// it runs in, kept for as long as the test holds it.
fn started(argv: &[&str]) -> (ProcessRun, tempfile::TempDir) {
    started_within(argv, None)
}

/// [`started`], with the command deadline `timeout_ms` names.
pub(super) fn started_within(
    argv: &[&str],
    timeout_ms: Option<u64>,
) -> (ProcessRun, tempfile::TempDir) {
    started_placed(argv, timeout_ms, &(Arc::new(Inherit) as Arc<dyn Placement>))
}

/// [`started_within`], placed by `placement`.
fn started_placed(
    argv: &[&str],
    timeout_ms: Option<u64>,
    placement: &Arc<dyn Placement>,
) -> (ProcessRun, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::open(root.path()).unwrap();
    let params = SpawnParams {
        argv: Cow::Owned(argv.iter().map(|arg| (*arg).to_owned()).collect()),
        cwd: None,
        env: Cow::Owned(BTreeMap::new()),
        pty: false,
        timeout_ms,
    };
    let (run, _input) =
        ProcessRun::start(&Plan::new(params, &workspace).unwrap(), placement).unwrap();
    (run, root)
}

/// A placement that reads every ending it is asked about as the kernel's
/// out-of-memory kill.
#[derive(Debug)]
struct AlwaysOutOfMemory;

impl Placement for AlwaysOutOfMemory {
    fn check(&self) -> crate::error::Result<()> {
        Ok(())
    }

    fn enter(&self) -> std::io::Result<()> {
        Ok(())
    }

    fn judge(&self, _ending: Ending) -> Ending {
        Ending::OutOfMemory
    }
}

/// Drives `run` until it ends, `stop` as its stop token, and answers with
/// the ending its `process/exited` reported.
async fn reported_ending(run: ProcessRun, stop: CancellationToken) -> Option<Ending> {
    let (lines, mut written) = mpsc::channel(QUEUED);
    let driving = tokio::spawn(run.drive(1, stop, lines));
    let mut ending = None;
    while let Some(line) = written.recv().await {
        let message: Value = serde_json::from_slice(&line).unwrap();
        if message["method"] == NOTIFY_EXITED {
            ending = serde_json::from_value(message["params"]["ending"].clone()).ok();
        }
    }
    driving.await.unwrap();
    ending
}

/// One chunk of `bytes` bytes, as the reader hands it on.
fn chunk(bytes: usize) -> Chunk {
    Chunk {
        stream: Stream::Stdout,
        data: Bytes::from(vec![b'y'; bytes]),
    }
}

/// How many lines reached the writer, once it is the only one left.
async fn lines_written(lines: mpsc::Sender<Bytes>, mut written: mpsc::Receiver<Bytes>) -> usize {
    drop(lines);
    let mut count = 0;
    while written.recv().await.is_some() {
        count += 1;
    }
    count
}

/// A driver no one takes output from waits on the writer, never dropping a
/// line and never queueing past the bound, and the process waits on its full
/// pipe; once the writer goes, the driver ends.
#[tokio::test]
async fn output_no_one_takes_holds_the_process_until_the_writer_goes() {
    let (run, _root) = started(&[YES]);
    let (lines, untaken) = mpsc::channel(QUEUED);
    let stop = CancellationToken::new();
    let driving = tokio::spawn(run.drive(7, stop.clone(), lines));

    tokio::time::timeout(PATIENCE, async {
        while untaken.len() < QUEUED {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(GLANCE).await;

    assert!(!driving.is_finished(), "the driver waits on the writer");
    stop.cancel();
    drop(untaken);
    let ended = tokio::time::timeout(PATIENCE, driving).await.unwrap();
    assert_eq!(ended.unwrap(), 7, "the writer gone, the stop lands");
}

#[test]
fn an_ending_with_no_status_is_logged_failed_and_any_other_completed() {
    let capture = Capture::install();

    report(3, Ending::Interrupted);
    report(4, Ending::Exited(0));

    let failed = capture.only(EVENT_PROCESS_FAILED);
    let completed = capture.only(EVENT_PROCESS_COMPLETED);
    assert_eq!(failed.level, Level::WARN);
    assert_eq!(failed.field("error_code"), Some("UZ-INTERNAL-003"));
    assert_eq!(failed.field("process_id"), Some("3"));
    assert_eq!(completed.level, Level::DEBUG);
    assert_eq!(completed.field("ending"), Some("exited"));
    assert_eq!(completed.field("code"), Some("0"));
}

#[test]
fn an_ending_is_logged_under_the_kind_the_wire_spells() {
    for ending in [
        Ending::Exited(2),
        Ending::Signaled(9),
        Ending::TimedOut,
        Ending::OutOfMemory,
        Ending::Interrupted,
    ] {
        let wire = serde_json::to_value(ending).unwrap();

        assert_eq!(wire["kind"], ending.kind(), "{ending:?}");
        assert_eq!(
            wire.get("code").and_then(Value::as_i64),
            ending.code().map(i64::from),
            "{ending:?}"
        );
    }
}

/// Only an ending a process came to by itself is judged: a kill or a timeout
/// the executor sent stays the executor's, even under a placement that would
/// read every ending as out of memory.
#[tokio::test]
async fn an_ending_the_executor_caused_is_never_read_as_out_of_memory() {
    let judging: Arc<dyn Placement> = Arc::new(AlwaysOutOfMemory);
    let (by_itself, _first) = started_placed(&[SH, "-c", "exit 0"], None, &judging);
    let (killed, _second) = started_placed(&[SH, "-c", "sleep 30"], None, &judging);
    let (timed_out, _third) = started_placed(&[SH, "-c", "sleep 30"], Some(50), &judging);
    let kill = CancellationToken::new();
    kill.cancel();

    let endings = [
        reported_ending(by_itself, CancellationToken::new()).await,
        reported_ending(killed, kill).await,
        reported_ending(timed_out, CancellationToken::new()).await,
    ];

    assert_eq!(
        endings,
        [
            Some(Ending::OutOfMemory),
            Some(Ending::Signaled(Signal::TERM.as_raw())),
            Some(Ending::TimedOut),
        ],
        "only the ending the process came to by itself is judged"
    );
}

#[test]
fn a_signal_to_a_group_already_gone_is_logged_with_its_process() {
    let capture = Capture::install();
    // Past every platform's process-number range, so never a live group.
    let group = Group {
        process: 5,
        pid: Pid::from_raw(i32::MAX).unwrap(),
    };

    group.signal(Signal::TERM);

    let missed = capture.only(EVENT_SIGNAL_MISSED);
    assert_eq!(missed.field("process_id"), Some("5"));
    assert!(missed.field("reason").is_some());
}

/// The output a process left behind when its leader ended reaches the
/// writer whole, however slowly the supervisor reads: the drain's grace is
/// for a pipe held open, not for the writer.
#[tokio::test]
async fn leftover_output_reaches_a_slow_writer_whole() {
    const SAID: usize = 300_000;
    const LINE_PACE: Duration = Duration::from_millis(150);
    let script = format!("head -c {SAID} /dev/zero");
    let (run, _root) = started(&[SH, "-c", &script]);
    let (lines, mut written) = mpsc::channel(1);
    let driving = tokio::spawn(run.drive(3, CancellationToken::new(), lines));

    let mut forwarded = 0;
    let mut last = None;
    while let Some(line) = written.recv().await {
        let message: Value = serde_json::from_slice(&line).unwrap();
        if let Some(data) = message["params"]["data"].as_str() {
            forwarded += BASE64_STANDARD.decode(data).unwrap().len();
        }
        last = message["method"].as_str().map(str::to_owned);
        tokio::time::sleep(LINE_PACE).await;
    }

    assert_eq!(forwarded, SAID, "every byte left in the pipe was forwarded");
    assert_eq!(last.as_deref(), Some("process/exited"));
    assert_eq!(
        tokio::time::timeout(PATIENCE, driving)
            .await
            .unwrap()
            .unwrap(),
        3
    );
}

/// A descendant that keeps writing after the leader ended is forwarded up to
/// the cap, not a chunk more, and then given up on.
#[tokio::test]
async fn the_drain_stops_at_its_cap_while_output_keeps_coming() {
    let pieces = DRAIN_BYTES_MAX / READ_CHUNK_BYTES;
    let (said, mut output) = mpsc::channel(pieces + 1);
    for _ in 0..=pieces {
        said.send(chunk(READ_CHUNK_BYTES)).await.unwrap();
    }
    let (lines, written) = mpsc::channel(pieces + 2);

    let closed = drain(&mut output, &lines, 1, None).await;

    assert!(!closed, "still writing at the cap, so given up on");
    assert_eq!(lines_written(lines, written).await, pieces);
    drop(said);
}

/// Output that closes ends the drain closed, with every chunk forwarded.
#[tokio::test]
async fn the_drain_ends_closed_once_the_output_closes() {
    let (said, mut output) = mpsc::channel(2);
    said.send(chunk(3)).await.unwrap();
    said.send(chunk(5)).await.unwrap();
    drop(said);
    let (lines, written) = mpsc::channel(4);

    assert!(drain(&mut output, &lines, 1, None).await);
    assert_eq!(lines_written(lines, written).await, 2);
}

/// Output held open and silent is waited for as long as the grace, then
/// given up on.
#[tokio::test]
async fn the_drain_gives_up_on_silent_output_held_open_past_its_grace() {
    let (said, mut output) = mpsc::channel::<Chunk>(1);
    let (lines, _written) = mpsc::channel(1);
    let started = tokio::time::Instant::now();

    assert!(!drain(&mut output, &lines, 1, None).await);
    assert!(started.elapsed() >= DRAIN_GRACE, "{:?}", started.elapsed());
    drop(said);
}

/// A process whose output closed when it ended is logged completed, never
/// as output abandoned.
#[tokio::test]
async fn a_process_whose_output_closes_is_not_logged_abandoned() {
    let capture = Capture::install();
    let (run, _root) = started(&[SH, "-c", "exit 0"]);
    let (lines, _written) = mpsc::channel(4);

    let ended = run.drive(4, CancellationToken::new(), lines).await;

    assert_eq!(ended, 4);
    assert_eq!(
        capture.only(EVENT_PROCESS_COMPLETED).field("ending"),
        Some("exited")
    );
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field("event") != Some(EVENT_OUTPUT_ABANDONED)),
        "{:?}",
        capture.events()
    );
}
