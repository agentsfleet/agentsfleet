//! Checkpoint invalidation, cancellation ordering and bounded planner work.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fixture setup and selected plan shape must fail the proof loudly"
)]
mod support;

use afd_context::{Compaction, Plan, PortFuture, Summarizer, Summary, TokenEstimator, Window};
use std::sync::atomic::{AtomicBool, Ordering};
use support::{Estimator, Model, Source, Store, identity};

const GROUP_BYTES: usize = 1_000;
const PINNED_TOKENS: u64 = 1_000;
const OUTPUT_TOKENS: u32 = 1_000;
const LARGE_WINDOW_TOKENS: u32 = 100_000;

async fn task(source: &mut Source) -> Compaction {
    let plan = afd_context::prepare(
        source,
        &mut Estimator {
            pinned: PINNED_TOKENS,
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
        panic!("the source exceeds the selected window");
    };
    task
}

#[tokio::test]
async fn should_rebuild_from_original_history_when_model_or_pinned_identity_changes() {
    let mut source = Source::new(100, 100);
    let mut store = Store::default();
    let mut model = Model {
        calls: 0,
        refuse: false,
    };
    let checkpoint = afd_context::execute(
        task(&mut source).await,
        &mut model,
        &mut store,
        || false,
        |_| {},
    )
    .await
    .unwrap();
    for changed_model in [true, false] {
        let mut changed = identity();
        if changed_model {
            changed.model = "another selected model".into();
        } else {
            changed.pinned = "new authoritative instructions".into();
        }
        let plan = afd_context::prepare(
            &mut source,
            &mut Estimator {
                pinned: PINNED_TOKENS,
            },
            changed,
            Some(checkpoint.clone()),
            Window::new(LARGE_WINDOW_TOKENS, OUTPUT_TOKENS).unwrap(),
            || false,
            |_| {},
        )
        .await
        .unwrap();
        let Plan::Ready {
            summary, groups, ..
        } = plan
        else {
            panic!("original history fits");
        };
        assert!(summary.is_none());
        assert_eq!(groups.len(), 100);
        assert_eq!(groups.first().unwrap().sequence, 1);
        assert_eq!(groups.last().unwrap().sequence, 100);
    }
    assert_eq!(model.calls, 1);
    assert_eq!(store.writes, 1);
}

struct CancellingModel<'a> {
    cancelled: &'a AtomicBool,
    calls: usize,
}
impl Summarizer for CancellingModel<'_> {
    fn summarize<'a>(&'a mut self, _task: &'a Compaction) -> PortFuture<'a, Summary> {
        self.calls += 1;
        self.cancelled.store(true, Ordering::SeqCst);
        Box::pin(async { Ok(support::summary()) })
    }
}

#[tokio::test]
async fn should_preserve_completed_model_work_but_never_publish_after_cancellation() {
    let mut source = Source::new(100, 100);
    let cancelled = AtomicBool::new(false);
    let mut model = CancellingModel {
        cancelled: &cancelled,
        calls: 0,
    };
    let mut store = Store::default();
    let error = afd_context::execute(
        task(&mut source).await,
        &mut model,
        &mut store,
        || cancelled.load(Ordering::SeqCst),
        |_| {},
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        afd_context::Error::cancelled().to_string()
    );
    assert_eq!(model.calls, 1);
    assert_eq!(store.writes, 0);
    assert!(store.checkpoint.is_none());
}

#[derive(Default)]
struct CountingEstimator {
    encoded_bytes: usize,
    calls: usize,
}
impl TokenEstimator for CountingEstimator {
    fn continuation(
        &mut self,
        _summary: Option<&Summary>,
        groups: &[afd_context::Group],
    ) -> afd_context::Result<u64> {
        Ok(groups.iter().map(|group| group.text.len() as u64).sum())
    }
    fn compaction(&mut self, task: &Compaction) -> afd_context::Result<u64> {
        let bytes: usize = task.groups.iter().map(|group| group.text.len()).sum();
        self.encoded_bytes += bytes;
        self.calls += 1;
        Ok(bytes as u64)
    }
}

#[tokio::test]
async fn should_bound_candidate_encoding_work_by_twice_one_page_at_any_history_length() {
    for count in [64, 128, 1_024] {
        let mut source = Source::new(count, GROUP_BYTES);
        let mut estimator = CountingEstimator::default();
        let plan = afd_context::prepare(
            &mut source,
            &mut estimator,
            identity(),
            None,
            Window::new(8_000, OUTPUT_TOKENS).unwrap(),
            || false,
            |_| {},
        )
        .await
        .unwrap();
        assert!(matches!(plan, Plan::Compact(_)));
        assert!(estimator.encoded_bytes <= 2 * afd_context::HISTORY_PAGE_SIZE * GROUP_BYTES);
        assert!(estimator.calls <= 6);
        assert_eq!(source.reads, 1);
    }
}
