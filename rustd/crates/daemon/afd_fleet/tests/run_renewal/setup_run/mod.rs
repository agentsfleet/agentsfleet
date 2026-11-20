//! A draft's setup turn on the common run path, against the live datastores:
//! the production assignment hands a host a draft run, and `report_draft`
//! settles it.
//!
//! Each case owns a private database, because placement takes the oldest
//! queued draft across every tenant: on the shared lane a sibling suite's
//! draft could win the host this case polls with.

use std::borrow::Cow;
use std::sync::Arc;

use af_core::{id::Uuid7, money::NANOS_PER_USD};
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use af_wire::run::{AcceptedKinds, SetupProposal, SetupReport, TaggedRun};
use afd_fleet::run::AssignRequest;
use afd_setup::types::Draft;

use super::{fixture::Running, replica::expire, seat::Seat};
use crate::rows::{LAPSE, Row};

mod context;
mod expiry;
mod failed;
mod faults;
mod mint;
mod notice;
mod page;
mod placement;
mod progress;
mod refused;
mod released;
mod saving;
mod superseded;

/// The key the seat's model secret is sealed under, as the plane opens it.
const KEK_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// What the setup turn answered.
const ANSWER: &str = "A daily check at nine, UTC.";

/// The draft run a host was given.
struct Taken {
    run: String,
    turn: String,
    fence: u64,
    /// The head candidate the run's setup starts from: its id and revision.
    base: Option<(String, u64)>,
}

impl Taken {
    fn run_id(&self) -> Uuid7 {
        Uuid7::parse(&self.run).expect("a run id is a UUIDv7")
    }

    fn turn_id(&self) -> Uuid7 {
        Uuid7::parse(&self.turn).expect("a turn id is a UUIDv7")
    }

    /// The head this run's setup starts from.
    fn base(&self) -> (&str, u64) {
        let (id, revision) = self
            .base
            .as_ref()
            .expect("the seat's draft starts from its source candidate");
        (id, *revision)
    }
}

/// What a draft run's report left on its turn.
#[derive(Debug)]
struct TurnRow {
    status: String,
    answer: Option<String>,
    candidate: Option<Uuid7>,
    failure_reason: Option<String>,
    failure_detail: Option<String>,
}

/// One queued draft turn on a private database, the host freed for it.
struct Case {
    running: Running,
    seat: Seat,
    drafts: afd_setup::Drafts,
    draft: Draft,
}

impl Case {
    /// The seat's draft with one queued turn, and the fixture's runtime run
    /// lapsed so its one worker is free.
    async fn queued() -> Self {
        let running = Running::create_isolated().await;
        let seat = Seat::create(&running).await;
        let drafts = Seat::drafts(&running.fixtures.database);
        let draft = seat.queue(&drafts).await;
        expire(&running).await;
        Self {
            running,
            seat,
            drafts,
            draft,
        }
    }

    /// `runner`'s poll for setup work: the draft run the production
    /// assignment placed, or none.
    async fn take(&self, runner: &Uuid7) -> Option<Taken> {
        self.running
            .fixtures
            .plane()
            .assign_with(
                &self.drafts,
                AssignRequest {
                    runner,
                    held: &[],
                    degraded: false,
                    now: self.running.now,
                    recovery: None,
                },
                AcceptedKinds::ALL,
                &mut af_observability::verb_span::VerbClock::untimed(),
                |run, _metadata| {
                    let TaggedRun::Fleet(run) = run;
                    run.owner.setup().map(|setup| Taken {
                        run: run.run_id.to_string(),
                        turn: run.event.turn_id.to_string(),
                        fence: run.fencing_token,
                        base: setup
                            .candidate
                            .as_ref()
                            .map(|head| (head.id.to_string(), head.revision)),
                    })
                },
            )
            .await
            .expect("the poll answers")
            .flatten()
    }

    /// The fixture host's poll, which must place the queued draft turn.
    async fn take_on_host(&self) -> Taken {
        self.take(&self.running.runner)
            .await
            .expect("the queued draft turn is placed as a draft run")
    }

    /// The host's report, through the production report path.
    async fn report(&self, request: &ReportRequest<'_>) -> Result<(), afd_fleet::Error> {
        let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
        let creations = afd_setup::Creations::new(
            afd_setup::Setup::new(
                self.drafts.clone(),
                Arc::new(Kek::from_hex(KEK_HEX).expect("fixture key")),
            ),
            afd_turns::Turns::new(self.running.fixtures.database.clone(), bus, Entropy::new()),
        );
        self.running
            .fixtures
            .plane()
            .report_draft(
                &creations,
                &self.running.runner,
                request,
                af_core::clock::now(),
            )
            .await
    }

    /// Lapses `run` and the draft's thread, as a holder that stopped renewing
    /// leaves them.
    async fn lapse(&self, run: &Uuid7) {
        let fixtures = &self.running.fixtures;
        fixtures.age(Row::Run(run), "run_expires_at", LAPSE).await;
        // A draft's thread is keyed by the draft, which `Row::Thread` (a
        // fleet's) does not name.
        let aged = sqlx::query(
            "UPDATE thread.threads SET run_expires_at = run_expires_at - $2::interval \
             WHERE id = $1",
        )
        .bind(&self.draft.id)
        .bind(LAPSE)
        .execute(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the draft's thread ages");
        assert_eq!(aged.rows_affected(), 1, "the draft holds one thread");
    }

    /// The turn `turn` as its report left it.
    async fn turn(&self, turn: &Uuid7) -> TurnRow {
        let (status, answer, candidate, failure_reason, failure_detail) = sqlx::query_as(
            "SELECT status, answer_text, candidate_id, failure_reason, failure_detail \
             FROM thread.turns WHERE id = $1 AND thread_id = $2",
        )
        .bind(turn)
        .bind(&self.draft.id)
        .fetch_one(&mut *self.connection().await)
        .await
        .expect("the draft's turn");
        TurnRow {
            status,
            answer,
            candidate,
            failure_reason,
            failure_detail,
        }
    }

    /// The draft's candidates: how many, and which is the head.
    async fn candidates(&self) -> (i64, Option<Uuid7>) {
        sqlx::query_as(
            "SELECT (SELECT count(*) FROM setup.candidates c WHERE c.draft_id = d.id), \
                    d.head_candidate_id \
             FROM setup.drafts d WHERE d.id = $1",
        )
        .bind(&self.draft.id)
        .fetch_one(&mut *self.connection().await)
        .await
        .expect("the draft and its candidates")
    }

    /// The person's next message on the draft, asking to save the setup the
    /// turn ends on, through the production send path.
    async fn send_saving(&self) {
        let accepted = self
            .drafts
            .send(
                self.seat.access(),
                &self.draft.id,
                afd_setup::types::SendMessage {
                    message: "Save it".to_owned(),
                    save: true,
                    operation_id: afd_db::test_util::mint_id(),
                },
                af_core::clock::now(),
            )
            .await
            .expect("the save message is accepted");
        assert_eq!(accepted.sequence, "2", "the second message on the draft");
    }

    /// The fleet a save linked the draft to, if any.
    async fn saved_fleet(&self) -> Option<Uuid7> {
        sqlx::query_scalar("SELECT fleet_id FROM setup.drafts WHERE id = $1")
            .bind(&self.draft.id)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the draft")
    }

    /// The draft's runs, by fence: each fence and status.
    async fn runs(&self) -> Vec<(i64, String)> {
        sqlx::query_as(
            "SELECT fencing_token, status FROM thread.runs WHERE draft_id = $1 \
             ORDER BY fencing_token",
        )
        .bind(&self.draft.id)
        .fetch_all(&mut *self.connection().await)
        .await
        .expect("the draft's runs")
    }

    async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.running
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection")
    }

    async fn cleanup(self) {
        let Self { running, seat, .. } = self;
        seat.remove(&running.fixtures.database).await;
        running.cleanup().await;
    }
}

/// A Definition the seat's draft could hold, whose one cron trigger fires on
/// `schedule`.
fn definition(schedule: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "description": "Explain failures.",
        "instructions_md": "Investigate failures.\n",
        "config": {"name": "checkout-watch", "x-agentsfleet": {
            "triggers": [{"type": "cron", "schedule": schedule, "timezone": "UTC", "message": "Daily check"}],
            "tools": [], "credentials": [], "network": {"allow": []},
            "gates": {"timeout_ms": 60000, "rules": []},
            "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
        }}
    })
    .to_string()
}

/// A proposal of `definition` on the head `taken`'s setup started from.
fn proposal<'a>(taken: &'a Taken, definition: &'a str) -> SetupProposal<'a> {
    let (base, revision) = taken.base();
    SetupProposal {
        base_candidate_id: Some(Cow::Borrowed(base)),
        base_revision: revision,
        definition: Cow::Borrowed(definition),
    }
}

/// The report a runner sends when `taken` answered [`ANSWER`] and kept
/// `proposal`, at `taken`'s own fence and with no usage.
fn processed<'a>(taken: &'a Taken, proposal: Option<SetupProposal<'a>>) -> ReportRequest<'a> {
    ReportRequest {
        folded_through_sequence: None,
        setup: Some(SetupReport { proposal }),
        task: None,
        run_id: Cow::Borrowed(&taken.run),
        turn_id: Cow::Borrowed(&taken.turn),
        fencing_token: taken.fence,
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(ANSWER),
        tokens: 0,
        requests: af_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: 1,
            segments: None,
        },
        checkpoint: ReportCheckpoint {
            last_turn_id: Cow::Borrowed(&taken.turn),
            last_response: Cow::Borrowed(ANSWER),
        },
        tool_calls: None,
        held_until: None,
        memory: Vec::new(),
    }
}
