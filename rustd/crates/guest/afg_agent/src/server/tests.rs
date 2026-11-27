#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs::{self, File, OpenOptions};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use rustix::process::Signal;
use tokio::io::AsyncReadExt as _;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use af_guest_wire::protocol::{CountParams, Counted, METHOD_COUNT};

use super::host_tests::Host;
use super::launch::Recorded;
use super::{Tenant, bind, write_lines};
use af_guest_wire::Ending;

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
/// processes where the guest agent runs and reads the same kill as reported.
#[test]
fn a_listener_with_a_tenant_places_its_processes_in_the_leaf() {
    let dir = tempfile::tempdir().unwrap();
    let (procs, events, pids_events) = (
        dir.path().join("cgroup.procs"),
        dir.path().join("memory.events"),
        dir.path().join("pids.events"),
    );
    fs::write(&procs, "").unwrap();
    fs::write(&events, NO_KILLS).unwrap();
    fs::write(&pids_events, "max 0\n").unwrap();
    let tenant = Tenant::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&procs)
            .unwrap()
            .into(),
        File::open(&events).unwrap().into(),
        File::open(&pids_events).unwrap().into(),
        Recorded::shared(),
    )
    .unwrap();
    let bare = bind(&dir.path().join("bare.sock")).unwrap();
    let tenanted = bind(&dir.path().join("tenant.sock"))
        .unwrap()
        .with_tenant(Arc::new(tenant));
    fs::write(&events, ONE_KILL).unwrap();
    let killed = Ending::Signaled(Signal::KILL.as_raw());

    assert_eq!(tenanted.placement.judge(killed), Ending::OutOfMemory);
    assert_eq!(bare.placement.judge(killed), killed);
}

/// The host reads the forks the tenant leaf refused at its process cap over
/// the agent's own link, from the leaf's `pids.events`; a bare listener caps
/// nothing and refuses none.
#[tokio::test]
async fn the_host_reads_the_forks_the_leaf_refused_at_its_process_cap() {
    let dir = tempfile::tempdir().unwrap();
    let (procs, events, pids_events) = (
        dir.path().join("cgroup.procs"),
        dir.path().join("memory.events"),
        dir.path().join("pids.events"),
    );
    fs::write(&procs, "").unwrap();
    fs::write(&events, NO_KILLS).unwrap();
    fs::write(&pids_events, "max 0\nmax.imposed 0\n").unwrap();
    let tenant = Tenant::new(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&procs)
            .unwrap()
            .into(),
        File::open(&events).unwrap().into(),
        File::open(&pids_events).unwrap().into(),
        Recorded::shared(),
    )
    .unwrap();
    let root = dir.path().join("w");
    fs::create_dir(&root).unwrap();
    let mut refused = Vec::new();
    for (name, tenant) in [("tenant.sock", Some(tenant)), ("bare.sock", None)] {
        let socket = dir.path().join(name);
        let bound = bind(&socket).unwrap();
        let server = match tenant {
            Some(tenant) => bound.with_tenant(Arc::new(tenant)),
            None => bound,
        };
        let root = root.clone();
        let serving = tokio::spawn(async move { server.serve(&root).await });
        let mut host = Host::connect(&socket).await;
        fs::write(&pids_events, "max 3\nmax.imposed 0\n").unwrap();
        let counted = CountParams {
            count: Counted::PidsRefused,
        };
        let answer = host.ask(METHOD_COUNT, &counted).await;
        refused.push(answer.get("result").and_then(serde_json::Value::as_u64));
        drop(host);
        tokio::time::timeout(PATIENCE, serving)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    assert_eq!(refused, [Some(3), Some(0)]);
}
