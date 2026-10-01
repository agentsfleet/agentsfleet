//! The invite email: the seam the invite routes send through.
//!
//! A trait for the reason every seam here is one: a router suite proving the
//! refusal matrix holds no vault and reaches no relay. The production mailer
//! answers it directly.

use afd_core::id::Uuid7;
use afd_mail::{InviteMailer, InviteSend, Outcome};

/// Sends one invite email and reports what became of it, never failing.
pub trait InviteMail: Send + Sync + std::fmt::Debug + 'static {
    /// Sends `invite` through the relay `admin`'s vault names. See
    /// [`InviteMailer::send`].
    fn send(
        &self,
        admin: Option<&Uuid7>,
        invite: &InviteSend<'_>,
    ) -> impl Future<Output = Outcome> + Send;
}

/// The production mailer answers it directly.
impl InviteMail for InviteMailer {
    fn send(
        &self,
        admin: Option<&Uuid7>,
        invite: &InviteSend<'_>,
    ) -> impl Future<Output = Outcome> + Send {
        Self::send(self, admin, invite)
    }
}
