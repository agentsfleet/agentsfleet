//! Issuing, listing and revoking invites, and reading what waits for an address.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use super::{INVITE_TTL_MS, Invite, NewInvite, Team, Waiting};
use crate::sql::invite as sql;
use crate::workspace::access::{ROLE_OWNER, Role};
use crate::{Result, error};

/// The context each statement failure here reports under.
const CONTEXT_ISSUE: &str = "issue invite";
const CONTEXT_LIST: &str = "list invites";
const CONTEXT_REVOKE: &str = "revoke invite";
const CONTEXT_WAITING: &str = "list waiting invites";

/// The index that holds an account to one pending invite per address.
const PENDING_CONSTRAINT: &str = "uq_invites_tenant_id_email_pending";

/// Sends attempted for an invite that was only just issued.
const NO_ATTEMPTS: i32 = 0;

/// One invite row: id, address, role, expiry, issue time.
type InviteRow = (String, String, String, i64, i64);

impl Team {
    /// Issues an invite for `new.email` into `new.tenant`, as a member.
    ///
    /// # Errors
    /// Refuses an address that already belongs to the account or already has
    /// a pending invite there; reports a datastore that would not answer.
    pub async fn invite(&self, new: &NewInvite<'_>, now: UnixMillis) -> Result<Invite> {
        let id = self.entropy.uuid7(now)?;
        let expires_at_ms = now.as_millis().saturating_add(INVITE_TTL_MS);
        let raise = error::query(CONTEXT_ISSUE);
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(&raise)?;

        let member: Option<(i32,)> = sqlx::query_as(sql::SELECT_MEMBER_BY_EMAIL)
            .bind(new.tenant.as_str())
            .bind(new.email.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&raise)?;
        if member.is_some() {
            return Err(error::invite_conflict());
        }
        sqlx::query(sql::REVOKE_EXPIRED_PENDING)
            .bind(new.tenant.as_str())
            .bind(new.email.as_str())
            .bind(now.as_millis())
            .execute(&mut *transaction)
            .await
            .map_err(&raise)?;
        sqlx::query(sql::INSERT_INVITE)
            .bind(id.as_str())
            .bind(new.tenant.as_str())
            .bind(new.email.as_str())
            .bind(Role::Member.wire())
            .bind(new.inviter.as_str())
            .bind(expires_at_ms)
            .bind(now.as_millis())
            .bind(NO_ATTEMPTS)
            .execute(&mut *transaction)
            .await
            .map_err(classify_insert)?;
        transaction.commit().await.map_err(&raise)?;

        let tenant_id = new.tenant.as_str();
        let invite_id = id.as_str();
        tracing::info!(tenant_id, invite_id, event = "workspace_invite_created");
        Ok(Invite {
            id,
            email: new.email.as_str().to_owned(),
            role: Role::Member,
            expires_at_ms,
            created_at_ms: now.as_millis(),
        })
    }

    /// The account's invites that can still be accepted, newest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this build
    /// cannot read.
    pub async fn invites(&self, tenant: &Uuid7, now: UnixMillis) -> Result<Vec<Invite>> {
        let mut connection = self.database.acquire().await?;
        let rows: Vec<InviteRow> = sqlx::query_as(sql::SELECT_TENANT_PENDING)
            .bind(tenant.as_str())
            .bind(now.as_millis())
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_LIST))?;
        rows.into_iter().map(invite).collect()
    }

    /// Revokes one of the account's pending invites.
    ///
    /// Idempotent: an invite already revoked, accepted, or never this
    /// account's changes nothing and is not an error.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn revoke_invite(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let revoked = sqlx::query(sql::REVOKE_INVITE)
            .bind(tenant.as_str())
            .bind(invite.as_str())
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_REVOKE))?;
        if revoked.rows_affected() > 0 {
            let tenant_id = tenant.as_str();
            let invite_id = invite.as_str();
            tracing::info!(tenant_id, invite_id, event = "workspace_invite_revoked");
        }
        Ok(())
    }

    /// The invites waiting for `email`, newest first, with the account each opens.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn waiting_for(&self, email: &str, now: UnixMillis) -> Result<Vec<Waiting>> {
        let mut connection = self.database.acquire().await?;
        let rows: Vec<(String, String, String, i64)> =
            sqlx::query_as(sql::SELECT_PENDING_FOR_EMAIL)
                .bind(email.to_lowercase())
                .bind(now.as_millis())
                .bind(ROLE_OWNER)
                .fetch_all(connection.as_mut())
                .await
                .map_err(error::query(CONTEXT_WAITING))?;
        Ok(rows
            .into_iter()
            .map(|(id, tenant, owner_name, expires_at_ms)| Waiting {
                id,
                tenant,
                owner_name,
                expires_at_ms,
            })
            .collect())
    }
}

/// One invite from its row.
fn invite((id, email, role, expires_at_ms, created_at_ms): InviteRow) -> Result<Invite> {
    Ok(Invite {
        id: Uuid7::parse(&id).map_err(error::row_malformed("core.invites", "id"))?,
        email,
        role: Role::parse(&role)?,
        expires_at_ms,
        created_at_ms,
    })
}

/// Tells a second pending invite apart from a broken statement.
///
/// By exact constraint: the table's primary key is also unique, and an
/// identifier collision is not a fact about the address.
fn classify_insert(source: sqlx::Error) -> crate::Error {
    let pending = source.as_database_error().is_some_and(|failure| {
        failure.is_unique_violation() && failure.constraint() == Some(PENDING_CONSTRAINT)
    });
    if pending {
        error::invite_conflict()
    } else {
        error::query(CONTEXT_ISSUE)(source)
    }
}
