//! Initial-run acceptance requires a committed receipt for the original admission.

use crate::{Admissions, Producer, Result, error::query, replay::row::Confirmable, sql};

use super::{CLAIM_SPAN, pass};

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
    /// The saved payload is authoritative. The row is claimed the way a replay
    /// pass claims it, so the append runs with no transaction open; a row a
    /// replay pass or a sibling confirmation holds answers `None`, and the
    /// caller must keep its initial run pending and ask again.
    ///
    /// # Errors
    /// Reports an unavailable ledger. An append refusal keeps the admission pending.
    pub async fn confirm(&self, producer: Producer, key: &str) -> Result<Option<Confirmation>> {
        let row: Option<Confirmable> = sqlx::query_as(sql::CLAIM_ADMISSION)
            .bind(producer.as_str())
            .bind(key)
            .bind(CLAIM_SPAN)
            .fetch_optional(&mut *self.database.acquire().await?)
            .await
            .map_err(query(CONTEXT))?;
        let Some(Confirmable {
            pending,
            receipt,
            claimed,
        }) = row
        else {
            return Ok(None);
        };
        let accepted = match (receipt, claimed) {
            (Some(_), _) => true,
            (None, false) => return Ok(None),
            (None, true) => {
                let drained = pass::drain(std::slice::from_ref(&pending), |pending| {
                    self.append_pending(pending)
                })
                .await;
                self.record_replays(&drained).await?;
                !drained.appended.is_empty()
            }
        };
        if accepted {
            self.mark_ready(&pending.fleet_id).await;
        }
        Ok(Some(Confirmation {
            event_id: pending.id,
            accepted,
        }))
    }
}
