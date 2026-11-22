//! Source identity is proved from native turns, never model prose.

use af_core::id::Uuid7;
use afd_turns::Producer;
use sqlx::PgConnection;

use super::super::{DurableTaskRefusal, DurableTaskRun, PreparedPlan, guard::RunRow, sql};
use crate::error::{Result, query};

#[derive(sqlx::FromRow)]
pub(in crate::durable_task) struct Source {
    pub(super) actor: String,
    pub(super) producer: String,
    slack_person_allowed: bool,
    pub(in crate::durable_task) request_json: String,
    pub(super) reply_provider: Option<String>,
    pub(super) reply_address: Option<String>,
    input_run_id: Option<Uuid7>,
    input_fence: Option<i64>,
}

pub(in crate::durable_task) async fn lock(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    current: &RunRow,
    plan: &PreparedPlan,
) -> Result<Source> {
    let source = load(connection, current, &plan.source_turn_id).await?;
    check_assignment(&source, run, current, &plan.source_turn_id)?;
    Ok(source)
}

pub(in crate::durable_task) async fn load(
    connection: &mut PgConnection,
    current: &RunRow,
    turn: &Uuid7,
) -> Result<Source> {
    let source: Source = sqlx::query_as(sql::LOCK_SOURCE)
        .bind(turn)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(connection)
        .await
        .map_err(query("lock durable task source"))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    Ok(source)
}

pub(in crate::durable_task) fn check_assignment(
    source: &Source,
    run: DurableTaskRun<'_>,
    current: &RunRow,
    turn: &Uuid7,
) -> Result<()> {
    let owns_assignment = source.input_run_id.as_ref() == Some(run.run_id)
        && source.input_fence == Some(current.fence);
    if turn.as_str() != current.source_turn_id && !owns_assignment {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    Ok(())
}

pub(in crate::durable_task) fn check_human(source: &Source) -> Result<()> {
    if source.producer == Producer::SlackMention.as_str() {
        return if source.slack_person_allowed {
            Ok(())
        } else {
            Err(DurableTaskRefusal::InvalidTarget.into())
        };
    }
    if source.producer != Producer::Steer.as_str()
        || !afd_turns::view::is_steer_actor(&source.actor)
        || source.actor == afd_turns::view::ACTOR_MACHINE
    {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    Ok(())
}

pub(super) fn check(source: &Source, current: &RunRow, plan: &PreparedPlan) -> Result<()> {
    check_human(source)?;
    if current.revision_id.as_ref() != Some(&plan.accepted_revision_id) {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let message = source.message()?;
    for item in &plan.items {
        let start = usize::try_from(item.span().start).map_err(|source| {
            crate::error::Error::from(crate::error::ErrorKind::DurableTaskCounter {
                column: "source_start",
                source,
            })
        })?;
        let end = usize::try_from(item.span().end).map_err(|source| {
            crate::error::Error::from(crate::error::ErrorKind::DurableTaskCounter {
                column: "source_end",
                source,
            })
        })?;
        if message.get(start..end).is_none() {
            return Err(DurableTaskRefusal::ReplayConflict.into());
        }
    }
    Ok(())
}

impl Source {
    pub(in crate::durable_task) fn message(&self) -> Result<std::borrow::Cow<'_, str>> {
        message_for(&self.producer, &self.request_json)
    }
}

#[derive(serde::Deserialize)]
struct SlackPayload<'a> {
    #[serde(borrow)]
    human_message: std::borrow::Cow<'a, str>,
}

pub(in crate::durable_task) fn message_for<'a>(
    producer: &str,
    body: &'a str,
) -> Result<std::borrow::Cow<'a, str>> {
    if producer != Producer::SlackMention.as_str() {
        return Ok(af_wire::turn::message_of(body));
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
    use afd_turns::Producer;

    use super::{Source, check_human};

    fn steer_from(actor: &str) -> Source {
        Source {
            actor: actor.to_owned(),
            producer: Producer::Steer.as_str().to_owned(),
            slack_person_allowed: false,
            request_json: r#"{"message":"fix it"}"#.to_owned(),
            reply_provider: None,
            reply_address: None,
            input_run_id: None,
            input_fence: None,
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
            af_core::error_code::DURABLE_TASK_INVALID_TARGET,
            "the refusal is the invalid-target one"
        );
        check_human(&steer_from("steer:user_2abc")).expect("a person's steer may start a task");
    }

    fn mention(allowed: bool, request_json: &str) -> Source {
        Source {
            producer: Producer::SlackMention.as_str().to_owned(),
            slack_person_allowed: allowed,
            request_json: request_json.to_owned(),
            ..steer_from("slack:T0001:U0001")
        }
    }

    // Value: protects=a Slack mention starts a durable task only when the
    // workspace allows that person; fails_when=the mention arm accepts every
    // mention or refuses every one; why_new=the Slack arm of the human check had
    // no case; seam=the source check every task acceptance runs.
    #[test]
    fn a_slack_mention_counts_as_human_only_for_an_allowed_person() {
        let body = r#"{"human_message":"rerun the flaky check"}"#;
        check_human(&mention(true, body)).expect("an allowed person's mention is human");
        let refused = check_human(&mention(false, body))
            .expect_err("a mention from a person the workspace has not allowed is refused");
        assert_eq!(
            refused.code(),
            af_core::error_code::DURABLE_TASK_INVALID_TARGET
        );
    }

    // Value: protects=a Slack mention's task text is the person's own message,
    // and a body without one is a fault, not an empty goal; fails_when=the
    // mention body is read as a steer's `message` or a malformed body reads as
    // empty text; why_new=the Slack arm of the message reader had no case;
    // seam=the reader acceptance and the input page both call.
    #[test]
    fn a_slack_mention_reads_its_human_message_and_refuses_a_body_without_one() {
        let source = mention(true, r#"{"human_message":"Préserve the café title 🦉"}"#);
        assert_eq!(
            source.message().expect("a mention carries its message"),
            "Préserve the café title 🦉"
        );
        for body in [r#"{"message":"a steer's field"}"#, "not json", ""] {
            let unreadable = mention(true, body)
                .message()
                .expect_err("a mention body without its human message is refused");
            assert_eq!(
                unreadable.code(),
                af_core::error_code::INTERNAL_DB_QUERY,
                "{body:?}"
            );
        }
        assert_eq!(
            steer_from("steer:user_2abc")
                .message()
                .expect("a steer reads its message"),
            "fix it"
        );
    }
}
