//! What the fold suites share: sending a message to the held fleet, reading a
//! claim, reading what a settle left behind, and reporting the held lease.
//!
//! Every helper returns its failure instead of panicking, so a test names the
//! step that broke by the `?` that carried it out.

use std::error::Error;

use af_core::id::Uuid7;
use afd_admission::{Admission, Admissions, Key, Producer, Reply, RunFacts};
use afd_fleet::lease::fold::Folds;
use afd_fleet::lease::{Committed, Verdict};

use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::Held;

/// What a test returns: `()`, or the value it read, or the step that failed.
pub(super) type Checked<T = ()> = Result<T, Box<dyn Error>>;

/// What a person sends mid-run.
pub(super) const STEER_JSON: &str = r#"{"message":"actually, use staging"}"#;
/// What [`STEER_JSON`] reads as to the model.
pub(super) const STEER_TEXT: &str = "actually, use staging";
/// What a webhook delivers mid-run.
pub(super) const WEBHOOK_JSON: &str = r#"{"action":"completed","workflow":"ci"}"#;

/// One message admitted to the held fleet.
#[derive(Debug)]
pub(super) struct Sent {
    /// The message's admission id, which is its event id.
    pub(super) id: String,
    /// Its place in the fleet's order.
    pub(super) sequence: i64,
    /// What the ledger said of the live run when it was admitted.
    pub(super) run: RunFacts,
}

impl Sent {
    /// This message as a claim answers it, reading as `text`.
    pub(super) fn claimed(&self, text: &str) -> Claimed {
        Claimed {
            id: self.id.clone(),
            sequence: self.sequence,
            text: text.to_owned(),
        }
    }
}

/// One message a claim answered.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Claimed {
    pub(super) id: String,
    pub(super) sequence: i64,
    pub(super) text: String,
}

/// An event row a fold opened: its status, its reply and the run it names.
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
pub(super) struct EventRow {
    pub(super) status: String,
    pub(super) response_text: Option<String>,
    pub(super) resumes_event_id: Option<String>,
}

/// Whether an admission is settled, and the event it opened once it has one.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Settlement {
    pub(super) settled: bool,
    pub(super) event: Option<EventRow>,
}

impl Settlement {
    /// A message nothing has read: unsettled, with no event of its own.
    pub(super) const UNREAD: Self = Self {
        settled: false,
        event: None,
    };
}

/// The workspace the held fleet belongs to.
pub(super) async fn workspace_of(held: &Held) -> Checked<String> {
    let mut connection = held.fixtures.database.acquire().await?;
    let workspace =
        sqlx::query_scalar("SELECT workspace_id::text FROM fleet.fleets WHERE id = $1::uuid")
            .bind(&held.fleet)
            .fetch_one(&mut *connection)
            .await?;
    Ok(workspace)
}

/// Admits one message from `producer` to the held fleet.
pub(super) async fn send(
    held: &Held,
    workspace: &str,
    producer: Producer,
    request_json: &str,
) -> Checked<Sent> {
    send_to(held, workspace, producer, request_json, Reply::None).await
}

/// [`send`], owing its reply to `reply`.
pub(super) async fn send_to(
    held: &Held,
    workspace: &str,
    producer: Producer,
    request_json: &str,
    reply: Reply<'_>,
) -> Checked<Sent> {
    let admissions =
        Admissions::for_tests(held.fixtures.database.clone(), crate::queue::unreachable());
    let admitted = admissions
        .admit(Admission {
            producer,
            key: Key::Unrepeatable,
            fleet: &held.fleet,
            workspace,
            actor: crate::seed::ACTOR,
            event_type: af_wire::event::EventType::Chat,
            request_json,
            reply,
        })
        .await?;
    let mut connection = held.fixtures.database.acquire().await?;
    let sequence = sqlx::query_scalar("SELECT sequence FROM fleet.admissions WHERE id = $1::uuid")
        .bind(&admitted.stored.id)
        .fetch_one(&mut *connection)
        .await?;
    Ok(Sent {
        id: admitted.stored.id,
        sequence,
        run: admitted.run,
    })
}

/// Runs one statement that names one admission and changes it.
async fn change(held: &Held, statement: &'static str, admission: &str) -> Checked {
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query(statement)
        .bind(admission)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Parks `admission`, as an approval gate does.
pub(super) async fn park(held: &Held, admission: &str) -> Checked {
    change(
        held,
        "UPDATE fleet.admissions SET parked_at = now() WHERE id = $1::uuid",
        admission,
    )
    .await
}

/// Settles `admission` the way an earlier run's report would have.
pub(super) async fn settle_directly(held: &Held, admission: &str) -> Checked {
    change(
        held,
        "UPDATE fleet.admissions SET settled_at = now() WHERE id = $1::uuid",
        admission,
    )
    .await
}

/// Gives `admission` a new row version, the newest in the table's storage.
pub(super) async fn touch(held: &Held, admission: &str) -> Checked {
    change(
        held,
        "UPDATE fleet.admissions SET updated_at = now() WHERE id = $1::uuid",
        admission,
    )
    .await
}

/// Points the held lease at a durable task, or at none.
pub(super) async fn set_lease_task(held: &Held, task: Option<&str>) -> Checked {
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.leases SET task_id = $2::uuid WHERE id = $1::uuid")
        .bind(held.issued.lease_id.as_str())
        .bind(task)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// What `runner` claims for the held lease under `fence`, after `after`.
pub(super) async fn claim(
    held: &Held,
    runner: &Uuid7,
    fence: u64,
    after: i64,
) -> Checked<Vec<Claimed>> {
    let response = held
        .leases
        .claim_folds(runner, &held.issued.lease_id, fence, after)
        .await?;
    Ok(response
        .inputs
        .into_iter()
        .map(|input| Claimed {
            id: input.event_id.into_owned(),
            sequence: input.sequence,
            text: input.text.into_owned(),
        })
        .collect())
}

/// [`claim`], as the runner holding the lease under its own fence.
pub(super) async fn claim_as_holder(held: &Held, after: i64) -> Checked<Vec<Claimed>> {
    claim(held, &held.runner, held.fence.as_u64(), after).await
}

/// Whether `admission` is settled, and the event it opened, if it has.
pub(super) async fn settled(held: &Held, admission: &str) -> Checked<Settlement> {
    let mut connection = held.fixtures.database.acquire().await?;
    let settled = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(admission)
    .fetch_one(&mut *connection)
    .await?;
    let event = sqlx::query_as(
        "SELECT status, response_text, resumes_event_id FROM fleet.events \
         WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(&held.fleet)
    .bind(admission)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(Settlement { settled, event })
}

/// Reports the held lease with `verdict`, having read through `through`, and
/// answers what the report settled.
pub(super) async fn report_read_through(
    held: &Held,
    through: i64,
    verdict: Verdict<'_>,
) -> Checked<Folds> {
    let lease = held
        .leases
        .load_for_report(held.issued.lease_id.as_str(), &held.runner)
        .await?
        .ok_or("the held lease loads for its report")?;
    let mut terminal = report(
        held.issued.lease_id.as_str(),
        &held.runner,
        &lease,
        (&held.lease_key, &held.pricing),
        RESPONSE_ACCEPTED,
        held.now,
    );
    terminal.outcome.verdict = verdict;
    terminal.folded_through_sequence = Some(through);
    match held.leases.commit_report(terminal).await? {
        Committed::Settled { folds, .. } => Ok(folds),
        Committed::AlreadySettled | Committed::Fenced { .. } => {
            Err("the only holder of this fleet cannot be fenced out".into())
        }
    }
}
