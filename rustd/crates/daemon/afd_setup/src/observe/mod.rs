//! Private stream hints are read under the same creator and membership locks.

mod rows;

use crate::{Access, Drafts, Result, access, draft::read, error::raise::query, types::DraftState};
use af_core::{error_code, id::Uuid7};
use sqlx::Connection as _;

pub use rows::Notice;

const CONTEXT: &str = "observe private draft changes";

/// Every draft's private deletion generation, as the draft stream reports it.
///
/// Nothing ever advanced it, so the schema keeps no column for it and the
/// runner protocol carries none; only the observation stream still names it.
const DELETION_GENERATION: i64 = 0;

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
    /// Running state is authoritative even outside the recent notice window.
    pub running_sequence: Option<i64>,
}

impl Drafts {
    /// Authorizes an initial private read; missing drafts remain ordinary errors.
    ///
    /// # Errors
    /// Foreign, deleted, expired or revoked resources are hidden.
    pub async fn initial_view(&self, access: Access<'_>, id: &Uuid7) -> Result<View> {
        self.current_view(access, id)
            .await?
            .ok_or_else(|| crate::error::ErrorKind::Hidden.into())
    }

    /// Rechecks membership, creator and generation before reading private hints.
    ///
    /// # Errors
    /// Foreign or revoked access is hidden, including on an existing stream.
    pub async fn observe(
        &self,
        access: Access<'_>,
        id: &Uuid7,
        generation: i64,
    ) -> Result<Observation> {
        Ok(match self.current_view(access, id).await? {
            Some(view) if view.generation == generation => Observation::Current(view),
            Some(_) | None => Observation::Deleted,
        })
    }

    async fn current_view(&self, access: Access<'_>, id: &Uuid7) -> Result<Option<View>> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let locked = match read::lock(&mut tx, access, &person, id).await {
            Ok(locked) => locked,
            Err(error) if error.code() == error_code::AUTHORING_NOT_FOUND => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let mut notices = rows::messages(&mut tx, id).await?;
        let running_sequence = rows::running_sequence(&mut tx, id).await?;
        if let Some(candidate) = &locked.head_candidate_id {
            notices.push(Notice::candidate(id, candidate, locked.candidate_revision));
        }
        let closed = locked.status == DraftState::Closed;
        if closed {
            notices.push(Notice::closed(id));
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Some(View {
            generation: DELETION_GENERATION,
            notices,
            closed,
            running_sequence,
        }))
    }
}
