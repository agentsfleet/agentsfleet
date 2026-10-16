//! A client reload can resume a blocked first run without inventing a new event key.

use super::super::{ACTIVE, CONTEXT, Creations, read};
use crate::{Access, Result, access, error::raise::query};
use afd_core::id::Uuid7;
use sqlx::Connection as _;

impl Creations {
    pub(in crate::creation) async fn recover_initial(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
    ) -> Result<()> {
        let (snapshot, _) = self.snapshot(access, fleet).await?;
        if snapshot.status != ACTIVE || !snapshot.initial_start.unadmitted() {
            return Ok(());
        }
        let key = super::key(fleet);
        let payload = super::payload(&snapshot.initial_start.choice)?;
        let prepared = self
            .prepare_initial(&snapshot, &key, payload.as_deref())
            .await?;
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::current(&mut tx, access).await?;
        let current = read::lock(&mut tx, access.workspace, fleet).await?;
        if current.status != ACTIVE || !current.initial_start.unadmitted() {
            return Ok(());
        }
        let initial = self
            .stage_initial(&mut tx, current.initial_start, prepared)
            .await?;
        super::store(&mut tx, access, fleet, &initial).await?;
        tx.commit().await.map_err(query(CONTEXT))
    }
}
