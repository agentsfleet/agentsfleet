//! One leasable chat fleet, leased and answered through the plane, for the
//! suites that read what a lease carries: its earlier turns and its memory.

#![expect(
    clippy::panic,
    reason = "a lease of the wrong kind fails the proof loudly"
)]

use std::borrow::Cow;
use std::time::Duration;

use af_core::event::status;
use af_core::id::Uuid7;
use af_wire::event::EventType;
use af_wire::lease::TaggedLease;
use af_wire::memory::MemoryDelta;
use af_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use afd_db::test_util::{EventKind, FleetEventSeed, PlatformDefault, mint_id};
use afd_fleet::lease::LeaseService;
use jiff::Timestamp;

use super::seed::seed_provider_resolution;
use super::{READ_BOUND_CONFIG, set_config};
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

/// One leasable fleet with two runners, and the provider default its leases
/// resolve against, held for the case's life.
pub(super) struct Chat {
    pub(super) fixtures: Fixtures,
    fleet: Uuid7,
    workspace: Uuid7,
    pub(super) runner: Uuid7,
    pub(super) spare: Uuid7,
    _default: PlatformDefault,
}

/// What a case reads off an issued lease.
pub(super) struct Issued {
    lease_id: Uuid7,
    pub(super) event_id: Uuid7,
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
    pub(super) async fn ask(&self, event_type: EventType, message: &str, at: Timestamp) -> Uuid7 {
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
        // A run the fleet finished after the event a case leases.
        let later = mint_id();
        FleetEventSeed::new(&self.fleet, &self.workspace, &later, kind)
            .at(at)
            .insert(&mut *connection)
            .await
            .expect("seeding a finished run");
        sqlx::query(
            "UPDATE fleet.events SET request_json = $2::jsonb, response_text = $3 \
             WHERE event_id = $1",
        )
        .bind(&later)
        .bind(serde_json::json!({ "message": message }).to_string())
        .bind(answer)
        .execute(&mut *connection)
        .await
        .expect("the finished run carries its turn");
    }

    /// The fleet every lease here is for.
    pub(super) const fn fleet(&self) -> &Uuid7 {
        &self.fleet
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
        plane: &LeaseService,
        runner: &Uuid7,
        now: Timestamp,
    ) -> Issued {
        let claimed = self.claim(runner).await.expect("the fleet is leasable");
        let answer = plane
            .claim_selected(claimed, runner, now)
            .await
            .expect("every gate verdict is a decision, not a fault");
        assert!(answer.is_some(), "the lease was refused: {answer:?}");
        let body = answer.expect("an issued lease").to_string();
        let TaggedLease::Fleet(lease) =
            serde_json::from_str::<TaggedLease<'_>>(&body).expect("the issued fleet payload")
        else {
            panic!("the fleet plane grants fleet work");
        };
        Issued {
            lease_id: Uuid7::parse(&lease.lease_id).expect("the lease names a UUIDv7"),
            event_id: Uuid7::parse(&lease.event.event_id).expect("the event is a UUIDv7"),
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
            folded_through_sequence: None,
            task: None,
            lease_id: Cow::Borrowed(lease.lease_id.as_str()),
            event_id: Cow::Borrowed(lease.event_id.as_str()),
            fencing_token: lease.fencing_token,
            outcome: Outcome::Processed,
            failure_reason: None,
            failure_detail: Cow::Borrowed(""),
            response_text: Cow::Borrowed(answer),
            tokens: 0,
            requests: af_wire::usage::RequestLines::default(),
            telemetry: ReportTelemetry {
                time_to_first_token_ms: 0,
                wall_ms: SLICE_MS.unsigned_abs(),
                segments: None,
            },
            checkpoint: ReportCheckpoint {
                last_event_id: Cow::Borrowed(lease.event_id.as_str()),
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
