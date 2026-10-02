#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::terminal::{TerminalInput, Write, read_terminal};
use super::{Input as _, leader, pump};
use crate::api::Stream;

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
    assert_eq!(
        leader(None).unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        leader(Some(u32::MAX)).unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe,
        "a pid past the platform's range"
    );
    assert_eq!(leader(Some(1)).unwrap().as_raw_nonzero().get(), 1);
}

#[tokio::test]
async fn a_terminal_whose_writer_stopped_refuses_writes() {
    let (writes, stopped) = std::sync::mpsc::channel();
    drop(stopped);
    let mut input = TerminalInput(writes);

    let refused = input.write(Bytes::from_static(b"x")).await.unwrap_err();

    assert_eq!(refused.kind(), std::io::ErrorKind::BrokenPipe);
}

#[tokio::test]
async fn a_terminal_writer_stops_after_a_failed_write() {
    /// A terminal that refuses every write, as one with nothing on its far
    /// end does.
    struct Refusing;
    impl std::io::Write for Refusing {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut input = TerminalInput::start(Box::new(Refusing)).unwrap();

    let first = input.write(Bytes::from_static(b"x")).await.unwrap_err();
    let after = input.write(Bytes::from_static(b"y")).await.unwrap_err();

    assert_eq!(first.kind(), std::io::ErrorKind::BrokenPipe);
    assert_eq!(
        after.kind(),
        std::io::ErrorKind::BrokenPipe,
        "the writer is gone"
    );
}

#[tokio::test]
async fn a_terminal_writer_that_drops_a_write_unanswered_refuses_it() {
    let (writes, queued) = std::sync::mpsc::channel::<Write>();
    // The writer takes the write and goes away without saying how it went.
    let gone = std::thread::spawn(move || drop(queued.recv()));
    let mut input = TerminalInput(writes);

    let refused = input.write(Bytes::from_static(b"x")).await.unwrap_err();

    gone.join().unwrap();
    assert_eq!(refused.kind(), std::io::ErrorKind::BrokenPipe);
}
