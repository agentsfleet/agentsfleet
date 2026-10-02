//! A served executor and a connected client, and ways to read a process.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test support: a harness that cannot start should fail the test loudly"
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use afr_executor::{Client, Ending, Process, ProcessEvent, Stream};
use tokio::task::JoinHandle;

/// One kibibyte.
pub(crate) const KIB: usize = 1024;
/// One mebibyte.
pub(crate) const MIB: usize = KIB * KIB;

/// How long any one test waits on the executor before failing.
pub(crate) const PATIENCE: Duration = Duration::from_secs(20);

/// An executor serving a scratch workspace, and a client connected to it.
pub(crate) struct Harness {
    pub(crate) client: Client,
    pub(crate) root: PathBuf,
    pub(crate) server: JoinHandle<afr_executor::Result<()>>,
    pub(crate) scratch: tempfile::TempDir,
}

/// Serves an executor on a fresh socket and connects to it.
pub(crate) async fn start() -> Harness {
    let (scratch, socket, root) = scratch();
    let server = serve(&socket, &root);
    let client = connect(&socket).await;
    Harness {
        client,
        root,
        server,
        scratch,
    }
}

/// A scratch directory holding a socket path and a workspace.
pub(crate) fn scratch() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let scratch = tempfile::tempdir().unwrap();
    let socket = scratch.path().join("executor.sock");
    let root = scratch.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    (scratch, socket, root)
}

/// Serves on `socket` in a task of its own.
pub(crate) fn serve(socket: &Path, root: &Path) -> JoinHandle<afr_executor::Result<()>> {
    let (socket, root) = (socket.to_owned(), root.to_owned());
    tokio::spawn(async move { afr_executor::serve(&socket, &root).await })
}

/// Connects once the executor is listening.
pub(crate) async fn connect(socket: &Path) -> Client {
    for _attempt in 0..200 {
        if let Ok(client) = Client::connect(socket).await {
            return client;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the executor never listened on {}", socket.display());
}

/// Everything a process said, by stream, and how it ended.
#[derive(Debug, Default)]
pub(crate) struct Finished {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) terminal: Vec<u8>,
    pub(crate) endings: Vec<(Ending, u64)>,
}

/// Reads a process until its channel closes.
pub(crate) async fn finish(mut process: Process) -> Finished {
    tokio::time::timeout(PATIENCE, async {
        let mut finished = Finished::default();
        while let Some(event) = process.events.recv().await {
            match event {
                ProcessEvent::Output {
                    stream: Stream::Stdout,
                    data,
                } => finished.stdout.extend_from_slice(&data),
                ProcessEvent::Output {
                    stream: Stream::Stderr,
                    data,
                } => finished.stderr.extend_from_slice(&data),
                ProcessEvent::Output {
                    stream: Stream::Terminal,
                    data,
                } => finished.terminal.extend_from_slice(&data),
                ProcessEvent::Ended {
                    ending,
                    omitted_bytes,
                } => finished.endings.push((ending, omitted_bytes)),
            }
        }
        finished
    })
    .await
    .expect("the process's events ended in time")
}

/// Reads output until `wanted` appears in it, keeping the process open.
pub(crate) async fn read_until(process: &mut Process, wanted: &str) -> String {
    tokio::time::timeout(PATIENCE, async {
        let mut seen = Vec::new();
        while let Some(event) = process.events.recv().await {
            if let ProcessEvent::Output { data, .. } = event {
                seen.extend_from_slice(&data);
                let text = String::from_utf8_lossy(&seen).into_owned();
                if text.contains(wanted) {
                    return text;
                }
            }
        }
        panic!("the process ended before saying {wanted:?}");
    })
    .await
    .expect("the process said it in time")
}
