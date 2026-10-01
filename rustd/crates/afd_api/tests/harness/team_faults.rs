//! The team store with one write a suite can break, on one call of it.
//!
//! Every method answers from the live store. A suite names a step and which
//! call of it to break; that call alone goes to a store over a pool that
//! answers nothing, so the refusal is the one `afd_tenant` really raises when
//! its datastore is gone, through the real handler.
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

/// The writes after an invite commits, where a failure must not undo it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TeamStep {
    /// Counting one more send against the invite.
    BeginEmail,
    /// Recording what became of a send.
    RecordEmail,
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

    /// The store this call of `step` goes to.
    fn store_for(&self, step: TeamStep) -> &Team {
        match &self.failpoint {
            Some(failpoint) if failpoint.breaks(step) => &self.broken,
            _ => &self.live,
        }
    }
}

impl TenantTeam for HarnessTeam {
    fn invite(
        &self,
        new: &NewInvite<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Invitation>> + Send {
        self.live.invite(new, now)
    }

    fn begin_email(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Option<EmailAttempt>>> + Send {
        self.store_for(TeamStep::BeginEmail)
            .begin_email(tenant, invite, now)
    }

    fn record_email(
        &self,
        invite: &Uuid7,
        attempt: i32,
        status: EmailStatus,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        self.store_for(TeamStep::RecordEmail)
            .record_email(invite, attempt, status, now)
    }

    fn invitations(
        &self,
        tenant: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Invitation>>> + Send {
        self.live.invitations(tenant, now)
    }

    fn revoke_invitation(
        &self,
        tenant: &Uuid7,
        invite: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        self.live.revoke_invitation(tenant, invite, now)
    }

    fn waiting_for(
        &self,
        email: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Waiting>>> + Send {
        self.live.waiting_for(email, now)
    }

    fn accept(
        &self,
        invite: &Uuid7,
        invitee: &Invitee<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Accepted>> + Send {
        self.live.accept(invite, invitee, now)
    }

    fn members(
        &self,
        tenant: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Vec<Member>>> + Send {
        self.live.members(tenant)
    }

    fn remove(
        &self,
        tenant: &Uuid7,
        user: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Removal>> + Send {
        self.live.remove(tenant, user)
    }
}
