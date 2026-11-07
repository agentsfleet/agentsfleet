//! Source paging, repeated summaries and guarded publication through public ports.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "fixture failures must fail these boundary proofs loudly"
)]
mod support;

use afd_context::{Plan, Window, execute, prepare};
use support::{Estimator, Model, Source, Store, identity};

const PINNED_TOKENS: u64 = 1_000;
const OUTPUT_TOKENS: u32 = 1_000;
const LARGE_WINDOW_TOKENS: u32 = 100_000;

#[tokio::test]
async fn should_read_more_than_sixty_four_turns_without_refusing_or_dropping_history() {
    let mut source = Source::new(181, 100);
    let mut estimator = Estimator {
        pinned: PINNED_TOKENS,
    };
    let plan = prepare(
        &mut source,
        &mut estimator,
        identity(),
        None,
        Window::new(LARGE_WINDOW_TOKENS, OUTPUT_TOKENS).unwrap(),
        || false,
        |_| {},
    )
    .await
    .unwrap();
    let Plan::Ready { groups, .. } = plan else {
        panic!("history fits the selected window");
    };
    assert_eq!(groups.len(), 181);
    assert_eq!(groups.first().unwrap().sequence, 1);
    assert_eq!(groups.last().unwrap().sequence, 181);
    assert_eq!(source.reads, 3);
}

#[tokio::test]
async fn should_compact_repeatedly_and_restore_the_same_goal_and_retained_source() {
    let mut source = Source::new(181, 100);
    let mut estimator = Estimator {
        pinned: PINNED_TOKENS,
    };
    let mut store = Store::default();
    let mut model = Model {
        calls: 0,
        refuse: false,
    };
    let mut commits = Vec::new();
    loop {
        let plan = prepare(
            &mut source,
            &mut estimator,
            identity(),
            store.checkpoint.clone(),
            Window::new(8_000, OUTPUT_TOKENS).unwrap(),
            || false,
            |_| {},
        )
        .await
        .unwrap();
        match plan {
            Plan::Ready {
                summary, groups, ..
            } => {
                assert_eq!(summary.unwrap().goal, support::summary().goal);
                assert_eq!(groups.last().unwrap().sequence, 181);
                break;
            }
            Plan::Compact(task) => {
                assert!(task.output_tokens <= OUTPUT_TOKENS);
                let checkpoint = execute(
                    task,
                    &mut model,
                    &mut store,
                    || false,
                    |event| commits.push(event),
                )
                .await
                .unwrap();
                assert_eq!(checkpoint.generation, model.calls as u64);
            }
        }
    }
    assert!(model.calls >= 2);
    assert_eq!(model.calls, store.writes);
    assert_ne!(commits, []);
}

#[tokio::test]
async fn should_reject_a_changed_source_after_a_checkpoint_without_another_model_call() {
    let mut source = Source::new(100, 100);
    let mut estimator = Estimator {
        pinned: PINNED_TOKENS,
    };
    let Plan::Compact(task) = prepare(
        &mut source,
        &mut estimator,
        identity(),
        None,
        Window::new(8_000, OUTPUT_TOKENS).unwrap(),
        || false,
        |_| {},
    )
    .await
    .unwrap() else {
        panic!("compaction expected");
    };
    let mut store = Store::default();
    let mut model = Model {
        calls: 0,
        refuse: false,
    };
    execute(task, &mut model, &mut store, || false, |_| {})
        .await
        .unwrap();
    source.groups[0].text = "changed original source".into();
    prepare(
        &mut source,
        &mut estimator,
        identity(),
        store.checkpoint.clone(),
        Window::new(8_000, OUTPUT_TOKENS).unwrap(),
        || false,
        |_| {},
    )
    .await
    .unwrap_err();
    assert_eq!(model.calls, 1);
    assert_eq!(store.writes, 1);
}

#[tokio::test]
async fn should_reject_non_increasing_authoritative_sequences() {
    let mut source = Source::new(10, 10);
    source.malformed = true;
    let result = prepare(
        &mut source,
        &mut Estimator { pinned: 1 },
        identity(),
        None,
        Window::new(8_000, OUTPUT_TOKENS).unwrap(),
        || false,
        |_| {},
    )
    .await;
    result.unwrap_err();
}

#[tokio::test]
async fn should_stop_before_reading_history_or_spending_a_model_call() {
    let mut source = Source::new(181, 100);
    prepare(
        &mut source,
        &mut Estimator { pinned: 1 },
        identity(),
        None,
        Window::new(8_000, OUTPUT_TOKENS).unwrap(),
        || true,
        |_| {},
    )
    .await
    .unwrap_err();
    assert_eq!(source.reads, 0);
}

#[tokio::test]
async fn should_preserve_the_previous_checkpoint_when_the_model_outcome_is_unusable() {
    let mut source = Source::new(100, 100);
    let Plan::Compact(task) = prepare(
        &mut source,
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
    .unwrap() else {
        panic!("compaction expected");
    };
    let mut store = Store::default();
    let mut model = Model {
        calls: 0,
        refuse: true,
    };
    execute(task, &mut model, &mut store, || false, |_| {})
        .await
        .unwrap_err();
    assert_eq!(model.calls, 1);
    assert_eq!(store.writes, 0);
    assert!(store.checkpoint.is_none());
}

#[tokio::test]
async fn should_preserve_accounted_model_work_when_the_guarded_commit_is_stale() {
    let mut source = Source::new(100, 100);
    let Plan::Compact(task) = prepare(
        &mut source,
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
    .unwrap() else {
        panic!("compaction expected");
    };
    let mut store = Store {
        refuse: true,
        ..Store::default()
    };
    let mut model = Model {
        calls: 0,
        refuse: false,
    };
    execute(task, &mut model, &mut store, || false, |_| {})
        .await
        .unwrap_err();
    assert_eq!(model.calls, 1);
    assert_eq!(store.writes, 0);
}
