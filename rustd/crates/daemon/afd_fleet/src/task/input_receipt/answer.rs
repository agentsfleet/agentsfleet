//! Input answers reuse event completion and the existing outbound delivery owner.

use crate::{
    Result,
    error::query,
    run::{
        Runs,
        delivery::ReplyDestination,
        verdict::{Terminal, Verdict},
    },
    task::{TaskRefusal, Tasks, guard::RunRow},
};
use af_core::{clock, id::Uuid7};
use af_wire::task::TaskInputReceiptInput;
use afd_outbound::delivery::Delivery;
use sqlx::PgConnection;

pub(super) struct Completion {
    closed: Option<afd_turns::view::Closed>,
    owed: Option<Uuid7>,
    answer: String,
}

pub(super) async fn finish(
    tasks: &Tasks,
    connection: &mut PgConnection,
    current: &RunRow,
    input: &TaskInputReceiptInput,
) -> Result<Option<Completion>> {
    let rows: Vec<Option<String>> = sqlx::query_scalar(include_str!("sql/answers.sql"))
        .bind(crate::task::guard::wire(&input.source_turn_id)?)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("read every source-item reply delivery"))?;
    if rows.is_empty() || rows.iter().any(Option::is_none) {
        return Ok(None);
    }
    let answer = rows.into_iter().flatten().collect::<Vec<_>>().join("\n\n");
    let closed = complete_history(tasks, connection, current, input, &answer).await?;
    let reply =
        Runs::reply_destination(connection, &current.fleet_id, &input.source_turn_id).await?;
    let owed = owe_reply(tasks, connection, current, input, &answer, reply.as_ref()).await?;
    sqlx::query(include_str!("sql/settle_source.sql"))
        .bind(crate::task::guard::wire(&input.source_turn_id)?)
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
        answer,
    }))
}

async fn complete_history(
    tasks: &Tasks,
    connection: &mut PgConnection,
    current: &RunRow,
    input: &TaskInputReceiptInput,
    answer: &str,
) -> Result<Option<afd_turns::view::Closed>> {
    let source: Uuid7 = sqlx::query_scalar(include_str!("sql/source.sql"))
        .bind(crate::task::guard::wire(&input.source_turn_id)?)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock original accepted reply source"))?
        .ok_or(TaskRefusal::NotFound)?;
    // Same event owner and received/terminal writers as an ordinary run report: the
    // turn keeps the instant intake gave it, so a thread page lists it in place.
    sqlx::query(afd_turns::view::sql::OPEN_FOLDED_EVENTS)
        .bind(&current.fleet_id)
        .bind(std::slice::from_ref(&source))
        .bind(&current.source_turn_id)
        .bind(af_core::turn::status::RECEIVED)
        .execute(&mut *connection)
        .await
        .map_err(query("open accepted input history before completing it"))?;
    tasks
        .runs
        .mark_terminal(
            connection,
            &current.fleet_id,
            &crate::task::guard::wire(&input.source_turn_id)?,
            Terminal {
                verdict: Verdict::Succeeded,
                response_text: answer,
                tokens: 0,
                wall_ms: 0,
                tool_calls: None,
            },
        )
        .await
}

async fn owe_reply(
    tasks: &Tasks,
    connection: &mut PgConnection,
    current: &RunRow,
    input: &TaskInputReceiptInput,
    answer: &str,
    reply: Option<&ReplyDestination>,
) -> Result<Option<Uuid7>> {
    let owed = if let Some(reply) = reply {
        tasks
            .runs
            .owe_delivery(
                connection,
                Delivery {
                    fleet_id: &current.fleet_id,
                    workspace_id: &current.workspace_id,
                    provider: reply.provider,
                    destination: &reply.address,
                    turn_id: &crate::task::guard::wire(&input.source_turn_id)?,
                    answer,
                },
                clock::now(),
            )
            .await?
    } else {
        None
    };
    Ok(owed)
}

impl Completion {
    pub(super) async fn announce(self, tasks: &Tasks) {
        if let Some(closed) = self.closed {
            let publication = tasks
                .runs
                .publish_completion(&closed, Some(&self.answer))
                .await;
            afd_bus::observe_notification(
                &afd_bus::Subject::ThreadItems(closed.row.fleet_id.clone()),
                publication,
            );
        }
        if let Some(owed) = self.owed {
            tasks.runs.wake_delivery(&owed).await;
        }
    }
}
