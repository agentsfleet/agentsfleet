//! Listing an account's members, and removing one without leaving it ownerless.

use afd_core::id::Uuid7;

use super::{Member, Removal, Team};
use crate::sql::member as sql;
use crate::workspace::access::{ROLE_OWNER, Role};
use crate::{Result, error};

/// The context each statement failure here reports under.
const CONTEXT_LIST: &str = "list members";
const CONTEXT_REMOVE: &str = "remove member";

/// One member row: user, display name, address, role, when they joined.
type MemberRow = (String, Option<String>, String, String, i64);

impl Team {
    /// The account's members, oldest membership first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored role this
    /// build cannot read.
    pub async fn members(&self, tenant: &Uuid7) -> Result<Vec<Member>> {
        let mut connection = self.database.acquire().await?;
        let rows: Vec<MemberRow> = sqlx::query_as(sql::SELECT_MEMBERS)
            .bind(tenant.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_LIST))?;
        rows.into_iter()
            .map(|(user, display_name, email, role, joined_at_ms)| {
                Ok(Member {
                    user,
                    display_name,
                    email,
                    role: Role::parse(&role)?,
                    joined_at_ms,
                })
            })
            .collect()
    }

    /// Removes `user`'s membership in the account.
    ///
    /// The owners are locked before the count is read, so two removals cannot
    /// each see another owner standing and together leave none. Removing a
    /// membership that does not exist is [`Removal::Absent`], not an error.
    ///
    /// # Errors
    /// Refuses removing the account's last owner; reports a datastore that
    /// would not answer.
    pub async fn remove(&self, tenant: &Uuid7, user: &Uuid7) -> Result<Removal> {
        let raise = error::query(CONTEXT_REMOVE);
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(&raise)?;

        let held: Option<(String,)> = sqlx::query_as(sql::LOCK_MEMBERSHIP)
            .bind(tenant.as_str())
            .bind(user.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&raise)?;
        let Some((role,)) = held else {
            return Ok(Removal::Absent);
        };
        if Role::parse(&role)? == Role::Owner {
            let owners: Vec<(String,)> = sqlx::query_as(sql::LOCK_OWNERS)
                .bind(tenant.as_str())
                .bind(ROLE_OWNER)
                .fetch_all(&mut *transaction)
                .await
                .map_err(&raise)?;
            if owners.len() <= 1 {
                return Err(error::member_last_owner());
            }
        }
        sqlx::query(sql::DELETE_MEMBERSHIP)
            .bind(tenant.as_str())
            .bind(user.as_str())
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;
        transaction.commit().await.map_err(&raise)?;

        let tenant_id = tenant.as_str();
        let user_id = user.as_str();
        tracing::info!(tenant_id, user_id, event = "workspace_member_removed");
        Ok(Removal::Removed)
    }
}
