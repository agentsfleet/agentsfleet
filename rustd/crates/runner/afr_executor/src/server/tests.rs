#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs::{self, File, OpenOptions};
use std::time::Duration;

use bytes::Bytes;
use rustix::process::Signal;
use tokio::io::AsyncReadExt as _;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use super::{Tenant, bind, write_lines};
use crate::api::Ending;

/// Longer than a writer takes to give up on a dead socket.
const PATIENCE: Duration = Duration::from_secs(5);
/// The leaf's `memory.events`, before and after the kernel kills a process
/// in it for memory.
const NO_KILLS: &str = "oom 0\noom_kill 0\n";
const ONE_KILL: &str = "oom 1\noom_kill 1\n";

/// What the writer puts on the socket for what was queued, once both queues
/// close.
async fn written(answers: &[&'static [u8]], output: &[&'static [u8]]) -> Vec<u8> {
    let (ours, theirs) = UnixStream::pair().unwrap();
    let (_read, write) = ours.into_split();
    let (answer, answered) = mpsc::unbounded_channel();
    let (say, said) = mpsc::channel(output.len().max(1));
    for line in output {
        say.send(Bytes::from_static(line)).await.unwrap();
    }
    for line in answers {
        answer.send(Bytes::from_static(line)).unwrap();
    }
    drop((answer, say));

    write_lines(write, answered, said).await;
    let mut seen = Vec::new();
    let mut theirs = theirs;
    theirs.read_to_end(&mut seen).await.unwrap();
    seen
}

#[tokio::test]
async fn an_answer_queued_alongside_output_is_written_first() {
    let seen = written(&[b"spawned\n"], &[b"output\n", b"exited\n"]).await;

    assert_eq!(seen, b"spawned\noutput\nexited\n");
}

#[tokio::test]
async fn output_keeps_the_order_it_was_said_in() {
    let seen = written(&[], &[b"one\n", b"two\n", b"three\n"]).await;

    assert_eq!(seen, b"one\ntwo\nthree\n");
}

/// A supervisor that hung up ends the writer at its next line, though the
/// session still holds both queues open.
#[tokio::test]
async fn a_writer_whose_socket_closed_stops_though_its_queues_stay_open() {
    let (ours, theirs) = UnixStream::pair().unwrap();
    drop(theirs);
    let (_read, write) = ours.into_split();
    let (answer, answered) = mpsc::unbounded_channel();
    let (_say, said) = mpsc::channel::<Bytes>(1);
    for line in [&b"lost\n"[..], b"also lost\n"] {
        answer.send(Bytes::from_static(line)).unwrap();
    }

    let stopped = tokio::time::timeout(PATIENCE, write_lines(write, answered, said)).await;

    assert!(stopped.is_ok(), "the failed write ended the writer");
    drop(answer);
}

/// A listener given a tenant places what it starts in the tenant's leaf, so
/// a kill the leaf counted reads as out of memory; a bare listener keeps
/// processes where the executor runs and reads the same kill as reported.
#[test]
fn a_listener_with_a_tenant_places_its_processes_in_the_leaf() {
    let dir = tempfile::tempdir().unwrap();
    let (procs, events) = (
        dir.path().join("cgroup.procs"),
        dir.path().join("memory.events"),
    );
    fs::write(&procs, "").unwrap();
    fs::write(&events, NO_KILLS).unwrap();
    let tenant = Tenant::new(
        OpenOptions::new().write(true).open(&procs).unwrap().into(),
        File::open(&events).unwrap().into(),
    )
    .unwrap();
    let bare = bind(&dir.path().join("bare.sock")).unwrap();
    let tenanted = bind(&dir.path().join("tenant.sock"))
        .unwrap()
        .with_tenant(tenant);
    fs::write(&events, ONE_KILL).unwrap();
    let killed = Ending::Signaled(Signal::KILL.as_raw());

    assert_eq!(tenanted.placement.judge(killed), Ending::OutOfMemory);
    assert_eq!(bare.placement.judge(killed), killed);
}
