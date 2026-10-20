//! Listing an account's members, and removing one without leaving it ownerless.

use afd_core::id::Uuid7;
use jiff::Timestamp;

use super::{Member, Removal, Team};
use crate::sql::member as sql;
use crate::workspace::access::{ROLE_OWNER, Role};
use crate::{Result, error, stored};

/// The context each statement failure here reports under.
const CONTEXT_LIST: &str = "list members";
const CONTEXT_REMOVE: &str = "remove member";

/// Where a member's user identifier comes from, as a malformed one names it.
const TABLE_MEMBERSHIPS: &str = "identity.memberships";
const COLUMN_USER_ID: &str = "user_id";

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
        rows.into_iter().map(MemberRow::parse).collect()
    }

    /// Removes `user`'s membership in the account.
    ///
    /// Nothing is locked; see [`sql::COUNT_OWNERS`] for the race that leaves
    /// open. Removing a membership that does not exist is
    /// [`Removal::Absent`], not an error.
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

        let owners: i64 = sqlx::query_scalar(sql::COUNT_OWNERS)
            .bind(tenant.as_str())
            .bind(ROLE_OWNER)
            .fetch_one(&mut *transaction)
            .await
            .map_err(&raise)?;
        let held: Option<String> = sqlx::query_scalar(sql::SELECT_MEMBERSHIP)
            .bind(tenant.as_str())
            .bind(user.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&raise)?;
        let Some(role) = held else {
            return Ok(Removal::Absent);
        };
        if Role::parse(&role)? == Role::Owner && owners <= 1 {
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

/// One member's row, by column name.
#[derive(Debug, sqlx::FromRow)]
struct MemberRow {
    user_id: String,
    display_name: Option<String>,
    email: String,
    oidc_subject: String,
    role: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    joined_at: Timestamp,
}

impl MemberRow {
    /// The member this row stores. The user identifier and the role each
    /// parse, and a role this build does not know surfaces as its own error.
    fn parse(self) -> Result<Member> {
        Ok(Member {
            user: stored::uuid(TABLE_MEMBERSHIPS, COLUMN_USER_ID, &self.user_id)?,
            display_name: self.display_name,
            email: self.email,
            subject: self.oidc_subject,
            role: Role::parse(&self.role)?,
            joined_at: self.joined_at,
        })
    }
}
