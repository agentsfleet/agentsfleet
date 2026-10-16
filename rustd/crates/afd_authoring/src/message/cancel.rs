//! Stop advances a cutoff; messages accepted after that sequence stay queued.

use crate::{
    Access, Drafts, Result, access,
    draft::read,
    error::raise::{invalid, query},
    sql,
    types::{CANCELLED, QUEUED, RUNNING},
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_observability::AuthoringStep;
use serde::Serialize;
use sqlx::Connection as _;

const CONTEXT: &str = "cancel draft messages through cutoff";
const CUTOFF_INVALID: &str = "through_sequence must name an already accepted message or zero";

/// Messages whose transition this Stop committed.
#[derive(Debug, Serialize)]
pub struct Stopped {
    /// Decimal user request sequence.
    pub sequence: String,
    /// The terminal cancellation verdict.
    pub state: &'static str,
}

/// Standing watermark plus the newly stopped requests.
#[derive(Debug, Serialize)]
pub struct Cancellation {
    /// Never decreases when an older Stop is retried.
    pub cancelled_through: String,
    /// Requests transitioned by this operation only.
    pub stopped: Vec<Stopped>,
}

impl Drafts {
    /// Fences private publication through a monotone accepted-message cutoff.
    ///
    /// # Errors
    /// Refuses future/negative cutoffs and inaccessible drafts.
    pub async fn cancel(
        &self,
        access: Access<'_>,
        id: &Uuid7,
        through: i64,
        now: UnixMillis,
    ) -> Result<Cancellation> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let draft = read::lock(&mut tx, access, &person, id).await?;
        if through < 0 || through > draft.last_sequence {
            return Err(invalid(CUTOFF_INVALID));
        }
        let cutoff = through.max(draft.cancelled_through);
        let stopped: Vec<i64> = sqlx::query_scalar(sql::message::CANCEL)
            .bind(id.as_str())
            .bind(cutoff)
            .bind(CANCELLED)
            .bind(now.as_millis())
            .bind(QUEUED)
            .bind(RUNNING)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        if cutoff > draft.cancelled_through {
            sqlx::query(sql::draft::CANCEL)
                .bind(id.as_str())
                .bind(cutoff)
                .bind(now.as_millis())
                .execute(&mut *tx)
                .await
                .map_err(query(CONTEXT))?;
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        // One step per row this Stop moved: a retried Stop matches none.
        for _ in &stopped {
            let draft_id = id.clone();
            let step = AuthoringStep::MessageFinished {
                draft_id,
                state: CANCELLED,
            };
            self.reached(access, step);
        }
        Ok(Cancellation {
            cancelled_through: cutoff.to_string(),
            stopped: stopped
                .into_iter()
                .map(|sequence| Stopped {
                    sequence: sequence.to_string(),
                    state: CANCELLED,
                })
                .collect(),
        })
    }
}
