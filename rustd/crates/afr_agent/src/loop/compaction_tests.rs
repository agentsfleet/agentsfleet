//! Repeated runtime compaction uses the real loop and its provider/storage boundaries.
#![expect(
    clippy::unwrap_used,
    reason = "test fixture failures are test failures"
)]
use super::Loop;
use crate::{
    AgentEngine as _, AgentRun, Meter,
    fixture::{Canned, Frames, call, lease, say, spent, unbounded},
    testing::{ContextMemory, Discard},
};
use afd_wire::{
    activity::ActivityFrame,
    context::{ContextProgress, ContextState},
    lease::LeasePayload,
    policy::ExecutionPolicy,
};
use afr_providers::{Chunk, Connect, End, Message, Provider, Request};
use afr_tools::catalog::UPDATE_PLAN;
use futures_util::{StreamExt as _, stream::BoxStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio_util::sync::CancellationToken;

const TURNS: usize = 55;
const TOKENS_PER_MESSAGE: u64 = 1_000;
const GOAL_CONSTRAINT: &str = "preserve the goal";

fn summary() -> String {
    serde_json::json!({"goal":"finish all fifty-five steps","constraints":[GOAL_CONSTRAINT],
        "decisions":[],"completed":[],"remaining":["continue working"],"references":[]})
    .to_string()
}

#[derive(Debug, Default)]
struct Seen {
    normal: AtomicUsize,
    summaries: AtomicUsize,
    messages: Mutex<Vec<Vec<Message>>>,
}
#[derive(Debug, Clone)]
struct Model {
    seen: Arc<Seen>,
    failure: SummaryFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SummaryFailure {
    None,
    Invalid,
    Truncated,
}
impl Connect for Model {
    fn admit(&self, _: &ExecutionPolicy<'_>) -> afr_providers::Result<()> {
        Ok(())
    }
    fn connect(&self, _: &LeasePayload<'_>) -> afr_providers::Result<Box<dyn Provider>> {
        Ok(Box::new(self.clone()))
    }
}
impl Provider for Model {
    fn measure(&self, request: Request<'_>) -> afr_providers::Result<u64> {
        // The external scripted wire's framing: deterministic pressure after
        // complete tool pairs, independent of machine speed or reported usage.
        Ok(if request.single_attempt {
            100
        } else {
            100 + request.messages.len() as u64 * TOKENS_PER_MESSAGE
        })
    }
    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, afr_providers::Result<Chunk>> {
        let mut chunks = if request.single_attempt {
            self.seen.summaries.fetch_add(1, Ordering::SeqCst);
            assert!(request.tools.is_empty() && request.hosted.is_empty());
            assert_eq!(request.instructions, afd_context::SUMMARY_INSTRUCTIONS);
            assert_eq!(
                request.max_output_tokens,
                Some(afd_context::SUMMARY_OUTPUT_TOKENS)
            );
            let summary = summary();
            vec![
                say(if self.failure == SummaryFailure::Invalid {
                    "unusable notes"
                } else {
                    &summary
                }),
                spent(100, 0, 5),
            ]
        } else {
            assert_ne!(request.tools, []);
            assert!(request.messages.iter().any(|message| matches!(message, Message::User(text) if text.contains("triage the failed run"))));
            self.seen
                .messages
                .lock()
                .unwrap()
                .push(request.messages.to_vec());
            let index = self.seen.normal.fetch_add(1, Ordering::SeqCst);
            if index == TURNS {
                vec![say("all steps finished"), spent(100, 0, 1)]
            } else {
                vec![
                    call(
                        &format!("step-{index}"),
                        UPDATE_PLAN.name(),
                        serde_json::json!({}),
                    ),
                    spent(100, 0, 1),
                ]
            }
        };
        chunks.push(Chunk::End(End {
            cut: request.single_attempt && self.failure == SummaryFailure::Truncated,
            ..End::default()
        }));
        futures_util::stream::iter(chunks.into_iter().map(Ok)).boxed()
    }
    fn accepts_images(&self) -> bool {
        false
    }
}

async fn run(failure: SummaryFailure) -> (crate::RunOutput, Arc<Seen>, ContextMemory, Meter) {
    let seen = Arc::new(Seen::default());
    let engine = Loop::new(
        afr_tools::Catalog::new(vec![Canned::boxed(&UPDATE_PLAN, "completed step")]),
        Model {
            seen: Arc::clone(&seen),
            failure,
        },
    );
    let mut lease = lease(&[UPDATE_PLAN.name()], unbounded());
    lease.lease_id = "01900000-0000-7000-8000-000000000001".into();
    lease.policy.model_context_tokens = 10_000;
    lease.policy.context.context_cap_tokens = 10_000;
    lease.policy.context.stage_chunk_threshold = 0.90;
    lease.policy.inference = Some(afd_wire::inference::ModelInference {
        api: afd_wire::inference::InferenceApi::Chat,
        base_url: "https://models.example.com/v1".into(),
        dialect: None,
        max_output_tokens: 2_048,
    });
    let contexts = ContextMemory::default();
    let meter = Meter::default();
    let frames = Frames::default();
    let output = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &afr_egress::testing::CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            context_checkpoints: &contexts,
            events: &frames.sink(),
            meter: &meter,
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();
    let frames = frames.taken();
    assert!(
        !format!("{frames:?}").contains(GOAL_CONSTRAINT),
        "private summary is never a visible answer frame"
    );
    assert_progress(
        &frames,
        seen.summaries.load(Ordering::SeqCst),
        failure != SummaryFailure::None,
    );
    (output, seen, contexts, meter)
}

fn assert_progress(frames: &[ActivityFrame<'_>], summaries: usize, reject: bool) {
    let notices: Vec<_> = frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::ContextProgress(progress) => Some(*progress),
            _ => None,
        })
        .collect();
    let expected: Vec<_> = (1..=summaries)
        .flat_map(|generation| {
            [
                ContextProgress {
                    generation: generation as u64,
                    state: ContextState::Summarizing,
                },
                ContextProgress {
                    generation: generation as u64,
                    state: if reject {
                        ContextState::Interrupted
                    } else {
                        ContextState::Resuming
                    },
                },
            ]
        })
        .collect();
    assert_eq!(
        notices, expected,
        "each root summary announces one guarded outcome"
    );
}

#[tokio::test]
async fn repeated_compaction_continues_past_turn_forty_two_and_keeps_tools_and_goal() {
    let (output, seen, contexts, meter) = run(SummaryFailure::None).await;
    assert_eq!(output.result.content, "all steps finished");
    assert_eq!(output.records.len(), TURNS);
    assert_eq!(seen.normal.load(Ordering::SeqCst), TURNS + 1);
    let summaries = seen.summaries.load(Ordering::SeqCst);
    assert!(summaries > 1, "the test must cross repeated compactions");
    assert_eq!(meter.backlog(), TURNS + 1 + summaries);
    assert_eq!(meter.read().output, (TURNS + 1 + summaries * 5) as u64);
    let checkpoint =
        crate::ContextCheckpoints::load(&contexts, "01900000-0000-7000-8000-000000000001")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(checkpoint.generation, summaries as u64);
    assert_eq!(checkpoint.summary.goal, "finish all fifty-five steps");
    assert!(seen.messages.lock().unwrap().last().unwrap().iter().any(|message| matches!(message, Message::User(text) if text.contains("finish all fifty-five steps"))));
}

#[tokio::test]
async fn unusable_summary_retains_known_charge_without_publishing_or_running_more_tools() {
    for failure in [SummaryFailure::Invalid, SummaryFailure::Truncated] {
        let (output, seen, contexts, meter) = run(failure).await;
        assert_ne!(output.result.content, "all steps finished");
        assert_eq!(seen.summaries.load(Ordering::SeqCst), 1);
        assert!(
            crate::ContextCheckpoints::load(&contexts, "01900000-0000-7000-8000-000000000001")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(meter.backlog(), seen.normal.load(Ordering::SeqCst) + 1);
        assert_eq!(
            meter.read().output,
            seen.normal.load(Ordering::SeqCst) as u64 + 5
        );
        assert!(output.records.len() < TURNS);
    }
}
