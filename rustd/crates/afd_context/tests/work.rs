//! Work counts and overlapping conversations prove preparation costs and isolation.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fixture failures fail the proof"
)]
mod support;

use afd_context::{
    Compaction, Group, Plan, PortFuture, Summarizer, Summary, TokenEstimator, Window,
};
use std::{sync::Arc, time::Duration};
use support::{Estimator, Model, Source, Store, identity};
use tokio::sync::Notify;

const PHYSICAL_WINDOW: u32 = 1_000_000;
const OUTPUT_TOKENS: u32 = 1_000;
const LINEAR_WORK_MULTIPLIER: usize = 3;
const PROGRESS_TIMEOUT: Duration = Duration::from_secs(5);

struct Counted {
    estimator: Estimator,
    encoded: usize,
}
impl TokenEstimator for Counted {
    fn continuation(
        &mut self,
        summary: Option<&Summary>,
        groups: &[Group],
    ) -> afd_context::Result<u64> {
        self.encoded += groups.len();
        self.estimator.continuation(summary, groups)
    }
    fn compaction(&mut self, task: &Compaction) -> afd_context::Result<u64> {
        self.encoded += task.groups.len();
        self.estimator.compaction(task)
    }
}

#[tokio::test]
async fn cumulative_provider_encoding_grows_linearly_as_history_grows_tenfold() {
    for pages in [4, 40, 400] {
        let count = pages * afd_context::HISTORY_PAGE_SIZE;
        let mut source = Source::new(count as u64, 1);
        let mut estimator = Counted {
            estimator: Estimator { pinned: 1 },
            encoded: 0,
        };
        let plan = afd_context::prepare(
            &mut source,
            &mut estimator,
            identity(),
            None,
            Window::new(PHYSICAL_WINDOW, OUTPUT_TOKENS).unwrap(),
            || false,
            |_| {},
        )
        .await
        .unwrap();
        let Plan::Ready { groups, .. } = plan else {
            panic!("history fits the physical model");
        };
        assert_eq!(groups.len(), count);
        assert_eq!(source.reads, pages);
        assert!(
            estimator.encoded <= LINEAR_WORK_MULTIPLIER * count,
            "encoded {} groups for {count} source groups",
            estimator.encoded
        );
    }
}

struct Paused {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    model: Model,
}
impl Summarizer for Paused {
    fn summarize<'a>(&'a mut self, task: &'a Compaction) -> PortFuture<'a, Summary> {
        Box::pin(async move {
            self.entered.notify_one();
            self.release.notified().await;
            self.model.summarize(task).await
        })
    }
}

async fn task() -> Compaction {
    let mut source = Source::new(100, 100);
    let plan = afd_context::prepare(
        &mut source,
        &mut Estimator {
            pinned: OUTPUT_TOKENS.into(),
        },
        identity(),
        None,
        Window::new(8_000, OUTPUT_TOKENS).unwrap(),
        || false,
        |_| {},
    )
    .await
    .unwrap();
    let Plan::Compact(task) = plan else {
        panic!("small model needs compaction");
    };
    task
}

#[tokio::test]
async fn a_blocked_summary_does_not_block_another_conversations_checkpoint() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut paused = Paused {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
        model: Model {
            calls: 0,
            refuse: false,
        },
    };
    let mut first = Store::default();
    let mut second = Store::default();
    let first_task = task().await;
    let second_task = task().await;
    let blocked = afd_context::execute(first_task, &mut paused, &mut first, || false, |_| {});
    let independent = async {
        entered.notified().await;
        let mut model = Model {
            calls: 0,
            refuse: false,
        };
        afd_context::execute(second_task, &mut model, &mut second, || false, |_| {})
            .await
            .unwrap();
        assert_eq!(second.writes, 1);
        release.notify_one();
    };
    let (saved, ()) = tokio::time::timeout(PROGRESS_TIMEOUT, async {
        tokio::join!(blocked, independent)
    })
    .await
    .unwrap();
    saved.unwrap();
    assert_eq!(first.writes, 1);
    assert_eq!(paused.model.calls, 1);
}
