//! The row a producer's key already holds, read back.
//!
//! [`crate::Admissions::admit`] answers a repeat itself — its insert conflicts
//! on `UNIQUE (producer, producer_key)` and hands back the first row — but it
//! does not say whose payload that row holds, and a spent fleet budget or the
//! caller's own check refuses before the insert is reached. A producer that
//! must compare the payload, or answer a repeat a gate refused, reads it here.

use sqlx::Row as _;

use crate::error::{Result, query};
use crate::{Admissions, Producer, logical_id, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_FIND_REPEATED: &str = "find a repeated admission";

/// An admission already recorded under a producer's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeated {
    /// The logical event id the first admission was answered with.
    pub id: String,
    /// The payload digest it was admitted with, which a retry is checked
    /// against ([`crate::Admission::payload_digest`]).
    pub digest: String,
    /// The fleet the row was admitted for.
    pub fleet: String,
}

impl Admissions {
    /// The admission `producer` recorded under `key`, or `None` when the key
    /// is new.
    ///
    /// # Errors
    /// Reports a pool that would not give a connection, or a statement that
    /// would not run.
    pub async fn find_repeated(&self, producer: Producer, key: &str) -> Result<Option<Repeated>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_REPEATED)
            .bind(producer.as_str())
            .bind(key)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_FIND_REPEATED))?;
        row.map(|row| {
            let created_at: i64 = row.try_get(0).map_err(query(CONTEXT_FIND_REPEATED))?;
            let seq: i64 = row.try_get(1).map_err(query(CONTEXT_FIND_REPEATED))?;
            let digest: String = row.try_get(2).map_err(query(CONTEXT_FIND_REPEATED))?;
            let fleet: String = row.try_get(3).map_err(query(CONTEXT_FIND_REPEATED))?;
            Ok(Repeated {
                id: logical_id(created_at, seq),
                digest,
                fleet,
            })
        })
        .transpose()
    }
}
