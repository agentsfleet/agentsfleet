//! Replay returns its accepted setup revision even after a later improvement.

use super::super::{CONTEXT, Creations, etag, read};
use crate::{
    Access, Result, access,
    error::{ErrorKind, raise::query},
    types::AppliedRevision,
};
use afd_core::id::Uuid7;
use sqlx::Connection as _;

const ACCEPTED: &str =
    "SELECT revision FROM core.fleet_definition_revisions WHERE id=$1::uuid AND fleet_id=$2::uuid";

impl Creations {
    pub(super) async fn applied(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        revision: &Uuid7,
        digest: &str,
    ) -> Result<AppliedRevision> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::current(&mut tx, access).await?;
        let current = read::lock(&mut tx, access.workspace, fleet).await?;
        let number: Option<i64> = sqlx::query_scalar(ACCEPTED)
            .bind(revision.as_str())
            .bind(fleet.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let number = number.ok_or(ErrorKind::Gone)?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(AppliedRevision {
            fleet_id: fleet.clone(),
            setup_revision: number.to_string(),
            setup_etag: etag(fleet, number),
            etag: etag(fleet, current.resource_version),
            review_digest: digest.to_owned(),
        })
    }
}
