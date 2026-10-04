//! Dimensions 5.1 and 5.2: a recall the window cannot fill asks `agentsfleetd`,
//! once per miss and never past the cap, and merges without duplicates.
#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::error_code;
use afd_wire::memory::{
    MemoryDelta, MemoryRecallResponse, PINNED_CATEGORY, SharedMemory, Visibility,
};

use super::{Hydrated, MemoryBackend, RECALL_MISS_CAP, Recall, Seed};
use crate::Result;
use crate::tests::keys;

/// The limit every recall here asks for.
const LIMIT: usize = 5;

/// The other fleet in the workspace, whose name a reader sees on what it shared.
const WRITER: &str = "incident-fleet-3";

fn entry(key: &str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(format!("deploy note {key}")),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Fleet,
    }
}

/// What [`WRITER`] published under `key`, as the daemon answers it to a reader.
fn shared(key: &str) -> SharedMemory<'static> {
    SharedMemory {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(format!("deploy note {key}, from {WRITER}")),
        category: Cow::Borrowed(PINNED_CATEGORY),
        writer_fleet_id: Cow::Borrowed("0199a1f0-0000-7000-8000-000000000003"),
        writer_fleet_name: Cow::Borrowed(WRITER),
        updated_at: 0,
    }
}

/// A daemon that answers every recall with `found`, or refuses, counting asks.
#[derive(Debug, Default)]
struct Daemon {
    asked: AtomicUsize,
    refuses: bool,
    /// A key [`WRITER`] shared with the workspace, answered beside the fleet's
    /// own entries.
    shares: Option<&'static str>,
}

#[async_trait::async_trait]
impl Recall for Daemon {
    async fn recall(&self, _query: &str, _limit: usize) -> Result<MemoryRecallResponse<'static>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        if self.refuses {
            return Err(crate::Error::unanswered(error_code::MEM_UNAVAILABLE));
        }
        // `deploy-1` is the window's own entry: the merge must not repeat it.
        Ok(MemoryRecallResponse {
            memory: vec![entry("deploy-1"), entry("deploy-7"), entry("deploy-9")],
            shared: self.shares.map(shared).into_iter().collect(),
        })
    }
}

#[tokio::test]
async fn test_recall_miss_asks_agentsfleetd_once() {
    let window = vec![entry("deploy-1")];
    let daemon = Daemon::default();
    let memory = Hydrated::new(Seed {
        recall: Some(&daemon),
        ..Seed::window(&window)
    });

    let found = memory.recall("deploy", LIMIT).await.unwrap();

    assert_eq!(daemon.asked.load(Ordering::SeqCst), 1, "one miss, one ask");
    let keys: Vec<_> = found.iter().map(|hit| hit.key.as_ref()).collect();
    assert_eq!(
        keys,
        ["deploy-1", "deploy-7", "deploy-9"],
        "merged without the duplicate"
    );
}

#[tokio::test]
async fn test_recall_miss_cap_answers_from_the_window() {
    let window = vec![entry("deploy-1")];
    let daemon = Daemon::default();
    let memory = Hydrated::new(Seed {
        recall: Some(&daemon),
        ..Seed::window(&window)
    });
    for _miss in 0..RECALL_MISS_CAP {
        memory.recall("deploy", LIMIT).await.unwrap();
    }

    let past_cap = memory.recall("deploy", LIMIT).await.unwrap();

    assert_eq!(
        daemon.asked.load(Ordering::SeqCst),
        RECALL_MISS_CAP,
        "no ask past the cap"
    );
    let keys: Vec<_> = past_cap.iter().map(|hit| hit.key.as_ref()).collect();
    assert_eq!(keys, ["deploy-1"], "the window's answer");
}

#[tokio::test]
async fn a_recall_past_the_window_never_brings_back_a_key_the_run_forgot() {
    let window = vec![entry("deploy-1")];
    let daemon = Daemon::default();
    let mut memory = Hydrated::new(Seed {
        recall: Some(&daemon),
        ..Seed::window(&window)
    });
    memory.forget("deploy-1").await.unwrap();
    // Past the window too: the durable copy is not the window's to know of.
    memory.forget("deploy-7").await.unwrap();

    let found = memory.recall("deploy", LIMIT).await.unwrap();

    assert_eq!(daemon.asked.load(Ordering::SeqCst), 1, "the miss asked");
    assert_eq!(
        keys(&found),
        ["deploy-9"],
        "a forgotten key stays forgotten for the run"
    );
}

#[tokio::test]
async fn a_recall_past_the_window_never_brings_back_a_key_the_run_overwrote() {
    let window = vec![entry("deploy-1")];
    let daemon = Daemon::default();
    let mut memory = Hydrated::new(Seed {
        recall: Some(&daemon),
        ..Seed::window(&window)
    });
    // The run's own deploy-9 no longer holds "note"; agentsfleetd's still does.
    let rewritten = MemoryDelta {
        content: Cow::Borrowed("moved to fly"),
        ..entry("deploy-9")
    };
    memory.store(rewritten).await.unwrap();

    let found = memory.recall("note", LIMIT).await.unwrap();

    assert_eq!(daemon.asked.load(Ordering::SeqCst), 1, "the miss asked");
    assert_eq!(
        keys(&found),
        ["deploy-1", "deploy-7"],
        "the stale copy of an overwritten key is never answered"
    );
}

/// A forget hides the fleet's OWN copy of a key. Another fleet's entry under
/// the same key is that fleet's to keep or drop, so the run's forget never
/// reaches it: the recall still answers it, and still names who wrote it.
#[tokio::test]
async fn a_recall_past_the_window_keeps_another_fleets_entry_under_a_key_the_run_forgot() {
    let window = vec![entry("deploy-1")];
    let daemon = Daemon {
        shares: Some("deploy-7"),
        ..Daemon::default()
    };
    let mut memory = Hydrated::new(Seed {
        recall: Some(&daemon),
        ..Seed::window(&window)
    });
    memory.forget("deploy-7").await.unwrap();

    let found = memory.recall("deploy", LIMIT).await.unwrap();

    assert_eq!(daemon.asked.load(Ordering::SeqCst), 1, "the miss asked");
    let answered: Vec<_> = found
        .iter()
        .map(|hit| (hit.key.as_ref(), hit.writer.as_deref()))
        .collect();
    assert_eq!(
        answered,
        [
            ("deploy-1", None),
            ("deploy-9", None),
            ("deploy-7", Some(WRITER)),
        ],
        "the fleet's own deploy-7 stays forgotten; the workspace's is kept"
    );
}

#[tokio::test]
async fn a_full_window_and_a_refusing_daemon_both_answer_from_the_window() {
    let window: Vec<_> = (0..LIMIT)
        .map(|at| entry(&format!("deploy-{at}")))
        .collect();
    let daemon = Daemon {
        refuses: true,
        ..Daemon::default()
    };
    let full = Hydrated::new(Seed {
        recall: Some(&daemon),
        ..Seed::window(&window)
    });
    assert_eq!(full.recall("deploy", LIMIT).await.unwrap().len(), LIMIT);
    assert_eq!(
        daemon.asked.load(Ordering::SeqCst),
        0,
        "a filled window asks nothing"
    );

    let short = vec![entry("deploy-1")];
    let missing = Hydrated::new(Seed {
        recall: Some(&daemon),
        ..Seed::window(&short)
    });
    let answered = missing.recall("deploy", LIMIT).await.unwrap();
    assert_eq!(daemon.asked.load(Ordering::SeqCst), 1, "the miss asked");
    assert_eq!(answered.len(), 1, "a refused ask answers from the window");
}
