//! Stop on one turn of a creator-private draft: the setup thread's door onto
//! the one Stop every thread shares (`afd_turns::view::stop`).

use crate::{Access, Drafts, Result, access, draft::read, error::raise::query, types::CANCELLED};
use af_core::id::Uuid7;
use af_observability::SetupStep;
use afd_turns::view::stop::{Stop, Stopped};
use sqlx::Connection as _;

const CONTEXT: &str = "stop a draft turn";

impl Drafts {
    /// Stops `turn` of the creator's draft `id`: the turn is cancelled, and
    /// the setup run serving it is marked and told to end. `None` when the
    /// draft holds no such turn.
    ///
    /// # Errors
    /// Refuses a draft that is not the caller's, as every draft read does, and
    /// reports a datastore that would not answer.
    pub async fn stop_turn(
        &self,
        access: Access<'_>,
        id: &Uuid7,
        turn: &Uuid7,
    ) -> Result<Option<Stopped>> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        // The draft first, as every draft write locks it, then the thread.
        read::lock(&mut tx, access, &person, id).await?;
        let stopped = afd_turns::view::stop::stop_on(&mut tx, access.workspace, id, turn)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        if let Some(Stopped {
            stop: Stop::Cancelled { runs, .. },
            ..
        }) = &stopped
        {
            if let Some(bus) = &self.bus {
                afd_turns::view::stop::announce(bus, runs.iter().cloned()).await;
            }
            let draft_id = id.clone();
            self.reached(
                access,
                SetupStep::MessageFinished {
                    draft_id,
                    state: CANCELLED,
                },
            );
        }
        Ok(stopped)
    }
}
