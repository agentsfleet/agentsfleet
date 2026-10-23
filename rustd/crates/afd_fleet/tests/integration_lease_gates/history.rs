//! A chat lease's earlier turns, as the plane hands them to a runner.
//!
//! `integration_lease_history.rs` proves the read; these drive the path around
//! it: `Plane::lease_claimed` issues a lease, `Plane::report` ends it, and the
//! next lease's payload carries what that report left. They live under this
//! suite because an issued lease needs the provider seed only it owns.

#![expect(
    clippy::panic,
    reason = "a lease of the wrong kind fails the proof loudly"
)]

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_db::test_util::{EventKind, FleetEventSeed, PlatformDefault, unreachable_db};
use afd_events::History;
use afd_fleet::lease::Plane;
use afd_wire::event::EventType;
use afd_wire::lease::LeaseResponse;
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use jiff::Timestamp;

use super::seed::seed_provider_resolution;
use super::{NO_LEASE, READ_BOUND_CONFIG, set_config};
use crate::report_seed::{DEEP_POOL, SLICE, SLICE_MS};
use crate::seed::{ACTOR, seeded_parts};
use crate::support::Fixtures;

/// What the first run of each case is asked, and what it answers.
const FIRST_MESSAGE: &str = "which tests failed?";
const FIRST_ANSWER: &str = "two: a and b";

/// What the follow-up asks.
const FOLLOW_UP: &str = "fix the second one";

/// How much later than the fixture's instant a follow-up is stamped: past
/// every row the first turn writes on the database clock, as a message sent
/// after the first was answered is.
const ASKED_LATER: Duration = Duration::from_secs(60);

/// A run the fleet finished after the event a case leases.
const LATER_EVENT_ID: &str = "finished-after-the-leased-event";

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
        fixtures.seed_wallet(&tenant, DEEP_POOL).await;
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
    async fn ask(&self, event_type: EventType, message: &str, at: Timestamp) -> String {
        let request = serde_json::json!({ "message": message }).to_string();
        crate::queue::enqueue(
            &self.fixtures.database,
            &self.fleet,
            &self.workspace,
            ACTOR,
            event_type.as_str(),
            &request,
            at,
        )
        .await
    }

    /// A chat run asking `message` that finished `answer` at `at`, written
    /// straight to the fleet's log as a run finished after the leased one.
    async fn finished(&self, message: &str, answer: &str, at: Timestamp) {
        let mut connection = self
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection");
        let kind = EventKind {
            actor: ACTOR,
            event_type: EventType::Chat.as_str(),
            status: status::PROCESSED,
        };
        let row = FleetEventSeed::new(&self.fleet, &self.workspace, LATER_EVENT_ID, kind)
            .at(at)
            .insert(&mut *connection)
            .await
            .expect("seeding a finished run");
        sqlx::query(
            "UPDATE fleet.events SET request_json = $2::jsonb, response_text = $3 \
             WHERE id = $1::uuid",
        )
        .bind(row)
        .bind(serde_json::json!({ "message": message }).to_string())
        .bind(answer)
        .execute(&mut *connection)
        .await
        .expect("the finished run carries its turn");
    }

    /// `runner`'s claim on this fleet, or nothing while another holds it.
    async fn claim(&self, runner: &Uuid7) -> Option<afd_fleet::lease::Acquired> {
        let leases = self.fixtures.leases();
        crate::seed::select_fleet(&leases, runner, &self.fleet).await
    }

    /// The lease the plane issues `runner` for this fleet's next event.
    async fn lease(&self, runner: &Uuid7, now: Timestamp) -> Issued {
        self.lease_through(&self.fixtures.plane(), runner, now)
            .await
    }

    /// [`Self::lease`], issued by `plane`.
    async fn lease_through(&self, plane: &Plane, runner: &Uuid7, now: Timestamp) -> Issued {
        let claimed = self.claim(runner).await.expect("the fleet is leasable");
        let answer = plane
            .lease_claimed(claimed, runner, now)
            .await
            .expect("every gate verdict is a decision, not a fault");
        assert!(
            !answer.contains(NO_LEASE),
            "the lease was refused: {answer}"
        );
        let response: LeaseResponse<'_> =
            serde_json::from_str(&answer).expect("the plane answers a lease response");
        let afd_wire::lease::TaggedLease::Fleet(lease) = response.lease.expect("an issued lease")
        else {
            panic!("the fleet plane grants fleet work");
        };
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
    async fn answer(&self, runner: &Uuid7, lease: &Issued, answer: &str, now: Timestamp) {
        let request = ReportRequest {
            lease_id: Cow::Borrowed(&lease.lease_id),
            event_id: Cow::Borrowed(&lease.event_id),
            fencing_token: lease.fencing_token,
            outcome: Outcome::Processed,
            failure_reason: None,
            failure_detail: Cow::Borrowed(""),
            response_text: Cow::Borrowed(answer),
            tokens: 0,
            requests: afd_wire::usage::RequestLines::default(),
            telemetry: ReportTelemetry {
                time_to_first_token_ms: 0,
                wall_ms: SLICE_MS.unsigned_abs(),
            },
            checkpoint: ReportCheckpoint {
                last_event_id: Cow::Borrowed(&lease.event_id),
                last_response: Cow::Borrowed(answer),
            },
            tool_calls: None,
            held_until: None,
        };
        self.fixtures
            .plane()
            .report(runner, &request, now)
            .await
            .expect("the plane settles the report");
    }

    /// Asks [`FIRST_MESSAGE`], leases it and answers [`FIRST_ANSWER`]: one
    /// finished turn, settled one [`SLICE`] after the fixture's instant.
    async fn first_turn(&self) {
        self.ask(EventType::Chat, FIRST_MESSAGE, self.fixtures.enrolled_at())
            .await;
        let first = self.lease(&self.runner, self.fixtures.enrolled_at()).await;
        self.answer(
            &self.runner,
            &first,
            FIRST_ANSWER,
            self.fixtures.after(SLICE),
        )
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
        .ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;

    let follow_up = chat
        .lease(&chat.runner, chat.fixtures.after(SLICE * 2))
        .await;

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
    let asked = chat
        .ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;
    let second = chat
        .ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;

    let first = chat.lease(&chat.runner, chat.fixtures.enrolled_at()).await;
    let waiting = chat.claim(&chat.spare).await;
    chat.answer(
        &chat.runner,
        &first,
        FIRST_ANSWER,
        chat.fixtures.after(SLICE),
    )
    .await;
    let follow_up = chat
        .lease(&chat.spare, chat.fixtures.after(SLICE * 2))
        .await;

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
        .ask(
            EventType::Webhook,
            "a push landed",
            chat.fixtures.after(ASKED_LATER),
        )
        .await;

    let webhook = chat
        .lease(&chat.runner, chat.fixtures.after(SLICE * 2))
        .await;

    assert_eq!(webhook.event_id, delivered);
    assert!(webhook.history.is_empty(), "{:?}", webhook.history);
    chat.fixtures.cleanup().await;
}

/// A thread that will not answer costs a follow-up its turns, never its
/// lease: the plane issues it with no history rather than refusing the poll.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_follow_up_lease_is_issued_when_its_thread_will_not_answer() {
    let chat = Chat::open().await;
    chat.first_turn().await;
    let asked = chat
        .ask(EventType::Chat, FOLLOW_UP, chat.fixtures.after(ASKED_LATER))
        .await;
    let silent = Plane {
        thread: Arc::new(History::new(unreachable_db())),
        ..chat.fixtures.plane()
    };

    let follow_up = chat
        .lease_through(&silent, &chat.runner, chat.fixtures.after(SLICE * 2))
        .await;

    assert_eq!(follow_up.event_id, asked);
    assert!(follow_up.history.is_empty(), "{:?}", follow_up.history);
    chat.fixtures.cleanup().await;
}

/// A lease reads the turns before its own event: a run that finished after
/// the leased event was stamped is no turn of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lease_history_stops_at_its_own_event() {
    let chat = Chat::open().await;
    let asked = chat
        .ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;
    chat.finished(FOLLOW_UP, FIRST_ANSWER, chat.fixtures.after(SLICE))
        .await;

    let leased = chat
        .lease(&chat.runner, chat.fixtures.after(SLICE * 2))
        .await;

    assert_eq!(leased.event_id, asked);
    assert!(leased.history.is_empty(), "{:?}", leased.history);
    chat.fixtures.cleanup().await;
}

/// The issued-lease line carries how long its event waited for a runner, on
/// the daemon's own clock: admitted at T, polled at T + 350 ms, logged 350
/// plus the time the poll itself took to write the lease.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_runner_lease_issued_logs_queue_wait() {
    const WAITED_MS: i64 = 350;
    const WAITED: Duration = Duration::from_millis(WAITED_MS.unsigned_abs());
    let chat = Chat::open().await;
    chat.ask(EventType::Chat, FIRST_MESSAGE, chat.fixtures.enrolled_at())
        .await;

    let log = afd_core::test_util::trace::Capture::install();
    let polled = std::time::Instant::now();
    chat.lease(&chat.runner, chat.fixtures.after(WAITED)).await;
    let call_ms = i64::try_from(polled.elapsed().as_millis()).expect("a short call");
    let issued = log.only("runner_lease_issued");
    drop(log);

    let logged: i64 = issued
        .field("queue_wait_ms")
        .and_then(|millis| millis.parse().ok())
        .expect("a fresh lease logs its queue wait");
    assert!(
        (WAITED_MS..=WAITED_MS + call_ms).contains(&logged),
        "{logged} ms outside {WAITED_MS}..={} ms: {:?}",
        WAITED_MS + call_ms,
        issued.fields
    );
    chat.fixtures.cleanup().await;
}
