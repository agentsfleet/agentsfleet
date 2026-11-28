//! The run's sandbox as a tool's environment: what its executor answers
//! arrives in the harness's own types, one to one.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a harness that cannot start should fail the test loudly"
)]

use std::os::unix::net::UnixListener;

use afr_executor::{Executor, ExecutorSlot, Feed, Process, ProcessId, Sizes, Spawn};
use bytes::Bytes;
use guffbolt::tools::env::{
    DirEntry, Ending, EntryKind, Env, EnvErrorKind, Pattern, ProcessEvent, Search, Stream,
};

use super::{ENDING_GRACE, LazySandboxEnv, SandboxEnv, WAITS_FOR_FIRECRACKER};
use crate::testing::Live;

/// The sizes a stand-in's link holds to, none of them the defaults.
const SIZES: Sizes = Sizes {
    max_read_bytes: 4096,
    edge_bytes: 4,
    max_frame_bytes: 64 * 1024,
};

/// An executor whose one process says what its suite feeds it, kept at
/// [`SIZES`]'s edge.
#[derive(Debug)]
struct Fed(fn(&Feed));

#[async_trait::async_trait]
impl Executor for Fed {
    fn sizes(&self) -> Sizes {
        SIZES
    }

    async fn spawn(&self, _spawn: &Spawn) -> afr_executor::Result<Process> {
        let (feed, events) = afr_executor::Events::keeping(SIZES.edge_bytes);
        (self.0)(&feed);
        feed.end(afr_executor::Ending::Interrupted, true);
        Ok(Process {
            id: ProcessId::new(7),
            events,
        })
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> afr_executor::Result<()> {
        Err(afr_executor::error::unknown_process_refused())
    }

    async fn kill(&self, _process: ProcessId) -> afr_executor::Result<()> {
        Err(afr_executor::error::unknown_process_refused())
    }
}

#[tokio::test]
async fn a_listing_names_each_entry_as_the_harness_kinds_it() {
    let live = Live::start().await;
    std::fs::write(live.root.join("notes.txt"), "hello").unwrap();
    std::fs::create_dir(live.root.join("src")).unwrap();
    std::os::unix::fs::symlink("notes.txt", live.root.join("link")).unwrap();
    let _socket = UnixListener::bind(live.root.join("sock")).unwrap();
    let slot = ExecutorSlot::ready(&live.client);
    let env = LazySandboxEnv::new(&slot, Sizes::DEFAULT);

    let mut listing = env.list_dir(".").await.unwrap();
    listing.entries.sort_by(|a, b| a.name.cmp(&b.name));

    let kinds: Vec<(&str, EntryKind)> = listing
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("link", EntryKind::Symlink),
            ("notes.txt", EntryKind::File),
            ("sock", EntryKind::Other),
            ("src", EntryKind::Directory),
        ]
    );
    assert!(
        listing.entries.contains(&DirEntry {
            name: "notes.txt".to_owned(),
            kind: EntryKind::File,
            size: 5,
        }),
        "{listing:?}"
    );
    assert!(!listing.truncated);
    let missing = env.list_dir("absent").await.unwrap_err();
    assert_eq!(missing.kind(), EnvErrorKind::NotFound);
    live.stop().await;
}

#[tokio::test]
async fn a_search_the_executor_refuses_reads_as_the_environments_refusal() {
    let live = Live::start().await;
    let slot = ExecutorSlot::ready(&live.client);
    let env = LazySandboxEnv::new(&slot, Sizes::DEFAULT);
    let search = Search {
        path: ".".to_owned(),
        glob: None,
        pattern: Some(Pattern {
            text: "fn (".to_owned(),
            literal: false,
            ignore_case: false,
            context: 0,
            line_chars: None,
        }),
        limit: 10,
    };

    let refused = env.search(&search).await.unwrap_err();

    assert_eq!(refused.kind(), EnvErrorKind::Unavailable);
    assert!(refused.to_string().contains("pattern"), "{refused}");
    live.stop().await;
}

#[test]
fn the_sandbox_reads_its_executors_sizes_and_has_no_browser() {
    let executor = Fed(|_feed| {});
    let env = SandboxEnv::new(&executor);

    assert_eq!(env.max_read_bytes(), 4096);
    assert_eq!(env.edge_bytes(), 4);
    assert_eq!(
        env.ending_grace(),
        afr_executor::timing::KILL_GRACE
            + afr_executor::timing::DRAIN_GRACE
            + std::time::Duration::from_secs(1)
    );
    assert_eq!(ENDING_GRACE, env.ending_grace());
    assert_eq!(env.browser().err(), Some(WAITS_FOR_FIRECRACKER));
}

#[tokio::test]
async fn a_process_reads_each_stream_the_output_dropped_unread_and_its_interruption() {
    let executor = Fed(|feed| {
        feed.output(afr_executor::Stream::Stderr, Bytes::from_static(b"oops"));
        feed.output(
            afr_executor::Stream::Terminal,
            Bytes::from_static(b"0123456789"),
        );
    });
    let env = SandboxEnv::new(&executor);
    let spawn = guffbolt::tools::env::Spawn::program("tool");

    let mut process = env.spawn(&spawn).await.unwrap();
    let mut events = Vec::new();
    while let Some(event) = process.events.recv().await {
        events.push(event);
    }

    assert_eq!(process.id.get(), 7);
    assert_eq!(
        events,
        [
            ProcessEvent::Output {
                stream: Stream::Stderr,
                data: Bytes::from_static(b"oops"),
            },
            ProcessEvent::Omitted { bytes: 6 },
            ProcessEvent::Output {
                stream: Stream::Terminal,
                data: Bytes::from_static(b"6789"),
            },
            ProcessEvent::Ended {
                ending: Ending::Interrupted,
                output_abandoned: true,
            },
        ]
    );
}
