//! Dimensions 5.1 and 5.2 — what a retry costs, and what a shutdown keeps.
//!
//! Both are graded against a poster that answers on command, not against Slack:
//! the questions are about the WORKER's loop — how many times it offers a job,
//! when it stops offering, and whether it acknowledges — and a real vendor can
//! answer none of them on demand. `slack.rs`'s own suite covers the half that
//! is about reading Slack's replies.
//!
//! # Why the retry is exercised through `deliver_with_retry` and not `run`
//!
//! `Worker::run` needs a live Redis: an `OutboundReader` owns a socket, and
//! there is no in-memory stand-in for one that would prove anything about a
//! consumer group. The loop's Redis half is the integration lane's
//! (`test_outbound_delivery_retry`, `test_outbound_shutdown_no_loss`). What is
//! provable here without a server is the retry POLICY, which is the part that
//! decides what a vendor sees — and it is worth proving here because the
//! integration lane cannot make Slack answer 429 three times either.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Deliver, Posters, Verdict, dispatch};
use afd_redis::OutboundDelivery;
use afd_redis::streams::EventId;

/// A poster that answers from a script and counts what it was asked.
#[derive(Debug)]
struct Scripted {
    /// One verdict per attempt; the last is repeated once the script runs out.
    answers: Mutex<Vec<Verdict>>,
    attempts: AtomicUsize,
    /// Fires on the attempt at this index, so a test can cancel the worker
    /// mid-delivery rather than before it or after it.
    cancel_on: Option<(usize, tokio_util::sync::CancellationToken)>,
}

impl Scripted {
    fn new(answers: &[Verdict]) -> Self {
        let mut reversed = answers.to_vec();
        reversed.reverse();
        Self {
            answers: Mutex::new(reversed),
            attempts: AtomicUsize::new(0),
            cancel_on: None,
        }
    }

    /// The same, but cancelling `token` as the `on`th attempt is answered.
    fn cancelling(
        answers: &[Verdict],
        on: usize,
        token: tokio_util::sync::CancellationToken,
    ) -> Self {
        let mut scripted = Self::new(answers);
        scripted.cancel_on = Some((on, token));
        scripted
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Relaxed)
    }
}

impl Deliver for Scripted {
    fn deliver(&self, _job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        let index = self.attempts.fetch_add(1, Ordering::Relaxed);
        if let Some((on, token)) = &self.cancel_on
            && index == *on
        {
            token.cancel();
        }
        let mut answers = self.answers.lock().expect("no test panics holding this");
        // The last scripted answer repeats: a destination that is down stays
        // down, and a test asserting the attempt CEILING must not accidentally
        // be asserting the length of its own script.
        let answer = if answers.len() > 1 {
            answers.pop().expect("length checked above")
        } else {
            *answers.last().expect("a script is never empty")
        };
        std::future::ready(answer)
    }
}

fn job() -> OutboundDelivery {
    OutboundDelivery {
        id: EventId::of("1700000000001-0"),
        provider: "slack".to_owned(),
        workspace_id: "0199a0b0-0000-7000-8000-000000000001".to_owned(),
        fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
        event_id: "1700000000000-0".to_owned(),
        answer: "Aurora is healthy.".to_owned(),
    }
}

/// Dimension 5.1 — a queued answer is delivered ONCE when the destination
/// takes it.
///
/// The assertion that matters is the count. A worker that retried a delivered
/// job would post the same answer into the same thread twice, and a person
/// reading it would see the fleet stutter.
#[tokio::test]
async fn test_a_delivered_answer_is_offered_exactly_once() {
    let posters = Posters {
        slack: Scripted::new(&[Verdict::Delivered]),
    };

    let verdict = dispatch(&posters, &job()).await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(
        posters.slack.attempts(),
        1,
        "a delivered answer posted twice is a fleet that appears to stutter"
    );
}

/// Dimension 5.1 — a destination answering 5xx is retried up to the budget and
/// then given up on.
///
/// Two halves, and both are the point: it retries at all, and it STOPS. An
/// unbounded retry would hold this job at the head of a serial queue forever,
/// so every answer behind it would wait on one dead destination.
#[tokio::test]
async fn test_a_retryable_destination_is_offered_the_budget_and_no_more() {
    let posters = Posters {
        slack: Scripted::new(&[Verdict::Retryable]),
    };

    let verdict = afd_outbound::deliver_with_retry(&posters, &job(), &never_cancelled()).await;

    assert_eq!(
        verdict,
        Verdict::Retryable,
        "an exhausted budget reports itself, so the caller can log it before \
         acknowledging"
    );
    assert_eq!(
        posters.slack.attempts(),
        DELIVERY_ATTEMPTS,
        "the budget is the ceiling, not a suggestion"
    );
}

/// Dimension 5.1 — a destination that recovers mid-budget is delivered, and
/// the remaining attempts are not spent.
#[tokio::test]
async fn test_a_destination_that_recovers_is_not_offered_again() {
    let posters = Posters {
        slack: Scripted::new(&[Verdict::Retryable, Verdict::Delivered]),
    };

    let verdict = afd_outbound::deliver_with_retry(&posters, &job(), &never_cancelled()).await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(
        posters.slack.attempts(),
        2,
        "a delivery that succeeded on the second try must not use the third"
    );
}

/// Dimension 5.1 — a permanent refusal is not retried.
///
/// The distinction a boolean would lose. A revoked token or a deleted channel
/// refuses identically every time, so three attempts cost the vendor three
/// requests and the operator three log lines to reach the same place.
#[tokio::test]
async fn test_a_permanent_refusal_is_offered_once() {
    let posters = Posters {
        slack: Scripted::new(&[Verdict::Permanent]),
    };

    let verdict = afd_outbound::deliver_with_retry(&posters, &job(), &never_cancelled()).await;

    assert_eq!(verdict, Verdict::Permanent);
    assert_eq!(
        posters.slack.attempts(),
        1,
        "retrying a refusal that cannot change spends the vendor's budget for \
         nothing"
    );
}

/// Dimension 5.2 — a shutdown arriving mid-delivery stops the RETRY, not the
/// attempt in flight.
///
/// The attempt that is running finishes, and no further one starts. That is
/// what bounds a shutdown at one vendor deadline rather than at the whole
/// retry budget, and it is what lets the supervisor join inside its timeout.
/// The job is left unacknowledged by the caller on this verdict, which is the
/// "re-queues" half — proven against a real pending list in the integration
/// lane, since only Redis can say what is pending.
#[tokio::test]
async fn test_a_shutdown_stops_the_retry_without_abandoning_the_attempt() {
    let token = tokio_util::sync::CancellationToken::new();
    let posters = Posters {
        // Answers retryable forever, so the ONLY thing that can stop the loop
        // before the budget is the cancellation.
        slack: Scripted::cancelling(&[Verdict::Retryable], 0, token.clone()),
    };

    let verdict = afd_outbound::deliver_with_retry(&posters, &job(), &token).await;

    assert_eq!(
        verdict,
        Verdict::Retryable,
        "the job is handed back rather than reported delivered"
    );
    assert_eq!(
        posters.slack.attempts(),
        1,
        "the attempt in flight completes and no further one starts — a \
         shutdown must not wait out the retry budget"
    );
}

/// Dimension 5.2 — a shutdown does not discard an attempt that SUCCEEDED.
///
/// The direction that would be worse to get wrong. A worker that treated a
/// cancelled token as "give up" after a successful post would leave the job
/// unacknowledged, and the next process would deliver the same answer into the
/// same thread a second time.
#[tokio::test]
async fn test_a_shutdown_during_a_successful_delivery_still_reports_it() {
    let token = tokio_util::sync::CancellationToken::new();
    let posters = Posters {
        slack: Scripted::cancelling(&[Verdict::Delivered], 0, token.clone()),
    };

    let verdict = afd_outbound::deliver_with_retry(&posters, &job(), &token).await;

    assert_eq!(
        verdict,
        Verdict::Delivered,
        "an answer that landed is acknowledged even as the process stops, or \
         the next one posts it again"
    );
}

/// A token nothing ever cancels.
fn never_cancelled() -> tokio_util::sync::CancellationToken {
    tokio_util::sync::CancellationToken::new()
}
