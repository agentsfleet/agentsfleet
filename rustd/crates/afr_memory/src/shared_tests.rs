//! Dimensions 5.1 and 5.2: a recall the window cannot fill asks `agentsfleetd`,
//! once per miss and never past the cap, and merges without duplicates.
#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::error_code;
use afd_wire::memory::{MemoryDelta, MemoryRecallResponse, PINNED_CATEGORY, Visibility};

use super::{Hydrated, MemoryBackend, RECALL_MISS_CAP, Recall, Seed};
use crate::Result;

/// The limit every recall here asks for.
const LIMIT: usize = 5;

fn entry(key: &str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(format!("deploy note {key}")),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Fleet,
    }
}

/// A daemon that answers every recall with `found`, or refuses, counting asks.
#[derive(Debug, Default)]
struct Daemon {
    asked: AtomicUsize,
    refuses: bool,
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
            shared: Vec::new(),
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
