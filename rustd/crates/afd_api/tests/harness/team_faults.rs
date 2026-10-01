//! The team store with one call a suite can break, on one occurrence of it.
//!
//! Every method answers through the production `TenantTeam` impl over the live
//! store. A suite names a step and which call of it to break; that call alone
//! goes to a store over a pool that answers nothing, so the refusal is the one
//! `afd_tenant` really raises when its datastore is gone, through the real
//! handler.
//!
//! A struct and a trait impl, not a closure: the seam stands in for a whole
//! store whose methods share one failpoint. The failpoint counts with atomics,
//! so concurrent requests through one router select the same call each run.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_http::services::TenantTeam;
use afd_tenant::team::{
    Accepted, EmailAttempt, EmailStatus, Invitation, Invitee, Member, NewInvite, Removal, Team,
    Waiting,
};

use super::readiness::unreachable_pool;

/// The team-store calls a suite can break, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TeamStep {
    /// Issuing the invite: the store refuses before anything commits.
    Invite,
    /// Counting one more send against the invite.
    BeginEmail,
    /// The invite stops being sendable between its commit and its count, as a
    /// revoke landing in that gap would leave it: the count answers nothing.
    BeginEmailGone,
    /// Recording what became of a send.
    RecordEmail,
    /// Removing a member: the store refuses.
    Remove,
}

/// Which call of which step breaks, and how often it did.
#[derive(Debug)]
pub(crate) struct Failpoint {
    step: TeamStep,
    /// The call to break, counting from one.
    ordinal: usize,
    seen: AtomicUsize,
    fired: AtomicUsize,
}

impl Failpoint {
    /// How many calls the failpoint broke: one, when the suite reached it.
    pub(crate) fn fired(&self) -> usize {
        self.fired.load(Ordering::SeqCst)
    }

    /// Whether this call of `step` is the one to break.
    fn breaks(&self, step: TeamStep) -> bool {
        if step != self.step {
            return false;
        }
        let call = self.seen.fetch_add(1, Ordering::SeqCst) + 1;
        let selected = call == self.ordinal;
        if selected {
            self.fired.fetch_add(1, Ordering::SeqCst);
        }
        selected
    }
}

/// The live team store, and the broken one a failpoint routes a call to.
#[derive(Debug, Clone)]
pub(crate) struct HarnessTeam {
    live: Team,
    broken: Team,
    failpoint: Option<Arc<Failpoint>>,
}

impl HarnessTeam {
    /// The live store over `database`, with nothing broken.
    pub(crate) fn new(database: Db) -> Self {
        Self {
            live: Team::new(database, Entropy::new()),
            broken: Team::new(Db::unreachable(&unreachable_pool()), Entropy::new()),
            failpoint: None,
        }
    }

    /// The same store, breaking call `ordinal` of `step`; the failpoint is
    /// handed back so the suite can prove it fired.
    pub(crate) fn breaking(mut self, step: TeamStep, ordinal: usize) -> (Self, Arc<Failpoint>) {
        let failpoint = Arc::new(Failpoint {
            step,
            ordinal,
            seen: AtomicUsize::new(0),
            fired: AtomicUsize::new(0),
        });
        self.failpoint = Some(Arc::clone(&failpoint));
        (self, failpoint)
    }

    /// Whether this call of `step` is the one the suite breaks.
    fn breaks(&self, step: TeamStep) -> bool {
        self.failpoint
            .as_ref()
            .is_some_and(|failpoint| failpoint.breaks(step))
    }

    /// The store this call of `step` goes to.
    fn store_for(&self, step: TeamStep) -> &Team {
        if self.breaks(step) {
            &self.broken
        } else {
            &self.live
        }
    }
}

impl TenantTeam for HarnessTeam {
    fn invite(
        &self,
        new: &NewInvite<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Invitation>> + Send {
        TenantTeam::invite(self.store_for(TeamStep::Invite), new, now)
    }

    fn begin_email(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Option<EmailAttempt>>> + Send {
        let gone = self.breaks(TeamStep::BeginEmailGone);
        let store = self.store_for(TeamStep::BeginEmail);
        async move {
            if gone {
                return Ok(None);
            }
            TenantTeam::begin_email(store, tenant, invite, now).await
        }
    }

    fn record_email(
        &self,
        invite: &Uuid7,
        attempt: i32,
        status: EmailStatus,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        TenantTeam::record_email(
            self.store_for(TeamStep::RecordEmail),
            invite,
            attempt,
            status,
            now,
        )
    }

    fn invitations(
        &self,
        tenant: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Invitation>>> + Send {
        TenantTeam::invitations(&self.live, tenant, now)
    }

    fn revoke_invitation(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        TenantTeam::revoke_invitation(&self.live, tenant, invite, now)
    }

    fn waiting_for(
        &self,
        email: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Waiting>>> + Send {
        TenantTeam::waiting_for(&self.live, email, now)
    }

    fn accept(
        &self,
        invite: &Uuid7,
        invitee: &Invitee<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Accepted>> + Send {
        TenantTeam::accept(&self.live, invite, invitee, now)
    }

    fn members(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Member>>> + Send {
        TenantTeam::members(&self.live, tenant)
    }

    fn remove(
        &self,
        tenant: &Uuid7,
        user: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Removal>> + Send {
        TenantTeam::remove(self.store_for(TeamStep::Remove), tenant, user)
    }
}
