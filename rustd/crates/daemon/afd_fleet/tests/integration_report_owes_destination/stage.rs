//! One fleet, funded and leasable, and the verbs a report-owes case drives it
//! through: admit, run, report, read the ledger back.
//!
//! Split from the cases in `integration_report_owes_destination/mod.rs` at the file
//! cap. The cases state what is owed; this is the stage they state it on.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use afd_connector::Provider;
use afd_fleet::run::{Billed, Committed, Owing, Reported, Runs};
use afd_turns::{Admission, Key, Producer, Reply};
use jiff::Timestamp;
use sqlx::Row as _;

use crate::recovery_seed::{admission, ledger, producer_key};
use crate::report_commit::{RESPONSE_ACCEPTED, RESUME_EVENT_ID, RESUME_RESPONSE, report};
use crate::report_seed::{DEEP_POOL, SLICE_MS};
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts};
use crate::support::Fixtures;

/// How far apart consecutive runs on one fleet are stamped, so each run is
/// issued after the one before it settled.
const RUN_SPACING_MS: i64 = 10 * SLICE_MS;

/// One owed row, as the ledger holds it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub(crate) provider: String,
    pub(crate) destination: Option<String>,
    pub(crate) event_id: Uuid7,
    pub(crate) answer: String,
}

/// Every delivery this fleet holds, oldest first.
pub(crate) async fn deliveries(fixtures: &Fixtures, fleet: &Uuid7) -> Vec<Delivery> {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "SELECT provider, destination, event_id, answer FROM thread.deliveries \
         WHERE fleet_id = $1 ORDER BY created_at, sequence",
    )
    .bind(fleet)
    .fetch_all(&mut *connection)
    .await
    .expect("the ledger answers")
    .into_iter()
    .map(|row| Delivery {
        provider: row.try_get(0).expect("provider is text"),
        destination: row.try_get(1).expect("destination is nullable text"),
        event_id: row.try_get(2).expect("event_id is a uuid"),
        answer: row.try_get(3).expect("answer is text"),
    })
    .collect()
}

/// One fleet with a funded tenant and a runner to run on it.
pub(crate) struct Stage {
    pub(crate) fixtures: Fixtures,
    pub(crate) runs: Runs,
    pub(crate) fleet: Uuid7,
    pub(crate) workspace: Uuid7,
    pub(crate) tenant: Uuid7,
    pub(crate) runner: Uuid7,
}

pub(crate) async fn stage() -> Stage {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    fixtures.seed_wallet(&tenant, DEEP_POOL).await;
    let runs = fixtures.runs();
    Stage {
        fixtures,
        runs,
        fleet,
        workspace,
        tenant,
        runner,
    }
}

impl Stage {
    pub(crate) async fn admit_thread(&self, delivery: &str) -> Uuid7 {
        self.admit(
            Producer::Webhook,
            delivery,
            Reply::To {
                connector: Provider::Slack.id(),
                address: super::THREAD,
            },
        )
        .await
    }

    pub(crate) async fn report_through_plane(
        &self,
        run_id: &Uuid7,
        run: &Reported,
        now: Timestamp,
    ) {
        let request = ReportRequest {
            folded_through_sequence: None,
            setup: None,
            task: None,
            run_id: Cow::Borrowed(run_id.as_str()),
            event_id: Cow::Borrowed(run.event_id.as_str()),
            fencing_token: run.fence.as_u64(),
            outcome: Outcome::Processed,
            failure_reason: None,
            failure_detail: Cow::Borrowed(""),
            response_text: Cow::Borrowed(RESPONSE_ACCEPTED),
            tokens: 0,
            requests: af_wire::usage::RequestLines::default(),
            telemetry: ReportTelemetry {
                time_to_first_token_ms: 0,
                wall_ms: u64::try_from(SLICE_MS).expect("a slice is positive"),
                segments: None,
            },
            checkpoint: ReportCheckpoint {
                last_event_id: Cow::Borrowed(RESUME_EVENT_ID),
                last_response: Cow::Borrowed(RESUME_RESPONSE),
            },
            tool_calls: None,
            held_until: None,
            memory: Vec::new(),
        };
        self.fixtures
            .plane()
            .report_with(
                &self.runner,
                &request,
                af_core::clock::after(
                    now,
                    std::time::Duration::from_millis(
                        u64::try_from(SLICE_MS).expect("a fixture span is not negative"),
                    ),
                ),
                None,
                &mut af_observability::verb_span::VerbClock::untimed(),
            )
            .await
            .expect("the plane settles the report");
    }

    /// The receipt of the row `event` owes, if the report owed one.
    pub(crate) async fn receipt(&self, event: &Uuid7) -> Option<Uuid7> {
        let mut connection = self
            .fixtures
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        sqlx::query_scalar(
            "SELECT receipt FROM thread.deliveries
          WHERE fleet_id = $1 AND event_id = $2",
        )
        .bind(&self.fleet)
        .bind(event)
        .fetch_optional(&mut *connection)
        .await
        .expect("the delivery read answers")
    }

    /// Admits one event from `producer`, stating `reply`.
    pub(crate) async fn admit(
        &self,
        producer: Producer,
        delivery: &str,
        reply: Reply<'_>,
    ) -> Uuid7 {
        let key = producer_key(&self.fleet, delivery);
        ledger(&self.fixtures)
            .admit(Admission {
                producer,
                key: Key::Repeated(&key),
                reply,
                ..admission(&self.fleet, &self.workspace, &key)
            })
            .await
            .expect("the ledger admits")
            .stored
            .id
    }

    /// Runs this fleet's next event at `now` and loads the run a report
    /// addresses.
    pub(crate) async fn assign_next(&self, now: Timestamp) -> (Uuid7, Reported) {
        let acquired = crate::seed::select_fleet(&self.runs, &self.runner, &self.fleet)
            .await
            .expect("one rotation of polls must reach the fleet holding admitted work");
        let issued = self
            .runs
            .issue(
                &self.runner,
                &acquired,
                Billed {
                    tenant_id: &self.tenant,
                    posture: POSTURE,
                    provider: PROVIDER,
                    model: MODEL,
                },
                now,
            )
            .await
            .expect("the run row must be written")
            .expect("the claim is still held");
        let run = self
            .runs
            .load_for_report(&issued.run_id, &self.runner)
            .await
            .expect("the run load must reach the datastore")
            .expect("the run belongs to this runner");
        (issued.run_id, run)
    }

    /// Reports `response` on a run, a slice after it was issued.
    pub(crate) async fn report(
        &self,
        run_id: &Uuid7,
        run: &Reported,
        response: &str,
        issued_at: Timestamp,
    ) -> afd_fleet::Result<Committed> {
        let pricing = self.fixtures.pricing(run_id).await;
        self.runs
            .commit_report(report(
                run_id,
                &self.runner,
                run,
                (run_id, &pricing),
                response,
                af_core::clock::after(
                    issued_at,
                    std::time::Duration::from_millis(
                        u64::try_from(SLICE_MS).expect("a fixture span is not negative"),
                    ),
                ),
            ))
            .await
    }

    /// Reports and settles the next admission, answering what was owed.
    pub(crate) async fn run_next(&self, run: i64) -> Committed {
        let now = af_core::clock::after(
            self.fixtures.enrolled_at(),
            std::time::Duration::from_millis(
                u64::try_from(run * RUN_SPACING_MS).expect("a fixture span is not negative"),
            ),
        );
        let (run_id, run) = self.assign_next(now).await;
        self.report(&run_id, &run, RESPONSE_ACCEPTED, now)
            .await
            .expect("the report must reach the datastore")
    }
}

/// What a settled report newly owed, failing loudly on any other ending.
pub(crate) fn owed_by(committed: Committed) -> Option<Owing> {
    let Committed::Settled { owed, .. } = committed else {
        unreachable!("the only holder of this fleet cannot be fenced out of its own report")
    };
    owed
}
