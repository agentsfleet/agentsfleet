//! One leasable chat fleet, held and answered through the plane, for the
//! suites that read what a run carries: its earlier turns and its memory.

#![expect(
    clippy::panic,
    reason = "a run of the wrong kind fails the proof loudly"
)]

use std::borrow::Cow;
use std::time::Duration;

use af_core::id::Uuid7;
use af_core::turn::status;
use af_wire::memory::MemoryDelta;
use af_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use af_wire::run::TaggedRun;
use af_wire::turn::TurnKind;
use afd_db::test_util::{PlatformDefault, TurnFacts, TurnSeed, mint_id};
use afd_fleet::run::RunService;
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

/// One leasable fleet with two runners, and the provider default its runs
/// resolve against, held for the case's life.
pub(super) struct Chat {
    pub(super) fixtures: Fixtures,
    fleet: Uuid7,
    workspace: Uuid7,
    pub(super) runner: Uuid7,
    pub(super) spare: Uuid7,
    _default: PlatformDefault,
}

/// What a case reads off an issued run.
pub(super) struct Issued {
    run_id: Uuid7,
    pub(super) turn_id: Uuid7,
    fencing_token: u64,
    pub(super) history: Vec<(String, String)>,
    /// The memory window's entries, key and content, or `None` when the run
    /// carried no window.
    pub(super) memory: Option<Vec<(String, String)>>,
}

impl Chat {
    pub(super) async fn open() -> Self {
        let fixtures = Fixtures::create().await;
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
    pub(super) async fn ask(&self, event_type: TurnKind, message: &str, at: Timestamp) -> Uuid7 {
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
    /// straight to the fleet's log as a run finished after the held one.
    pub(super) async fn finished(&self, message: &str, answer: &str, at: Timestamp) {
        let mut connection = self
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection");
        let kind = TurnFacts {
            actor: ACTOR,
            event_type: TurnKind::Chat.as_str(),
            status: status::PROCESSED,
        };
        // A run the fleet finished after the event a case runs.
        let later = mint_id();
        TurnSeed::new(&self.fleet, &self.workspace, &later, kind)
            .at(at)
            .insert(&mut *connection)
            .await
            .expect("seeding a finished test_run");
        sqlx::query(
            "UPDATE thread.turns SET request_json = $2::jsonb, answer_text = $3 \
             WHERE id = $1",
        )
        .bind(&later)
        .bind(serde_json::json!({ "message": message }).to_string())
        .bind(answer)
        .execute(&mut *connection)
        .await
        .expect("the finished test_run carries its turn");
    }

    /// The fleet every run here is for.
    pub(super) const fn fleet(&self) -> &Uuid7 {
        &self.fleet
    }

    /// `runner`'s assignment on this fleet, or nothing while another holds it.
    pub(super) async fn assign(&self, runner: &Uuid7) -> Option<afd_fleet::run::Acquired> {
        let runs = self.fixtures.runs();
        crate::seed::select_fleet(&runs, runner, &self.fleet).await
    }

    /// The run the plane issues `runner` for this fleet's next event.
    pub(super) async fn run(&self, runner: &Uuid7, now: Timestamp) -> Issued {
        self.run_through(&self.fixtures.plane(), runner, now).await
    }

    /// [`Self::run`], issued by `plane`.
    pub(super) async fn run_through(
        &self,
        plane: &RunService,
        runner: &Uuid7,
        now: Timestamp,
    ) -> Issued {
        let assigned = self.assign(runner).await.expect("the fleet is leasable");
        let answer = plane
            .assign_selected(assigned, runner, now)
            .await
            .expect("every gate verdict is a decision, not a fault");
        assert!(answer.is_some(), "the run was refused: {answer:?}");
        let body = answer.expect("an issued run").to_string();
        let TaggedRun::Fleet(run) =
            serde_json::from_str::<TaggedRun<'_>>(&body).expect("the issued fleet payload")
        else {
            panic!("the fleet plane grants fleet work");
        };
        Issued {
            run_id: Uuid7::parse(&run.run_id).expect("the run names a UUIDv7"),
            turn_id: Uuid7::parse(&run.event.turn_id).expect("the event is a UUIDv7"),
            fencing_token: run.fencing_token,
            history: run
                .history
                .into_iter()
                .map(|turn| (turn.message.into_owned(), turn.answer.into_owned()))
                .collect(),
            memory: run.memory.map(|window| {
                window
                    .memory
                    .into_iter()
                    .map(|entry| (entry.key.into_owned(), entry.content.into_owned()))
                    .collect()
            }),
        }
    }

    /// `runner` reports `run` processed with `answer`, through the plane.
    pub(super) async fn answer(&self, runner: &Uuid7, run: &Issued, answer: &str, now: Timestamp) {
        self.answer_remembering(runner, run, answer, Vec::new(), now)
            .await;
    }

    /// [`Self::answer`], the report carrying `memory` for the fleet's next run.
    pub(super) async fn answer_remembering(
        &self,
        runner: &Uuid7,
        run: &Issued,
        answer: &str,
        memory: Vec<MemoryDelta<'_>>,
        now: Timestamp,
    ) {
        let request = ReportRequest {
            folded_through_sequence: None,
            setup: None,
            task: None,
            run_id: Cow::Borrowed(run.run_id.as_str()),
            turn_id: Cow::Borrowed(run.turn_id.as_str()),
            fencing_token: run.fencing_token,
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
                last_turn_id: Cow::Borrowed(run.turn_id.as_str()),
                last_response: Cow::Borrowed(answer),
            },
            tool_calls: None,
            held_until: None,
            memory,
        };
        self.fixtures
            .plane()
            .report_with(
                runner,
                &request,
                now,
                None,
                &mut af_observability::verb_span::VerbClock::untimed(),
            )
            .await
            .expect("the plane settles the report");
    }

    /// Asks [`FIRST_MESSAGE`], runs it and answers [`FIRST_ANSWER`]: one
    /// finished turn, settled one [`SLICE`] after the fixture's instant.
    pub(super) async fn first_turn(&self) {
        self.ask(TurnKind::Chat, FIRST_MESSAGE, self.fixtures.enrolled_at())
            .await;
        let first = self.run(&self.runner, self.fixtures.enrolled_at()).await;
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
