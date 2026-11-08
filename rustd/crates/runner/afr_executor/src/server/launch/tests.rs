#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::io::{Read as _, Write};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::time::Duration;

use bytes::Bytes;
use jsonrpsee_types::error::INTERNAL_ERROR_CODE;
use rustix::process::Pid;
use tokio::sync::mpsc;

use super::terminal::{KillOnDrop, read_terminal, write_terminal};
use super::{leader, pump};
use crate::api::Stream;

/// A program that waits far longer than any test, until it is killed.
const SLEEPER: &str = "sleep";
/// What the sleeper is asked to wait.
const LONG: &str = "30";
/// The signal a kill delivers.
const KILLED: i32 = 9;

#[tokio::test]
async fn a_pipe_reader_stops_once_no_one_takes_its_output() {
    let (sender, receiver) = mpsc::channel(1);
    drop(receiver);

    // The reader never ends, so only the dropped receiver can stop the pump.
    let stopped = tokio::time::timeout(
        Duration::from_secs(5),
        pump(tokio::io::repeat(b'y'), Stream::Stdout, sender),
    )
    .await;

    assert!(
        stopped.is_ok(),
        "the pump stopped instead of reading forever"
    );
}

#[test]
fn a_terminal_reader_stops_once_no_one_takes_its_output() {
    let (sender, receiver) = mpsc::channel(1);
    drop(receiver);
    let (done, finished) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        read_terminal(Box::new(std::io::repeat(b'y')), &sender);
        done.send(()).unwrap();
    });

    assert!(
        finished.recv_timeout(Duration::from_secs(5)).is_ok(),
        "the reader stopped"
    );
}

#[test]
fn a_process_with_no_usable_pid_leads_no_group() {
    assert_eq!(leader(None).unwrap_err().rpc_code(), INTERNAL_ERROR_CODE);
    assert_eq!(
        leader(Some(u32::MAX)).unwrap_err().rpc_code(),
        INTERNAL_ERROR_CODE,
        "a pid past the platform's range"
    );
    assert_eq!(leader(Some(1)).unwrap().as_raw_nonzero().get(), 1);
}

#[test]
fn a_terminal_writer_writes_in_order_until_its_queue_closes() {
    let (mut read, written) = std::io::pipe().unwrap();
    let (writes, queued) = mpsc::channel(4);
    writes.try_send(Bytes::from_static(b"ab")).unwrap();
    writes.try_send(Bytes::from_static(b"c")).unwrap();
    drop(writes);

    write_terminal(Box::new(written), queued);

    let mut seen = String::new();
    read.read_to_string(&mut seen).unwrap();
    assert_eq!(seen, "abc");
}

#[test]
fn a_terminal_writer_stops_at_its_first_failed_write_and_closes_its_queue() {
    /// A terminal that refuses every write, as one with nothing on its far
    /// end does.
    struct Refusing;
    impl Write for Refusing {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let (writes, queued) = mpsc::channel(4);
    writes.try_send(Bytes::from_static(b"x")).unwrap();

    write_terminal(Box::new(Refusing), queued);

    assert!(writes.is_closed(), "later writes find the input closed");
}

#[test]
fn an_abandoned_terminal_wait_kills_the_leaders_group_and_a_finished_one_does_not() {
    let start = || {
        std::process::Command::new(SLEEPER)
            .arg(LONG)
            .process_group(0)
            .spawn()
            .unwrap()
    };
    let group =
        |child: &std::process::Child| Pid::from_raw(i32::try_from(child.id()).unwrap()).unwrap();
    let mut abandoned = start();
    let mut finished = start();

    drop(KillOnDrop(Some(group(&abandoned))));
    KillOnDrop(Some(group(&finished))).disarm();

    assert_eq!(abandoned.wait().unwrap().signal(), Some(KILLED));
    assert!(finished.try_wait().unwrap().is_none(), "left running");
    finished.kill().unwrap();
    finished.wait().unwrap();
}
