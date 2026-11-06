//! The row a producer's key already holds, read back.
//!
//! [`crate::Admissions::admit`] answers a repeat itself — its one statement
//! reads the key before any gate and hands back the first row, digest and
//! fleet included, as [`crate::Admitted::stored`]. A same-key send that
//! committed while this one waited on the fleet lock is invisible to that read,
//! and a spent deployment ceiling then refuses before the insert can meet it,
//! so a producer that must answer a repeat on that path reads the row here.

use jiff::Timestamp;

use crate::error::{Result, query};
use crate::{Admissions, Producer, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_FIND_REPEATED: &str = "find a repeated admission";

/// An admission row as recorded under a producer's key: an earlier call's
/// when read back here, or the one an admission just wrote or met.
///
/// Decoded straight from [`sql::SELECT_REPEATED`], and assembled from the
/// answer row of [`sql::ADMIT_ONE`], whose columns are named after these fields.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Repeated {
    /// The event id the row's admission was answered with: the row's own
    /// `UUIDv7`, as text.
    pub id: String,
    /// The payload digest it was admitted with, which a retry is checked
    /// against ([`crate::Admission::payload_digest`]).
    pub digest: String,
    /// The fleet the row was admitted for.
    pub fleet: String,
    /// When the database admitted the row — the event's own `created_at`.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: Timestamp,
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
        sqlx::query_as::<_, Repeated>(sql::SELECT_REPEATED)
            .bind(producer.as_str())
            .bind(key)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_FIND_REPEATED))
    }
}
