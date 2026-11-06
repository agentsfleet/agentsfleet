//! Input answers reuse event completion and the existing outbound obligation owner.

use crate::{
    Result,
    error::query,
    lease::{
        Leases,
        obligation::ReplyDestination,
        verdict::{Terminal, Verdict},
    },
    task::{
        TaskRefusal, Tasks,
        guard::{self, LeaseRow},
    },
};
use afd_core::{clock, id::Uuid7};
use afd_outbound::obligation::Delivery;
use afd_wire::task::TaskInputReceiptInput;
use sqlx::PgConnection;

#[derive(sqlx::FromRow)]
struct Source {
    actor: String,
    event_type: String,
    request_json: String,
}

pub(super) struct Completion {
    closed: Option<afd_events::Closed>,
    owed: Option<Uuid7>,
    reply: Option<ReplyDestination>,
    fleet: String,
    workspace: String,
    source: String,
    answer: String,
}

pub(super) async fn finish(
    tasks: &Tasks,
    connection: &mut PgConnection,
    current: &LeaseRow,
    input: &TaskInputReceiptInput,
) -> Result<Option<Completion>> {
    let rows: Vec<Option<String>> = sqlx::query_scalar(include_str!("sql/answers.sql"))
        .bind(&input.source_admission_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("read every source-item reply obligation"))?;
    if rows.is_empty() || rows.iter().any(Option::is_none) {
        return Ok(None);
    }
    let answer = rows.into_iter().flatten().collect::<Vec<_>>().join("\n\n");
    let source: Source = sqlx::query_as(include_str!("sql/source.sql"))
        .bind(&input.source_admission_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock original accepted reply source"))?
        .ok_or(TaskRefusal::NotFound)?;
    // Same event owner and received/terminal writers as an ordinary lease report.
    let _received: (bool,) = sqlx::query_as(afd_events::sql::INSERT_LEASED_FLEET_EVENT)
        .bind(&current.fleet_id)
        .bind(&input.source_admission_id)
        .bind(&current.workspace_id)
        .bind(source.actor)
        .bind(source.event_type)
        .bind(source.request_json)
        .bind(Some(&current.source_admission_id))
        .bind(afd_core::event::status::RECEIVED)
        .bind(&input.source_admission_id)
        .bind(afd_admission::Producer::Steer.as_str())
        .fetch_one(&mut *connection)
        .await
        .map_err(query("open accepted input history before completing it"))?;
    let closed = tasks
        .leases
        .mark_terminal(
            connection,
            &guard::id(&current.fleet_id, "fleet_id")?,
            &input.source_admission_id,
            Terminal {
                verdict: Verdict::Succeeded,
                response_text: &answer,
                tokens: 0,
                wall_ms: 0,
                tool_calls: None,
            },
        )
        .await?;
    let reply =
        Leases::reply_destination(connection, &current.fleet_id, &input.source_admission_id)
            .await?;
    let owed = if let Some(reply) = &reply {
        tasks
            .leases
            .owe_delivery(
                connection,
                Delivery {
                    fleet_id: &current.fleet_id,
                    workspace_id: &current.workspace_id,
                    provider: reply.provider,
                    destination: &reply.address,
                    event_id: &input.source_admission_id,
                    answer: &answer,
                },
                clock::now(),
            )
            .await?
    } else {
        None
    };
    sqlx::query(include_str!("sql/settle_source.sql"))
        .bind(&input.source_admission_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .execute(connection)
        .await
        .map_err(query(
            "settle an input only after every accepted item has its own reply",
        ))?;
    Ok(Some(Completion {
        closed,
        owed,
        reply,
        fleet: current.fleet_id.clone(),
        workspace: current.workspace_id.clone(),
        source: input.source_admission_id.clone(),
        answer,
    }))
}

impl Completion {
    pub(super) async fn announce(self, tasks: &Tasks) {
        if let Some(closed) = self.closed {
            tasks
                .leases
                .publish_completion(&closed, Some(&self.answer))
                .await;
        }
        if let (Some(owed), Some(reply)) = (self.owed, self.reply)
            && let Err(error) = tasks
                .leases
                .queue_delivery(
                    &owed,
                    Delivery {
                        fleet_id: &self.fleet,
                        workspace_id: &self.workspace,
                        provider: reply.provider,
                        destination: &reply.address,
                        event_id: &self.source,
                        answer: &self.answer,
                    },
                )
                .await
        {
            tracing::warn!(event = "dt_input_reply_deferred", error_code = %error.code());
        }
    }
}
