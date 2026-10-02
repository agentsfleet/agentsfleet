//! Listing an account's members, and removing one without leaving it ownerless.

use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::{Member, Removal, Team};
use crate::sql::member as sql;
use crate::sql::{COLUMN_EMAIL, COLUMN_ROLE};
use crate::workspace::access::{ROLE_OWNER, Role};
use crate::{Result, error, stored};

/// The context each statement failure here reports under.
const CONTEXT_LIST: &str = "list members";
const CONTEXT_REMOVE: &str = "remove member";

/// Where a member's user identifier comes from, as a malformed one names it.
const TABLE_MEMBERSHIPS: &str = "core.memberships";
const COLUMN_USER_ID: &str = "user_id";

impl Team {
    /// The account's members, oldest membership first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored role this
    /// build cannot read.
    pub async fn members(&self, tenant: &Uuid7) -> Result<Vec<Member>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_MEMBERS)
            .bind(tenant.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_LIST))?;
        rows.iter().map(member).collect()
    }

    /// Removes `user`'s membership in the account.
    ///
    /// The owners are locked before the count is read, so two removals cannot
    /// each see another owner standing and together leave none. They are
    /// locked before the removed row too: two owners removing each other then
    /// queue on the same first lock, where locking their own rows first had
    /// each wait on the other's until Postgres aborted one. Removing a
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

        let owners: Vec<i32> = sqlx::query_scalar(sql::LOCK_OWNERS)
            .bind(tenant.as_str())
            .bind(ROLE_OWNER)
            .fetch_all(&mut *transaction)
            .await
            .map_err(&raise)?;
        let held: Option<String> = sqlx::query_scalar(sql::LOCK_MEMBERSHIP)
            .bind(tenant.as_str())
            .bind(user.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&raise)?;
        let Some(role) = held else {
            return Ok(Removal::Absent);
        };
        if Role::parse(&role)? == Role::Owner && owners.len() <= 1 {
            return Err(error::member_last_owner());
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

/// One member from its row, read by column name.
///
/// Mapped by hand rather than derived: the user identifier and the role each
/// parse, and a role this build does not know must surface as its own error.
fn member(row: &PgRow) -> Result<Member> {
    let unreadable = error::query(CONTEXT_LIST);
    let user: String = row.try_get(COLUMN_USER_ID).map_err(&unreadable)?;
    let role: String = row.try_get(COLUMN_ROLE).map_err(&unreadable)?;
    Ok(Member {
        user: stored::uuid(TABLE_MEMBERSHIPS, COLUMN_USER_ID, &user)?,
        display_name: row.try_get("display_name").map_err(&unreadable)?,
        email: row.try_get(COLUMN_EMAIL).map_err(&unreadable)?,
        subject: row.try_get("oidc_subject").map_err(&unreadable)?,
        role: Role::parse(&role)?,
        joined_at_ms: row.try_get("joined_at").map_err(&unreadable)?,
    })
}
