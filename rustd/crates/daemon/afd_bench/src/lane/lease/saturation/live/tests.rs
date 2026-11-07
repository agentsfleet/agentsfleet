//! What a viewer reads, in what order, and when it stops.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::future::ready;
use core::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use afd_core::id::Uuid7;

use super::{Wall, view};
use crate::abort::Abort;
use crate::lane::lease::saturation::requester::Shared;

const POPULATION: u64 = 1;

/// A window long enough that only a fake closes it.
const WINDOW: Duration = Duration::from_secs(60);

fn window() -> Shared {
    Shared {
        deadline: Instant::now() + WINDOW,
        leased: AtomicU64::new(0),
        population: POPULATION,
        abort: Arc::new(Abort::new(1.0)),
    }
}

fn workspace(last: u8) -> Uuid7 {
    Uuid7::parse(&format!("01920000-0000-7000-8000-0000000000{last:02}")).expect("canonical v7")
}

/// Records what it was asked for, answers `answers`, and closes the window
/// after `reads`.
struct Recording<'a> {
    shared: &'a Shared,
    reads: u64,
    answers: bool,
    asked: Mutex<Vec<Uuid7>>,
}

impl Wall for Recording<'_> {
    fn read(&self, workspace: &Uuid7) -> impl Future<Output = bool> + Send {
        let mut asked = self.asked.lock().expect("unpoisoned");
        asked.push(workspace.clone());
        if asked.len() as u64 >= self.reads {
            self.shared.leased.store(POPULATION, Ordering::Relaxed);
        }
        ready(self.answers)
    }
}

#[tokio::test]
async fn test_a_viewer_cycles_every_workspace_until_the_window_closes() {
    let shared = window();
    let wall = Recording {
        shared: &shared,
        reads: 3,
        answers: true,
        asked: Mutex::default(),
    };
    let spaces = [workspace(1), workspace(2)];
    let reads = view(&wall, &spaces, &shared)
        .await
        .expect("a scripted wall is read");
    let asked = wall.asked.into_inner().expect("unpoisoned");
    assert_eq!(asked, vec![workspace(1), workspace(2), workspace(1)]);
    assert_eq!((reads.latency.count(), reads.refused), (3, 0));
}

#[tokio::test]
async fn test_a_refused_read_is_counted_and_reads_absorb() {
    let shared = window();
    let wall = Recording {
        shared: &shared,
        reads: 2,
        answers: false,
        asked: Mutex::default(),
    };
    let mut reads = view(&wall, &[workspace(1)], &shared)
        .await
        .expect("refusals are counted");
    assert_eq!((reads.latency.count(), reads.refused), (0, 2));
    let more = view(&wall, &[workspace(1)], &shared)
        .await
        .expect("a closed window reads nothing");
    reads.absorb(&more).expect("latencies merge");
    assert_eq!(reads.refused, 2, "a closed window adds nothing");
}
