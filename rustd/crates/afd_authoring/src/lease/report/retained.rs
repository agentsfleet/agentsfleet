//! An unsettled acknowledgement settles once when its late usage becomes known.
//!
//! A report acknowledged while a provider request was pending saved
//! `settled:false`, and the supervisor resends that same report until it reads
//! `settled:true`. The late provider result records known usage on the
//! dispatch receipt, so the replay that first sees it logs the attempt's usage
//! lines and settles. The attempt row lock taken by `guard::accounting`
//! serializes concurrent replays, and the outcome the settlement stamps on the
//! attempt is what every later replay reads.

use afd_wire::authoring::{Knowledge, Outcome, Reported};
use sqlx::PgConnection;

use super::{Basis, CONTEXT, Status, guard::Attempt, prepare::Prepared, settle, sql};
use crate::{Drafts, Result, draft, error::raise::query, funnel::Owed};

pub(super) async fn resolve(
    drafts: &Drafts,
    connection: &mut PgConnection,
    attempt: &Attempt,
    prepared: &Prepared,
    saved: Reported,
    owed: &mut Owed,
) -> Result<(Reported, Option<settle::Owing>)> {
    if attempt.status == Status::Settled {
        return Ok((settled(connection, attempt, saved).await?, None));
    }
    // Accounting only: a retained replay never reopens private rows.
    let basis = Basis::measure(connection, attempt, prepared, None).await?;
    if basis.verified.usage.knowledge != Knowledge::Known {
        return Ok((saved, None));
    }
    basis
        .conclude(drafts, connection, settle::Acknowledgement::Retained, owed)
        .await
}

async fn settled(
    connection: &mut PgConnection,
    attempt: &Attempt,
    saved: Reported,
) -> Result<Reported> {
    let raw: String = sqlx::query_scalar(sql::SETTLED_OUTCOME)
        .bind(attempt.id.as_str())
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    let state: Outcome = draft::word(&raw)?;
    Ok(Reported {
        settled: true,
        state,
        ..saved
    })
}
