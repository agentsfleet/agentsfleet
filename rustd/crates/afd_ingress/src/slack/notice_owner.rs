//! Stable channel ownership for deterministic notices; never a model execution target.
use super::ChannelId;
use crate::error::{self, COLUMN_FLEET, Result, row_unreadable, stored_fleet, stored_status};
use crate::{Ingress, sql};
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use jiff::Timestamp;

/// New channel bindings carry only deterministic notices.
pub const KIND_NOTICE_OWNER: &str = "notice_owner";
const CONTEXT_OWNER: &str = "resolve channel notice owner";
const CONTEXT_BIND: &str = "bind channel notice owner";

/// A channel's notice owner, as its binding reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeOwner {
    /// The notice owner fleet.
    pub fleet: Uuid7,
    /// Lifecycle status; killed owners cannot acquire new notice obligations.
    pub status: FleetStatus,
}

impl Ingress {
    /// The fleet bound as `channel`'s notice owner in `team` on `provider`, if one
    /// is bound in `workspace`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    pub async fn notice_owner(
        &self,
        workspace: &Uuid7,
        provider: &str,
        team: &str,
        channel: &ChannelId,
    ) -> Result<Option<NoticeOwner>> {
        let mut connection = self.database.acquire().await?;
        let found: Option<OwnerRow> = sqlx::query_as(sql::SELECT_NOTICE_OWNER)
            .bind(provider)
            .bind(team)
            .bind(channel.as_str())
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_OWNER))?;
        found.map(OwnerRow::bound).transpose()
    }

    /// Binds `fleet` as `channel`'s notice owner unless `workspace` already has
    /// one bound, and answers whichever fleet is bound there. `now` mints the
    /// binding's id; Postgres stamps the row.
    ///
    /// # Errors
    /// Reports entropy or an instant that would not mint the binding's id, a
    /// datastore that would not answer, and a row this build cannot read.
    pub async fn bind_notice_owner(
        &self,
        workspace: &Uuid7,
        provider: &str,
        team: &str,
        channel: &ChannelId,
        fleet: &Uuid7,
        now: Timestamp,
    ) -> Result<Uuid7> {
        let id = self.entropy.uuid7(now)?;
        let mut connection = self.database.acquire().await?;
        let bound: Option<String> = sqlx::query_scalar(sql::INSERT_NOTICE_OWNER)
            .bind(id.as_str())
            .bind(provider)
            .bind(team)
            .bind(channel.as_str())
            .bind(fleet.as_str())
            .bind(KIND_NOTICE_OWNER)
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_BIND))?;
        drop(connection);
        match bound {
            Some(bound) => stored_fleet(&bound),
            // The insert waited on a concurrent one and did nothing, and its
            // own read was taken before that commit; a fresh statement sees it.
            None => self
                .notice_owner(workspace, provider, team, channel)
                .await?
                .map(|bound| bound.fleet)
                .ok_or_else(|| row_unreadable(COLUMN_FLEET)),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct OwnerRow {
    fleet_id: String,
    status: String,
}
impl OwnerRow {
    fn bound(self) -> Result<NoticeOwner> {
        Ok(NoticeOwner {
            fleet: stored_fleet(&self.fleet_id)?,
            status: stored_status(&self.status)?,
        })
    }
}
