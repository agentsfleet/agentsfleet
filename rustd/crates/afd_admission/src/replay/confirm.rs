//! Initial-run acceptance requires a committed receipt for the original admission.

use afd_core::clock;
use sqlx::{Connection as _, Row as _};

use crate::{Admissions, Producer, Result, error::query, replay::row::Pending, sql};

const CONTEXT: &str = "confirm one admitted event";

/// Durable admission identity and its current queue confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmation {
    /// The same logical identity returned by admission, including after replay.
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
        let row = sqlx::query(sql::CONFIRM_ADMISSION)
            .bind(producer.as_str())
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let pending = Pending::read(&row)?;
        let receipt: Option<String> = row.try_get(9).map_err(query(CONTEXT))?;
        let accepted =
            receipt.is_some() || self.append_pending(&pending, clock::now(), &mut tx).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        if accepted {
            self.mark_ready(&pending.fleet_id).await;
        }
        Ok(Some(Confirmation {
            event_id: pending.logical_id,
            accepted,
        }))
    }
}
