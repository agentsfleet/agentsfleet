//! What happens to an invitation: issued by an owner, listed, revoked, found
//! by the address it is for, and accepted.
//!
//! Accepting is one transaction with the invitation row locked from the first
//! read. The lock makes two tabs accepting at once a single accept: the second
//! waits, then reads the first one's stamp and answers as it did. The
//! transaction makes a failure between the two writes leave neither, since a
//! membership with no accepted invitation behind it is access nobody can
//! account for.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::constraint::violates_unique;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::{
    Acceptance, COLUMN_EXPIRES_AT, COLUMN_ID, COLUMN_TENANT_ID, INVITE_TTL_MS, Invitation,
};
use crate::sql::invite as sql;
use crate::team::{Accepted, Invitee, NewInvite, Team, Waiting, email};
use crate::workspace::access::{ROLE_OWNER, Role};
use crate::{Result, error};

/// The context each statement failure here reports under.
const CONTEXT_ISSUE: &str = "issue invitation";
const CONTEXT_LIST: &str = "list invitations";
const CONTEXT_REVOKE: &str = "revoke invitation";
const CONTEXT_WAITING: &str = "list waiting invitations";
const CONTEXT_ACCEPT: &str = "accept invitation";

/// The index that holds an account to one pending invitation per address.
const PENDING_CONSTRAINT: &str = "uq_invites_tenant_id_email_pending";

/// Sends attempted for an invitation that was only just issued.
const NO_ATTEMPTS: i32 = 0;

impl Team {
    /// Issues an invitation for `new.email` into `new.tenant`, as a member.
    ///
    /// # Errors
    /// Refuses an address that already belongs to the account or already has
    /// a pending invitation there; reports a datastore that would not answer.
    pub async fn invite(&self, new: &NewInvite<'_>, now: UnixMillis) -> Result<Invitation> {
        let id = self.entropy.uuid7(now)?;
        let expires_at_ms = now.saturating_add_millis(INVITE_TTL_MS).as_millis();
        let raise = error::query(CONTEXT_ISSUE);
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(&raise)?;

        let member: Option<i32> = sqlx::query_scalar(sql::SELECT_MEMBER_BY_EMAIL)
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
        Ok(Invitation {
            id,
            tenant: new.tenant.clone(),
            email: new.email.as_str().to_owned(),
            role: Role::Member,
            expires_at_ms,
            created_at_ms: now.as_millis(),
            accepted_by: None,
            revoked_at_ms: None,
        })
    }

    /// The account's invitations that can still be accepted, newest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this build
    /// cannot read.
    pub async fn invitations(&self, tenant: &Uuid7, now: UnixMillis) -> Result<Vec<Invitation>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_TENANT_PENDING)
            .bind(tenant.as_str())
            .bind(now.as_millis())
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_LIST))?;
        rows.iter().map(Invitation::read).collect()
    }

    /// Revokes one of the account's pending invitations.
    ///
    /// Idempotent: an invitation already revoked, accepted, or never this
    /// account's changes nothing and is not an error.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn revoke_invitation(
        &self,
        tenant: &Uuid7,
        invitation: &Uuid7,
        now: UnixMillis,
    ) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let revoked = sqlx::query(sql::REVOKE_INVITE)
            .bind(tenant.as_str())
            .bind(invitation.as_str())
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_REVOKE))?;
        if revoked.rows_affected() > 0 {
            let tenant_id = tenant.as_str();
            let invite_id = invitation.as_str();
            tracing::info!(tenant_id, invite_id, event = "workspace_invite_revoked");
        }
        Ok(())
    }

    /// The invitations waiting for `address`, newest first, with the account
    /// each opens.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn waiting_for(&self, address: &str, now: UnixMillis) -> Result<Vec<Waiting>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_PENDING_FOR_EMAIL)
            .bind(email::fold(address))
            .bind(now.as_millis())
            .bind(ROLE_OWNER)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_WAITING))?;
        rows.iter().map(waiting).collect()
    }

    /// Accepts `invitation` as `invitee`, making them a member of its account.
    ///
    /// Accepting again answers as the first accept did, while the membership
    /// stands; a member removed since cannot rejoin through the old link.
    ///
    /// # Errors
    /// Refuses an invitation that cannot be accepted any more, and one sent
    /// to another address; reports a datastore that would not answer.
    pub async fn accept(
        &self,
        invitation: &Uuid7,
        invitee: &Invitee<'_>,
        now: UnixMillis,
    ) -> Result<Accepted> {
        let membership = self.entropy.uuid7(now)?;
        let raise = error::query(CONTEXT_ACCEPT);
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(&raise)?;

        let row = sqlx::query(sql::LOCK_INVITE)
            .bind(invitation.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&raise)?
            .ok_or_else(error::invite_not_found)?;
        let locked = Invitation::read(&row)?;
        let tenant = locked.tenant.as_str();
        settle(&mut transaction, &locked, invitee, &membership, now).await?;
        let workspaces: Vec<String> = sqlx::query_scalar(sql::SELECT_TENANT_WORKSPACE_IDS)
            .bind(tenant)
            .fetch_all(&mut *transaction)
            .await
            .map_err(&raise)?;
        transaction.commit().await.map_err(&raise)?;

        let invite_id = invitation.as_str();
        let user_id = invitee.user.as_str();
        tracing::info!(
            tenant_id = tenant,
            invite_id,
            user_id,
            event = "workspace_invite_accepted"
        );
        Ok(Accepted {
            tenant: locked.tenant,
            workspaces,
        })
    }
}

/// Writes what accepting `invitation` means for `invitee`, inside the accept's
/// transaction: a new membership and the invitation's stamp, or a check that a
/// replay's membership still stands.
async fn settle(
    transaction: &mut sqlx::PgConnection,
    invitation: &Invitation,
    invitee: &Invitee<'_>,
    membership: &Uuid7,
    now: UnixMillis,
) -> Result<()> {
    let raise = error::query(CONTEXT_ACCEPT);
    let tenant = invitation.tenant.as_str();
    match invitation.acceptance_for(invitee, now) {
        Acceptance::Closed => Err(error::invite_not_found()),
        Acceptance::WrongAddress => Err(error::invite_email_mismatch()),
        Acceptance::AlreadyJoined => {
            let held: Option<i32> = sqlx::query_scalar(sql::SELECT_MEMBERSHIP_EXISTS)
                .bind(tenant)
                .bind(invitee.user.as_str())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(&raise)?;
            held.map(|_| ()).ok_or_else(error::invite_not_found)
        }
        Acceptance::Join => {
            sqlx::query(sql::INSERT_MEMBERSHIP)
                .bind(membership.as_str())
                .bind(tenant)
                .bind(invitee.user.as_str())
                .bind(invitation.role.wire())
                .bind(now.as_millis())
                .execute(&mut *transaction)
                .await
                .map_err(&raise)?;
            sqlx::query(sql::MARK_ACCEPTED)
                .bind(invitation.id.as_str())
                .bind(invitee.user.as_str())
                .bind(now.as_millis())
                .execute(&mut *transaction)
                .await
                .map_err(&raise)?;
            Ok(())
        }
    }
}

/// One invitation waiting for an address, read by column name.
fn waiting(row: &PgRow) -> Result<Waiting> {
    let unreadable = error::query(CONTEXT_WAITING);
    Ok(Waiting {
        id: row.try_get(COLUMN_ID).map_err(&unreadable)?,
        tenant: row.try_get(COLUMN_TENANT_ID).map_err(&unreadable)?,
        owner_name: row.try_get("owner_name").map_err(&unreadable)?,
        expires_at_ms: row.try_get(COLUMN_EXPIRES_AT).map_err(&unreadable)?,
    })
}

/// Tells a second pending invitation apart from a broken statement.
fn classify_insert(source: sqlx::Error) -> crate::Error {
    if violates_unique(&source, PENDING_CONSTRAINT) {
        error::invite_conflict()
    } else {
        error::query(CONTEXT_ISSUE)(source)
    }
}
