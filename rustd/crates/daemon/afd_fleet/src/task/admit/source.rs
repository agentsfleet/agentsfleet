//! Source identity is proved from native admissions, never model prose.

use af_core::id::Uuid7;
use afd_admission::Producer;
use sqlx::PgConnection;

use super::super::{PreparedPlan, TaskLease, TaskRefusal, guard::LeaseRow, sql};
use crate::error::{Result, query};

#[derive(sqlx::FromRow)]
pub(in crate::task) struct Source {
    pub(super) actor: String,
    pub(super) producer: String,
    slack_person_allowed: bool,
    pub(in crate::task) request_json: String,
    pub(super) reply_provider: Option<String>,
    pub(super) reply_address: Option<String>,
    input_claim_lease_id: Option<Uuid7>,
    input_claim_fence: Option<i64>,
}

pub(in crate::task) async fn lock(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    current: &LeaseRow,
    plan: &PreparedPlan,
) -> Result<Source> {
    let source = load(connection, current, &plan.source_admission_id).await?;
    check_claim(&source, lease, current, &plan.source_admission_id)?;
    Ok(source)
}

pub(in crate::task) async fn load(
    connection: &mut PgConnection,
    current: &LeaseRow,
    admission: &af_core::id::Uuid7,
) -> Result<Source> {
    let source: Source = sqlx::query_as(sql::LOCK_SOURCE)
        .bind(admission)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(connection)
        .await
        .map_err(query("lock durable task source"))?
        .ok_or(TaskRefusal::NotFound)?;
    Ok(source)
}

pub(in crate::task) fn check_claim(
    source: &Source,
    lease: TaskLease<'_>,
    current: &LeaseRow,
    admission: &af_core::id::Uuid7,
) -> Result<()> {
    let owns_claim = source.input_claim_lease_id.as_ref() == Some(lease.lease_id)
        && source.input_claim_fence == Some(current.fence);
    if admission.as_str() != current.source_admission_id && !owns_claim {
        return Err(TaskRefusal::StaleState.into());
    }
    Ok(())
}

pub(in crate::task) fn check_human(source: &Source) -> Result<()> {
    if source.producer == Producer::SlackMention.as_str() {
        return if source.slack_person_allowed {
            Ok(())
        } else {
            Err(TaskRefusal::InvalidTarget.into())
        };
    }
    if source.producer != Producer::Steer.as_str()
        || !afd_events::is_steer_actor(&source.actor)
        || source.actor == afd_events::ACTOR_MACHINE
    {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    Ok(())
}

pub(super) fn check(source: &Source, current: &LeaseRow, plan: &PreparedPlan) -> Result<()> {
    check_human(source)?;
    if current.revision_id.as_ref() != Some(&plan.accepted_revision_id) {
        return Err(TaskRefusal::StaleState.into());
    }
    let message = source.message()?;
    for item in &plan.items {
        let start = usize::try_from(item.span().start).map_err(|source| {
            crate::error::Error::from(crate::error::ErrorKind::TaskCounter {
                column: "source_start",
                source,
            })
        })?;
        let end = usize::try_from(item.span().end).map_err(|source| {
            crate::error::Error::from(crate::error::ErrorKind::TaskCounter {
                column: "source_end",
                source,
            })
        })?;
        if message.get(start..end).is_none() {
            return Err(TaskRefusal::ReplayConflict.into());
        }
    }
    Ok(())
}

impl Source {
    pub(in crate::task) fn message(&self) -> Result<std::borrow::Cow<'_, str>> {
        message_for(&self.producer, &self.request_json)
    }
}

#[derive(serde::Deserialize)]
struct SlackPayload<'a> {
    #[serde(borrow)]
    human_message: std::borrow::Cow<'a, str>,
}

pub(in crate::task) fn message_for<'a>(
    producer: &str,
    body: &'a str,
) -> Result<std::borrow::Cow<'a, str>> {
    if producer != Producer::SlackMention.as_str() {
        return Ok(af_wire::event::message_of(body));
    }
    let human: SlackPayload<'_> =
        serde_json::from_str(body).map_err(af_core::json::Unreadable::from)?;
    Ok(human.human_message)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_admission::Producer;

    use super::{Source, check_human};

    fn steer_from(actor: &str) -> Source {
        Source {
            actor: actor.to_owned(),
            producer: Producer::Steer.as_str().to_owned(),
            slack_person_allowed: false,
            request_json: r#"{"message":"fix it"}"#.to_owned(),
            reply_provider: None,
            reply_address: None,
            input_claim_lease_id: None,
            input_claim_fence: None,
        }
    }

    /// A fleet's steer carries the steer producer, and still
    /// cannot start a durable task, which only a person's message may.
    #[test]
    fn check_human_refuses_a_fleet_actor() {
        let fleet = steer_from("fleet:0190d5c2-7e1a-7000-8000-00000000f001");
        let refused = check_human(&fleet).expect_err("a fleet's steer is not a person's");
        assert_eq!(
            refused.code(),
            af_core::error_code::TASK_INVALID_TARGET,
            "the refusal is the invalid-target one"
        );
        check_human(&steer_from("steer:user_2abc")).expect("a person's steer may start a task");
    }
}
