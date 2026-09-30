//! The people in an account: the seam the invite and member routes act through.
//!
//! A trait for the reason every seam in this module is one: the router suites
//! prove the refusal matrix in front of these handlers with no datastore. The
//! production store answers it directly.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_tenant::team::{Accepted, Invitation, Invitee, Member, NewInvite, Removal, Team, Waiting};

/// Invites into an account, their acceptance, and its members.
pub trait TenantTeam: Send + Sync + std::fmt::Debug + 'static {
    /// Issues an invite. See [`Team::invite`].
    fn invite(
        &self,
        new: &NewInvite<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Invitation>> + Send;

    /// The account's acceptable invitations. See [`Team::invitations`].
    fn invitations(
        &self,
        tenant: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Invitation>>> + Send;

    /// Revokes one invitation, idempotently. See [`Team::revoke_invitation`].
    fn revoke_invitation(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send;

    /// What waits for an address. See [`Team::waiting_for`].
    fn waiting_for(
        &self,
        email: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Waiting>>> + Send;

    /// Accepts an invite. See [`Team::accept`].
    fn accept(
        &self,
        invite: &Uuid7,
        invitee: &Invitee<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Accepted>> + Send;

    /// The account's members. See [`Team::members`].
    fn members(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Member>>> + Send;

    /// Removes a member, never the last owner. See [`Team::remove`].
    fn remove(
        &self,
        tenant: &Uuid7,
        user: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Removal>> + Send;
}

/// The production store answers it directly.
impl TenantTeam for Team {
    fn invite(
        &self,
        new: &NewInvite<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Invitation>> + Send {
        Self::invite(self, new, now)
    }

    fn invitations(
        &self,
        tenant: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Invitation>>> + Send {
        Self::invitations(self, tenant, now)
    }

    fn revoke_invitation(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        Self::revoke_invitation(self, tenant, invite, now)
    }

    fn waiting_for(
        &self,
        email: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Waiting>>> + Send {
        Self::waiting_for(self, email, now)
    }

    fn accept(
        &self,
        invite: &Uuid7,
        invitee: &Invitee<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Accepted>> + Send {
        Self::accept(self, invite, invitee, now)
    }

    fn members(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Member>>> + Send {
        Self::members(self, tenant)
    }

    fn remove(
        &self,
        tenant: &Uuid7,
        user: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Removal>> + Send {
        Self::remove(self, tenant, user)
    }
}
