//! Initial-run acceptance requires a committed receipt for the original admission.

use sqlx::Connection as _;

use crate::{Admissions, Producer, Result, error::query, replay::row::Confirmable, sql};

const CONTEXT: &str = "confirm one admitted event";

/// Durable admission identity and its current queue confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    /// The same event id admission answered with, including after replay.
    pub event_id: String,
    /// True only after append and receipt persistence have both committed.
    pub accepted: bool,
}

impl Admissions {
    /// Repairs at most one saved admission and confirms its durable queue receipt.
    ///
    /// The saved payload is authoritative. A concurrent repair may hold the row;
    /// absence in that case means the caller must keep its initial run pending.
    ///
    /// # Errors
    /// Reports an unavailable ledger. An append refusal keeps the admission pending.
    pub async fn confirm(&self, producer: Producer, key: &str) -> Result<Option<Confirmation>> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let row = sqlx::query_as::<_, Confirmable>(sql::CONFIRM_ADMISSION)
            .bind(producer.as_str())
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let Some(Confirmable { pending, receipt }) = row else {
            return Ok(None);
        };
        let accepted = receipt.is_some() || self.append_pending(&pending, &mut tx).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        if accepted {
            self.mark_ready(&pending.fleet_id).await;
        }
        Ok(Some(Confirmation {
            event_id: pending.id,
            accepted,
        }))
    }
}
