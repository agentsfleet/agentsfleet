//! A chat lease's earlier turns, as the plane hands them to a runner.
//!
//! `integration_lease_history.rs` proves the read; these drive the path around
//! it: `Plane::lease_claimed` issues a lease, `Plane::report` ends it, and the
//! next lease's payload carries what that report left. They live under this
//! suite because an issued lease needs the provider seed only it owns.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::test_util::PlatformDefault;
use afd_wire::event::EventType;
use afd_wire::lease::LeaseResponse;
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};

use super::seed::seed_provider_resolution;
use super::{NO_LEASE, READ_BOUND_CONFIG, set_config};
use crate::report_seed::{DEEP_POOL, SLICE_MS};
use crate::requests::ENROLLED_AT;
use crate::seed::{ACTOR, seeded_parts};
use crate::support::Fixtures;

/// What the first run of each case is asked, and what it answers.
const FIRST_MESSAGE: &str = "which tests failed?";
const FIRST_ANSWER: &str = "two: a and b";

/// What the follow-up asks.
const FOLLOW_UP: &str = "fix the second one";

/// One leasable fleet with two runners, and the provider default its leases
/// resolve against, held for the case's life.
struct Chat {
    fixtures: Fixtures,
    fleet: String,
    workspace: String,
    runner: Uuid7,
    spare: Uuid7,
    _default: PlatformDefault,
}

/// What a case reads off an issued lease.
struct Issued {
    lease_id: String,
    event_id: String,
    fencing_token: u64,
    history: Vec<(String, String)>,
}

impl Chat {
    async fn open() -> Self {
        let fixtures = Fixtures::create_with_queue().await;
        let (fleet, workspace, tenant, [runner, spare]) = seeded_parts::<2>(&fixtures).await;
        set_config(&fixtures, &fleet, READ_BOUND_CONFIG).await;
        fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;
        let default = seed_provider_resolution(&fixtures, &fleet).await;
        Self {
            fixtures,
            fleet,
            workspace,
            runner,
            spare,
            _default: default,
        }
    }

    /// Admits one `event_type` event asking `message`, stamped `at`.
    async fn ask(&self, event_type: EventType, message: &str, at: i64) -> String {
        let request = serde_json::json!({ "message": message }).to_string();
        crate::queue::enqueue(
            self.fixtures.queue(),
            &self.fleet,
            &self.workspace,
            ACTOR,
            event_type.as_str(),
            &request,
            at,
        )
        .await
    }

    /// `runner`'s claim on this fleet at `now`, or nothing while another
    /// holds it.
    async fn claim(&self, runner: &Uuid7, now: i64) -> Option<afd_fleet::lease::Acquired> {
        let leases = self.fixtures.leases();
        let now = UnixMillis::from_millis(now);
        crate::seed::select_fleet_within_rotations(&leases, runner, now, &self.fleet).await
    }

    /// The lease the plane issues `runner` for this fleet's next event.
    async fn lease(&self, runner: &Uuid7, now: i64) -> Issued {
        let claimed = self
            .claim(runner, now)
            .await
            .expect("the fleet is leasable");
        let answer = self
            .fixtures
            .plane()
            .lease_claimed(claimed, runner, UnixMillis::from_millis(now))
            .await
            .expect("every gate verdict is a decision, not a fault");
        assert!(
            !answer.contains(NO_LEASE),
            "the lease was refused: {answer}"
        );
        let response: LeaseResponse<'_> =
            serde_json::from_str(&answer).expect("the plane answers a lease response");
        let lease = response.lease.expect("an issued lease");
        Issued {
            lease_id: lease.lease_id.into_owned(),
            event_id: lease.event.event_id.into_owned(),
            fencing_token: lease.fencing_token,
            history: lease
                .history
                .into_iter()
                .map(|turn| (turn.message.into_owned(), turn.answer.into_owned()))
                .collect(),
        }
    }

    /// `runner` reports `lease` processed with `answer`, through the plane.
    async fn answer(&self, runner: &Uuid7, lease: &Issued, answer: &str, now: i64) {
        let request = ReportRequest {
            lease_id: Cow::Borrowed(&lease.lease_id),
            event_id: Cow::Borrowed(&lease.event_id),
            fencing_token: lease.fencing_token,
            outcome: Outcome::Processed,
            failure_reason: None,
            failure_detail: Cow::Borrowed(""),
            response_text: Cow::Borrowed(answer),
            tokens: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            telemetry: ReportTelemetry {
                time_to_first_token_ms: 0,
                wall_ms: SLICE_MS.unsigned_abs(),
            },
            checkpoint: ReportCheckpoint {
                last_event_id: Cow::Borrowed(&lease.event_id),
                last_response: Cow::Borrowed(answer),
            },
            tool_calls: None,
            held_until_ms: None,
        };
        self.fixtures
            .plane()
            .report(runner, &request, UnixMillis::from_millis(now))
            .await
            .expect("the plane settles the report");
    }

    /// Asks [`FIRST_MESSAGE`], leases it and answers [`FIRST_ANSWER`]: one
    /// finished turn, settled by `ENROLLED_AT + SLICE_MS`.
    async fn first_turn(&self) {
        self.ask(EventType::Chat, FIRST_MESSAGE, ENROLLED_AT).await;
        let first = self.lease(&self.runner, ENROLLED_AT).await;
        self.answer(&self.runner, &first, FIRST_ANSWER, ENROLLED_AT + SLICE_MS)
            .await;
    }
}

/// The finished turn the first run of each case leaves.
fn first_turn() -> Vec<(String, String)> {
    vec![(FIRST_MESSAGE.to_owned(), FIRST_ANSWER.to_owned())]
}

/// A chat message leased after the fleet answered the one before it carries
/// that message and answer in its lease's `history`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_follow_up_lease_carries_the_previous_turn() {
    let chat = Chat::open().await;
    chat.first_turn().await;
    let asked = chat
        .ask(EventType::Chat, FOLLOW_UP, ENROLLED_AT + SLICE_MS)
        .await;

    let follow_up = chat.lease(&chat.runner, ENROLLED_AT + 2 * SLICE_MS).await;

    assert_eq!(follow_up.event_id, asked);
    assert_eq!(follow_up.history, first_turn());
    chat.fixtures.cleanup().await;
}

/// Two messages queued at once: the second waits while the first holds the
/// fleet's slot, then leases after the first's report and reads it as a
/// finished turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_second_message_waits_for_the_first_and_carries_it() {
    let chat = Chat::open().await;
    let asked = chat.ask(EventType::Chat, FIRST_MESSAGE, ENROLLED_AT).await;
    let second = chat.ask(EventType::Chat, FOLLOW_UP, ENROLLED_AT + 1).await;

    let first = chat.lease(&chat.runner, ENROLLED_AT).await;
    let waiting = chat.claim(&chat.spare, ENROLLED_AT + 1).await;
    chat.answer(&chat.runner, &first, FIRST_ANSWER, ENROLLED_AT + SLICE_MS)
        .await;
    let follow_up = chat.lease(&chat.spare, ENROLLED_AT + 2 * SLICE_MS).await;

    assert_eq!(first.event_id, asked);
    assert!(
        first.history.is_empty(),
        "nothing finished before the first"
    );
    assert!(waiting.is_none(), "one fleet, one holder: the second waits");
    assert_eq!(follow_up.event_id, second);
    assert_eq!(follow_up.history, first_turn());
    chat.fixtures.cleanup().await;
}

/// A webhook leased after a finished chat turn carries no history: turns are
/// for chat leases alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_non_chat_lease_carries_no_history_through_the_plane() {
    let chat = Chat::open().await;
    chat.first_turn().await;
    let delivered = chat
        .ask(EventType::Webhook, "a push landed", ENROLLED_AT + SLICE_MS)
        .await;

    let webhook = chat.lease(&chat.runner, ENROLLED_AT + 2 * SLICE_MS).await;

    assert_eq!(webhook.event_id, delivered);
    assert!(webhook.history.is_empty(), "{:?}", webhook.history);
    chat.fixtures.cleanup().await;
}
