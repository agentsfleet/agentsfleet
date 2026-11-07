//! Invalid source boundaries cannot authorize model work or checkpoint publication.
#![expect(
    clippy::unwrap_used,
    reason = "fixture construction and exact refusal classification must fail loudly"
)]
mod support;

use afd_context::{Compaction, Group, HistorySource, Page, PortFuture, Window};
use std::{cell::Cell, error::Error as _};
use support::{Estimator, Model, Source, Store, identity, summary};

fn task() -> Compaction {
    Compaction {
        identity: identity(),
        expected_generation: 0,
        prior_digest: String::new(),
        prior_summary: None,
        groups: vec![Group {
            sequence: 1,
            text: "original conversation".into(),
        }],
        output_tokens: 100,
    }
}

// Value: rejects an empty prepared prefix before the conditional write is invoked.
#[tokio::test]
async fn should_refuse_empty_compaction_without_invoking_publication() {
    let mut empty = task();
    empty.groups.clear();
    let writes = Cell::new(0);
    let error = empty
        .finish_with(summary(), |_, _| {
            writes.set(writes.get() + 1);
            async { Ok(()) }
        })
        .await
        .unwrap_err();
    assert!(error.is_stale());
    assert_eq!(writes.get(), 0);
}

// Value: cancellation before execution spends no model call and leaves the store intact.
#[tokio::test]
async fn should_stop_before_the_model_and_checkpoint_store_are_called() {
    let mut model = Model {
        calls: 0,
        refuse: false,
    };
    let mut store = Store::default();
    let mut observed = Vec::new();
    let error = afd_context::execute(
        task(),
        &mut model,
        &mut store,
        || true,
        |event| observed.push(event),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        afd_context::Error::cancelled().to_string()
    );
    assert_eq!(model.calls, 0);
    assert_eq!(store.writes, 0);
    assert!(store.checkpoint.is_none());
    assert_eq!(observed, [] as [afd_context::Progress; 0]);
}

struct InvalidPage {
    groups: Vec<Group>,
    has_more: bool,
    reads: usize,
}

impl HistorySource for InvalidPage {
    fn page(&mut self, _sequence: u64, _limit: usize) -> PortFuture<'_, Page> {
        self.reads += 1;
        Box::pin(async {
            Ok(Page {
                groups: self.groups.clone(),
                has_more: self.has_more,
            })
        })
    }
}

// Value: an oversized page or a non-progressing cursor fails before unbounded rereads.
#[tokio::test]
async fn should_refuse_oversized_and_non_progressing_source_pages() {
    for (count, has_more) in [(65, false), (0, true)] {
        let mut source = InvalidPage {
            groups: (1..=count)
                .map(|sequence| Group {
                    sequence,
                    text: "source".into(),
                })
                .collect(),
            has_more,
            reads: 0,
        };
        let error = afd_context::prepare(
            &mut source,
            &mut Estimator { pinned: 1 },
            identity(),
            None,
            Window::new(8_000, 100).unwrap(),
            || false,
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(error.is_stale());
        assert_eq!(source.reads, 1);
    }
}

// Value: deletion of covered original history cannot silently restore old model notes.
#[tokio::test]
async fn should_refuse_a_checkpoint_whose_covered_history_has_disappeared() {
    let checkpoint = task()
        .finish_with(summary(), |_, _| async { Ok(()) })
        .await
        .unwrap();
    let mut source = Source::new(0, 0);
    let error = afd_context::prepare(
        &mut source,
        &mut Estimator { pinned: 1 },
        identity(),
        Some(checkpoint),
        Window::new(8_000, 100).unwrap(),
        || false,
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(error.is_stale());
    assert_eq!(source.reads, 1);
}

// Value: whitespace-only model output cannot become a successful durable summary.
#[test]
fn should_reject_a_whitespace_only_summary_goal() {
    let mut invalid = summary();
    invalid.goal = " \t\n".into();
    let error =
        afd_context::Summary::decode(&serde_json::to_string(&invalid).unwrap()).unwrap_err();
    assert_eq!(error.to_string(), afd_context::Error::summary().to_string());
}

// Value: diagnostics retain useful scope/sequence but exclude private text and model state.
#[test]
fn should_hide_private_group_and_identity_values_and_preserve_boundary_causes() {
    let private = "private-conversation-canary";
    let group = Group {
        sequence: 7,
        text: private.into(),
    };
    let mut identity = identity();
    identity.model = private.into();
    identity.pinned = private.into();
    for rendered in [
        format!("{group:?}"),
        format!("{group:#?}"),
        format!("{identity:?}"),
        format!("{identity:#?}"),
    ] {
        assert!(!rendered.contains(private));
        assert!(rendered.contains("sequence") || rendered.contains("scope"));
    }
    let error = afd_context::Error::boundary(std::io::Error::other(private));
    assert_eq!(
        error.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(!error.to_string().contains(private));
    assert_eq!(error.source().unwrap().to_string(), private);
}

// Value: an unset operating allowance retains the selected physical window's trigger.
#[test]
fn should_use_the_physical_window_when_the_operating_allowance_is_zero() {
    let window = Window::new(10_000, 100).unwrap().with_operating_tokens(0);
    assert_eq!(window.trigger_tokens(), 9_000);
    assert!(!window.needs_compaction(8_999));
    assert!(window.needs_compaction(9_000));
}
