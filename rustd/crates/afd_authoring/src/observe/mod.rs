//! Private stream hints are read under the same creator and membership locks.

mod rows;

use crate::{
    Access, Drafts, Result, access, draft::read, error::raise::query,
    lease::guard::DELETION_GENERATION, types::DraftState,
};
use afd_core::{error_code, id::Uuid7};
use sqlx::Connection as _;

pub use rows::Notice;

const CONTEXT: &str = "observe private draft changes";

/// A reauthorized observation for one private stream poll.
#[derive(Debug)]
pub enum Observation {
    /// Current metadata; transcript text is read through the messages resource.
    Current(View),
    /// An already-authorized stream reached deletion or logical expiry.
    Deleted,
}

/// Bounded current state, never a claim of durable stream history.
#[derive(Debug)]
pub struct View {
    /// Private generation fixed when the stream opens.
    pub generation: i64,
    /// Latest notices; clients reconcile through ordinary reads on reconnect.
    pub notices: Vec<Notice>,
    /// The draft closed and this stream can end after its last notices.
    pub closed: bool,
}

impl Drafts {
    /// Rechecks membership, creator and generation before reading private hints.
    ///
    /// # Errors
    /// Foreign or revoked access is hidden, including on an existing stream.
    pub async fn observe(
        &self,
        access: Access<'_>,
        id: &Uuid7,
        generation: Option<i64>,
    ) -> Result<Observation> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let locked = match read::lock(&mut tx, access, &person, id).await {
            Ok(locked) => locked,
            Err(error)
                if generation.is_some() && error.code() == error_code::AUTHORING_NOT_FOUND =>
            {
                return Ok(Observation::Deleted);
            }
            Err(error) => return Err(error),
        };
        if generation.is_some_and(|generation| generation != DELETION_GENERATION) {
            return Ok(Observation::Deleted);
        }
        let mut notices = rows::messages(&mut tx, id, locked.cancelled_through_sequence).await?;
        if let Some(candidate) = &locked.head_candidate_id {
            notices.push(Notice::candidate(id, candidate, locked.candidate_revision));
        }
        let closed = locked.status == DraftState::Closed;
        if closed {
            notices.push(Notice::closed(id));
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Observation::Current(View {
            generation: DELETION_GENERATION,
            notices,
            closed,
        }))
    }
}
