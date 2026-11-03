//! One leasable chat fleet, leased and answered through the plane, for the
//! suites that read what a lease carries: its earlier turns and its memory.

#![expect(
    clippy::panic,
    reason = "a lease of the wrong kind fails the proof loudly"
)]

use std::borrow::Cow;
use std::time::Duration;

use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_db::test_util::{EventKind, FleetEventSeed, PlatformDefault};
use afd_fleet::lease::Plane;
use afd_wire::event::EventType;
use afd_wire::lease::LeaseResponse;
use afd_wire::memory::MemoryDelta;
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use jiff::Timestamp;

use super::seed::seed_provider_resolution;
use super::{NO_LEASE, READ_BOUND_CONFIG, set_config};
use crate::report_seed::{DEEP_POOL, SLICE, SLICE_MS};
use crate::seed::{ACTOR, seeded_parts};
use crate::support::Fixtures;

/// What the first run of each case is asked, and what it answers.
pub(super) const FIRST_MESSAGE: &str = "which tests failed?";
pub(super) const FIRST_ANSWER: &str = "two: a and b";

/// What the follow-up asks.
pub(super) const FOLLOW_UP: &str = "fix the second one";

/// How much later than the fixture's instant a follow-up is stamped: past
/// every row the first turn writes on the database clock, as a message sent
/// after the first was answered is.
pub(super) const ASKED_LATER: Duration = Duration::from_secs(60);

/// A run the fleet finished after the event a case leases.
const LATER_EVENT_ID: &str = "finished-after-the-leased-event";

/// One leasable fleet with two runners, and the provider default its leases
/// resolve against, held for the case's life.
pub(super) struct Chat {
    pub(super) fixtures: Fixtures,
    fleet: String,
    workspace: String,
    pub(super) runner: Uuid7,
    pub(super) spare: Uuid7,
    _default: PlatformDefault,
}

/// What a case reads off an issued lease.
pub(super) struct Issued {
    lease_id: String,
    pub(super) event_id: String,
    fencing_token: u64,
    pub(super) history: Vec<(String, String)>,
    /// The memory window's entries, key and content, or `None` when the lease
    /// carried no window.
    pub(super) memory: Option<Vec<(String, String)>>,
}

impl Chat {
    pub(super) async fn open() -> Self {
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
    pub(super) async fn ask(&self, event_type: EventType, message: &str, at: Timestamp) -> String {
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
    pub(super) async fn finished(&self, message: &str, answer: &str, at: Timestamp) {
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
    pub(super) async fn claim(&self, runner: &Uuid7) -> Option<afd_fleet::lease::Acquired> {
        let leases = self.fixtures.leases();
        crate::seed::select_fleet(&leases, runner, &self.fleet).await
    }

    /// The lease the plane issues `runner` for this fleet's next event.
    pub(super) async fn lease(&self, runner: &Uuid7, now: Timestamp) -> Issued {
        self.lease_through(&self.fixtures.plane(), runner, now)
            .await
    }

    /// [`Self::lease`], issued by `plane`.
    pub(super) async fn lease_through(
        &self,
        plane: &Plane,
        runner: &Uuid7,
        now: Timestamp,
    ) -> Issued {
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
            memory: lease.memory.map(|window| {
                window
                    .memory
                    .into_iter()
                    .map(|entry| (entry.key.into_owned(), entry.content.into_owned()))
                    .collect()
            }),
        }
    }

    /// `runner` reports `lease` processed with `answer`, through the plane.
    pub(super) async fn answer(
        &self,
        runner: &Uuid7,
        lease: &Issued,
        answer: &str,
        now: Timestamp,
    ) {
        self.answer_remembering(runner, lease, answer, Vec::new(), now)
            .await;
    }

    /// [`Self::answer`], the report carrying `memory` for the fleet's next run.
    pub(super) async fn answer_remembering(
        &self,
        runner: &Uuid7,
        lease: &Issued,
        answer: &str,
        memory: Vec<MemoryDelta<'_>>,
        now: Timestamp,
    ) {
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
                segments: None,
            },
            checkpoint: ReportCheckpoint {
                last_event_id: Cow::Borrowed(&lease.event_id),
                last_response: Cow::Borrowed(answer),
            },
            tool_calls: None,
            held_until: None,
            memory,
        };
        self.fixtures
            .plane()
            .report(runner, &request, now)
            .await
            .expect("the plane settles the report");
    }

    /// Asks [`FIRST_MESSAGE`], leases it and answers [`FIRST_ANSWER`]: one
    /// finished turn, settled one [`SLICE`] after the fixture's instant.
    pub(super) async fn first_turn(&self) {
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
pub(super) fn first_turn() -> Vec<(String, String)> {
    vec![(FIRST_MESSAGE.to_owned(), FIRST_ANSWER.to_owned())]
}
