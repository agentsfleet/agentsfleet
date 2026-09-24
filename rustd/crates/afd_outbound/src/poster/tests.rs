//! Which poster a job reaches, and which attempt it reaches it as.

use super::*;
use std::sync::atomic::AtomicUsize;

/// The thread an owed answer is addressed to, as a Slack producer records it.
const DESTINATION: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

/// A poster that records how many times it was asked.
#[derive(Debug, Default)]
struct Counting {
    calls: AtomicUsize,
}

impl Deliver for Counting {
    fn deliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.calls.fetch_add(1, Ordering::Relaxed);
        std::future::ready(Verdict::Delivered)
    }
}

fn job(provider: &str) -> OutboundDelivery {
    OutboundDelivery {
        id: afd_dragonfly::streams::EventId::of("1700000000001-0"),
        provider: provider.to_owned(),
        destination: DESTINATION.to_owned(),
        workspace_id: "0199a0b0-0000-7000-8000-000000000001".to_owned(),
        fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
        event_id: "1700000000000-0".to_owned(),
        answer: "Aurora is healthy.".to_owned(),
    }
}

#[tokio::test]
async fn test_a_slack_job_reaches_the_slack_poster() {
    let posters = Posters {
        slack: Counting::default(),
    };

    let verdict = dispatch(&posters, &job(Provider::Slack.id()), Attempt::First).await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(posters.slack.calls.load(Ordering::Relaxed), 1);
}

/// A provider string no connector answers to must not reach a poster and
/// must not be redelivered — the two halves of dropping it safely.
#[tokio::test]
async fn test_an_unknown_provider_is_permanent_and_reaches_no_poster() {
    let posters = Posters {
        slack: Counting::default(),
    };

    let verdict = dispatch(&posters, &job("pagerduty"), Attempt::First).await;

    assert_eq!(
        verdict,
        Verdict::Permanent,
        "an unroutable job retried forever is worse than one dropped"
    );
    assert_eq!(posters.slack.calls.load(Ordering::Relaxed), 0);
}

/// A provider this build CONNECTS but cannot answer through is the same
/// verdict for a different reason, and the reason is what the log carries.
#[tokio::test]
async fn test_a_connectable_provider_with_no_poster_is_permanent() {
    let posters = Posters {
        slack: Counting::default(),
    };

    for provider in [
        Provider::GitHub,
        Provider::Zoho,
        Provider::Jira,
        Provider::Linear,
    ] {
        let verdict = dispatch(&posters, &job(provider.id()), Attempt::Repeat).await;

        assert_eq!(
            verdict,
            Verdict::Permanent,
            "{} connects but has no answer surface yet",
            provider.id()
        );
    }
    assert_eq!(posters.slack.calls.load(Ordering::Relaxed), 0);
}

/// A poster that fails its first attempt as retryable, lands its second, and
/// records which way each attempt reached it.
#[derive(Debug, Default)]
struct Recording {
    seen: std::sync::Mutex<Vec<Attempt>>,
}

impl Recording {
    fn answer(&self, attempt: Attempt) -> impl Future<Output = Verdict> + Send + use<> {
        let mut seen = self
            .seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        seen.push(attempt);
        std::future::ready(if seen.len() > 1 {
            Verdict::Delivered
        } else {
            Verdict::Retryable
        })
    }

    fn seen(&self) -> Vec<Attempt> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Deliver for Recording {
    fn deliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.answer(Attempt::First)
    }

    fn redeliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.answer(Attempt::Repeat)
    }
}

/// Only a ledger count of exactly one opens a cycle as its answer's first
/// attempt; a later cycle, and a count nobody could read, look first.
#[test]
fn only_a_first_cycle_opens_as_a_first_attempt() {
    assert_eq!(Attempt::opening(Some(1)), Attempt::First);
    for cycles in [Some(2), Some(9), None] {
        assert_eq!(Attempt::opening(cycles), Attempt::Repeat, "{cycles:?}");
    }
}

/// The retry after a retryable verdict is a repeat: the attempt it follows
/// may have landed, so the poster is asked to look before posting again.
#[tokio::test(start_paused = true)]
async fn a_retry_after_a_first_attempt_is_a_repeat() {
    let posters = Posters {
        slack: Recording::default(),
    };
    let token = CancellationToken::new();

    let verdict = deliver_with_retry(&posters, &job(Provider::Slack.id()), &token, Attempt::First).await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(posters.slack.seen(), [Attempt::First, Attempt::Repeat]);
}

/// A cycle that is not the answer's first never opens with a plain post.
#[tokio::test(start_paused = true)]
async fn a_later_cycle_repeats_from_its_first_attempt() {
    let posters = Posters {
        slack: Recording::default(),
    };
    let token = CancellationToken::new();

    let verdict = deliver_with_retry(&posters, &job(Provider::Slack.id()), &token, Attempt::Repeat).await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(posters.slack.seen(), [Attempt::Repeat, Attempt::Repeat]);
}

/// A poster that cannot ask its destination what it holds keeps the default:
/// a repeat is another delivery.
#[tokio::test]
async fn a_repeat_to_a_poster_that_cannot_look_delivers_again() {
    let posters = Posters {
        slack: Counting::default(),
    };

    let verdict = dispatch(&posters, &job(Provider::Slack.id()), Attempt::Repeat).await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(posters.slack.calls.load(Ordering::Relaxed), 1);
}
