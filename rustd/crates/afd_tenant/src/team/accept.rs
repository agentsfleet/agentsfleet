//! Accepting an invite: the membership and the invite's stamp, together.
//!
//! One transaction, with the invite row locked from the first read. The lock
//! is what makes two tabs accepting at once a single accept: the second waits,
//! then reads the first one's stamp and answers as it did. The transaction is
//! what makes a failure between the two writes leave neither: a membership with
//! no accepted invite behind it is access nobody can account for.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::{Accepted, Invitee, Team, email};
use crate::sql::invite as sql;
use crate::workspace::access::Role;
use crate::{Result, error};

/// The locked row's account column, read and reported under one name.
const COLUMN_TENANT_ID: &str = "tenant_id";

/// The context each statement failure here reports under.
const CONTEXT_ACCEPT: &str = "accept invite";

/// The locked invite row.
struct LockedInvite {
    tenant: String,
    email: String,
    role: String,
    expires_at: i64,
    accepted_by: Option<String>,
    revoked_at: Option<i64>,
}

impl LockedInvite {
    /// Reads the row by column name; a `try_get` failure names the column.
    fn read(row: &PgRow) -> Result<Self> {
        let unreadable = error::query(CONTEXT_ACCEPT);
        Ok(Self {
            tenant: row.try_get(COLUMN_TENANT_ID).map_err(&unreadable)?,
            email: row.try_get("email").map_err(&unreadable)?,
            role: row.try_get("role").map_err(&unreadable)?,
            expires_at: row.try_get("expires_at").map_err(&unreadable)?,
            accepted_by: row.try_get("accepted_by").map_err(&unreadable)?,
            revoked_at: row.try_get("revoked_at").map_err(&unreadable)?,
        })
    }
}

/// Where an invite stands for this invitee, decided from its locked row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    /// Acceptable by this invitee now.
    Open,
    /// Already accepted by this invitee: a replay.
    Theirs,
    /// Sent to another address.
    Elsewhere,
    /// Expired, revoked, or accepted by somebody else.
    Closed,
}

impl Standing {
    fn of(row: &LockedInvite, invitee: &Invitee<'_>, now: UnixMillis) -> Self {
        match row.accepted_by.as_deref() {
            Some(user) if user == invitee.user.as_str() => Self::Theirs,
            Some(_) => Self::Closed,
            None if row.revoked_at.is_some() || row.expires_at <= now.as_millis() => Self::Closed,
            None if row.email != email::fold(invitee.email) => Self::Elsewhere,
            None => Self::Open,
        }
    }
}

impl Team {
    /// Accepts `invite` as `invitee`, making them a member of its account.
    ///
    /// Accepting again answers as the first accept did, while the membership
    /// stands; a member removed since cannot rejoin through the old link.
    ///
    /// # Errors
    /// Refuses an invite that cannot be accepted any more, and one sent to
    /// another address; reports a datastore that would not answer.
    pub async fn accept(
        &self,
        invite: &Uuid7,
        invitee: &Invitee<'_>,
        now: UnixMillis,
    ) -> Result<Accepted> {
        let membership = self.entropy.uuid7(now)?;
        let raise = error::query(CONTEXT_ACCEPT);
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(&raise)?;

        let locked = sqlx::query(sql::LOCK_INVITE)
            .bind(invite.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(&raise)?
            .ok_or_else(error::invite_not_found)?;
        let row = LockedInvite::read(&locked)?;
        let (tenant, role) = (&row.tenant, &row.role);
        match Standing::of(&row, invitee, now) {
            Standing::Closed => return Err(error::invite_not_found()),
            Standing::Elsewhere => return Err(error::invite_email_mismatch()),
            Standing::Theirs => {
                let held: Option<i32> = sqlx::query_scalar(sql::SELECT_MEMBERSHIP_EXISTS)
                    .bind(tenant)
                    .bind(invitee.user.as_str())
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(&raise)?;
                held.ok_or_else(error::invite_not_found)?;
            }
            Standing::Open => {
                sqlx::query(sql::INSERT_MEMBERSHIP)
                    .bind(membership.as_str())
                    .bind(tenant)
                    .bind(invitee.user.as_str())
                    .bind(Role::parse(role)?.wire())
                    .bind(now.as_millis())
                    .execute(&mut *transaction)
                    .await
                    .map_err(&raise)?;
                sqlx::query(sql::MARK_ACCEPTED)
                    .bind(invite.as_str())
                    .bind(invitee.user.as_str())
                    .bind(now.as_millis())
                    .execute(&mut *transaction)
                    .await
                    .map_err(&raise)?;
            }
        }
        let workspaces: Vec<String> = sqlx::query_scalar(sql::SELECT_TENANT_WORKSPACE_IDS)
            .bind(tenant)
            .fetch_all(&mut *transaction)
            .await
            .map_err(&raise)?;
        transaction.commit().await.map_err(&raise)?;

        let tenant =
            Uuid7::parse(tenant).map_err(error::row_malformed("core.invites", COLUMN_TENANT_ID))?;
        let tenant_id = tenant.as_str();
        let invite_id = invite.as_str();
        let user_id = invitee.user.as_str();
        tracing::info!(
            tenant_id,
            invite_id,
            user_id,
            event = "workspace_invite_accepted"
        );
        Ok(Accepted { tenant, workspaces })
    }
}

#[cfg(test)]
mod tests {
    use afd_core::clock::UnixMillis;
    use afd_core::id::Uuid7;

    use super::{LockedInvite, Standing};
    use crate::team::Invitee;

    const NOW: i64 = 1_767_225_600_000;
    const BOB: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";
    const CAROL: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1012";

    fn row(accepted_by: Option<&str>, revoked_at: Option<i64>, expires_at: i64) -> LockedInvite {
        LockedInvite {
            tenant: String::new(),
            email: "bob@example.com".to_owned(),
            role: String::new(),
            expires_at,
            accepted_by: accepted_by.map(str::to_owned),
            revoked_at,
        }
    }

    fn standing(row: &LockedInvite, email: &str) -> Standing {
        let bob = Uuid7::parse(BOB).ok();
        let Some(bob) = bob.as_ref() else {
            return Standing::Closed;
        };
        Standing::of(
            row,
            &Invitee { user: bob, email },
            UnixMillis::from_millis(NOW),
        )
    }

    #[test]
    fn an_open_invite_is_acceptable_by_its_address_in_any_case() {
        assert_eq!(
            standing(&row(None, None, NOW + 1), "Bob@Example.com"),
            Standing::Open
        );
    }

    #[test]
    fn an_invite_the_invitee_accepted_is_a_replay() {
        assert_eq!(
            standing(&row(Some(BOB), None, NOW + 1), "bob@example.com"),
            Standing::Theirs
        );
    }

    #[test]
    fn expired_revoked_and_someone_elses_are_one_answer() {
        for closed in [
            row(None, None, NOW),
            row(None, Some(NOW - 1), NOW + 1),
            row(Some(CAROL), None, NOW + 1),
        ] {
            assert_eq!(standing(&closed, "bob@example.com"), Standing::Closed);
        }
    }

    #[test]
    fn an_open_invite_for_another_address_is_refused_as_such() {
        assert_eq!(
            standing(&row(None, None, NOW + 1), "carol@example.com"),
            Standing::Elsewhere
        );
    }
}
