//! What the fold suites share: sending a message to the held fleet, reading a
//! claim, reading what a settle left behind, and reporting the held lease.
//!
//! Every helper returns its failure instead of panicking, so a test names the
//! step that broke by the `?` that carried it out.

use std::error::Error;

use af_core::id::Uuid7;
use afd_admission::{Admission, Admissions, Key, PersonSteer, Producer, Reply, RunFacts};
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
    pub(super) id: Uuid7,
    /// Its place in the fleet's order.
    pub(super) sequence: i64,
    /// What a steer meets of the live run (default for any other producer).
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
    pub(super) id: Uuid7,
    pub(super) sequence: i64,
    pub(super) text: String,
}

/// An event row a fold opened: its status, its reply and the run it names.
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
pub(super) struct EventRow {
    pub(super) status: String,
    pub(super) response_text: Option<String>,
    pub(super) resumes_event_id: Option<Uuid7>,
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
pub(super) async fn workspace_of(held: &Held) -> Checked<Uuid7> {
    let mut connection = held.fixtures.database.acquire().await?;
    let workspace = sqlx::query_scalar("SELECT workspace_id FROM fleet.fleets WHERE id = $1")
        .bind(&held.fleet)
        .fetch_one(&mut *connection)
        .await?;
    Ok(workspace)
}

/// The actor a person's steer to the held fleet records.
pub(super) fn person() -> String {
    afd_events::steer_actor("fixture-person")
}

/// Admits one message from `producer` to the held fleet: a steer from a
/// person, anything else from the fixture's own actor.
pub(super) async fn send(
    held: &Held,
    workspace: &Uuid7,
    producer: Producer,
    request_json: &str,
) -> Checked<Sent> {
    send_to(held, workspace, producer, request_json, Reply::None).await
}

/// [`send`], owing its reply to `reply`.
pub(super) async fn send_to(
    held: &Held,
    workspace: &Uuid7,
    producer: Producer,
    request_json: &str,
    reply: Reply<'_>,
) -> Checked<Sent> {
    let actor = if producer == Producer::Steer {
        person()
    } else {
        crate::seed::ACTOR.to_owned()
    };
    send_as(held, workspace, producer, &actor, request_json, reply).await
}

/// [`send_to`], recording `actor`.
pub(super) async fn send_as(
    held: &Held,
    workspace: &Uuid7,
    producer: Producer,
    actor: &str,
    request_json: &str,
    reply: Reply<'_>,
) -> Checked<Sent> {
    let admissions = Admissions::for_tests(
        held.fixtures.database.clone(),
        afd_bus::test_util::unreachable(),
    );
    let admitted = admissions
        .admit(Admission {
            producer,
            key: Key::Unrepeatable,
            fleet: &held.fleet,
            workspace,
            actor,
            event_type: af_wire::event::EventType::Chat,
            request_json,
            reply,
        })
        .await?;
    let pattern = format!("{}%", afd_events::ACTOR_PREFIX);
    let person = PersonSteer {
        machine_actor: afd_events::ACTOR_MACHINE,
        actor_pattern: &pattern,
    };
    // The facts are a steer's: only a steer reads them.
    let run = if producer == Producer::Steer {
        admissions
            .run_facts(&held.fleet, &admitted.stored.id, person)
            .await?
    } else {
        RunFacts::default()
    };
    let mut connection = held.fixtures.database.acquire().await?;
    let sequence = sqlx::query_scalar("SELECT sequence FROM fleet.admissions WHERE id = $1")
        .bind(&admitted.stored.id)
        .fetch_one(&mut *connection)
        .await?;
    Ok(Sent {
        id: admitted.stored.id,
        sequence,
        run,
    })
}

/// Stamps `admission` as claimed under `fence`, as a later lease's claim
/// would have.
pub(super) async fn stamp_claim(held: &Held, admission: &Uuid7, fence: i64) -> Checked {
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query("UPDATE fleet.admissions SET input_claim_fence = $2 WHERE id = $1")
        .bind(admission)
        .bind(fence)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Runs one statement that names one admission and changes it.
async fn change(held: &Held, statement: &'static str, admission: &Uuid7) -> Checked {
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query(statement)
        .bind(admission)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Parks `admission`, as an approval gate does.
pub(super) async fn park(held: &Held, admission: &Uuid7) -> Checked {
    change(
        held,
        "UPDATE fleet.admissions SET parked_at = now() WHERE id = $1",
        admission,
    )
    .await
}

/// Settles `admission` the way an earlier run's report would have.
pub(super) async fn settle_directly(held: &Held, admission: &Uuid7) -> Checked {
    change(
        held,
        "UPDATE fleet.admissions SET settled_at = now() WHERE id = $1",
        admission,
    )
    .await
}

/// Points the held lease at a durable task, or at none.
pub(super) async fn set_lease_task(held: &Held, task: Option<&Uuid7>) -> Checked {
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.leases SET task_id = $2 WHERE id = $1")
        .bind(&held.issued.lease_id)
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
    let claimed = response
        .inputs
        .into_iter()
        .map(|input| {
            Ok(Claimed {
                id: Uuid7::parse(&input.event_id)?,
                sequence: input.sequence,
                text: input.text.into_owned(),
            })
        })
        .collect::<af_core::error::Result<_>>()?;
    Ok(claimed)
}

/// [`claim`], as the runner holding the lease under its own fence.
pub(super) async fn claim_as_holder(held: &Held, after: i64) -> Checked<Vec<Claimed>> {
    claim(held, &held.runner, held.fence.as_u64(), after).await
}

/// Whether `admission` is settled, and the event it opened, if it has.
pub(super) async fn settled(held: &Held, admission: &Uuid7) -> Checked<Settlement> {
    let mut connection = held.fixtures.database.acquire().await?;
    let settled =
        sqlx::query_scalar("SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1")
            .bind(admission)
            .fetch_one(&mut *connection)
            .await?;
    let event = sqlx::query_as(
        "SELECT status, response_text, resumes_event_id FROM fleet.events \
         WHERE fleet_id = $1 AND event_id = $2",
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
        .load_for_report(&held.issued.lease_id, &held.runner)
        .await?
        .ok_or("the held lease loads for its report")?;
    let mut terminal = report(
        &held.issued.lease_id,
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

/// Makes the held run's own message a reply in `address`, a Slack thread.
pub(super) async fn address_head(held: &Held, address: &str) -> Checked {
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query(
        "UPDATE fleet.admissions SET reply_provider = $2, reply_address = $3 WHERE id = $1",
    )
    .bind(&held.event_id)
    .bind(afd_connector::Provider::Slack.id())
    .bind(address)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Installs Slack team `team` in the held fleet's workspace and links `person`
/// to a member of its tenant, as the OAuth callback would.
pub(super) async fn link_slack_person(
    held: &Held,
    workspace: &Uuid7,
    team: &str,
    person: &str,
) -> Checked {
    let user = afd_db::test_util::mint_id();
    let subject = format!("person-{user}");
    let mut connection = held.fixtures.database.acquire().await?;
    afd_db::test_util::UserSeed::new(
        &user,
        &held.tenant,
        &subject,
        &format!("{user}@example.test"),
    )
    .insert(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO connector.installs (id, provider, external_account_id, workspace_id, scopes, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, '{}', now(), now())",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(afd_connector::Provider::Slack.id())
    .bind(team)
    .bind(workspace)
    .execute(&mut *connection)
    .await?;
    let linked =
        afd_connector::grant::link_person(&mut connection, workspace, team, person, &subject)
            .await?;
    if linked == 1 {
        Ok(())
    } else {
        Err("the person's link was not written".into())
    }
}
