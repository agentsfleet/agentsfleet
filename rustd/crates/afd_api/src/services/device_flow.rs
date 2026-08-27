//! The device-flow login seam: opening, approving and redeeming one sign-in.

use afd_core::clock::UnixMillis;
use afd_tenant::session::{Cancelled, Fingerprint, Opened, Redeemed, Waiting, input};

/// Opening, approving and redeeming one command-line login.
///
/// Every method takes ALREADY-PARSED values — an [`input::Opening`] cannot hold
/// an oversized key and an [`input::Code`] cannot hold five digits — so there is
/// no validation arm in any implementation of this trait, and none that a stub
/// could implement differently from the real one.
pub trait DeviceFlow: Send + Sync + std::fmt::Debug + 'static {
    /// Opens a login, answering its id and the page a person approves it on.
    ///
    /// # Errors
    /// Reports a host that cannot draw entropy, and a queue that would not
    /// answer.
    fn open(
        &self,
        opening: &input::Opening<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Opened>> + Send;

    /// Reads where a login has got to.
    ///
    /// # Errors
    /// Refuses an id naming nothing held and each terminal state with its own
    /// registry code; reports a queue that would not answer.
    fn poll(&self, session_id: &str) -> impl Future<Output = afd_tenant::Result<Waiting>> + Send;

    /// Records one dashboard approval.
    ///
    /// # Errors
    /// Refuses an id naming nothing held and a session already past pending;
    /// reports a queue that would not answer.
    fn approve(
        &self,
        session_id: &str,
        approval: &input::Approval<'_>,
        approver: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send;

    /// Presents a code, redeeming the session if it matches.
    ///
    /// # Errors
    /// Refuses every terminal state, a session no human has approved, and a
    /// code that did not match; reports a queue that would not answer.
    fn verify(
        &self,
        session_id: &str,
        code: &input::Code<'_>,
        fingerprint: &Fingerprint,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Redeemed>> + Send;

    /// Cancels one login held by `owner`.
    ///
    /// # Errors
    /// Refuses an id naming nothing held, a foreign session, and one already
    /// redeemed; reports a queue that would not answer.
    fn cancel(
        &self,
        session_id: &str,
        owner: &str,
    ) -> impl Future<Output = afd_tenant::Result<Cancelled>> + Send;

    /// Cancels every in-flight login `owner` holds, answering their ids.
    ///
    /// # Errors
    /// Reports a queue that would not answer.
    fn cancel_all(
        &self,
        owner: &str,
    ) -> impl Future<Output = afd_tenant::Result<Vec<String>>> + Send;
}

/// The production surface answers it directly.
///
/// Forwarding rather than `async fn` throughout: every method already has the
/// future the service returns, so there is no state machine to build here.
impl DeviceFlow for afd_tenant::session::Sessions {
    fn open(
        &self,
        opening: &input::Opening<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Opened>> + Send {
        Self::open(self, opening, now)
    }

    fn poll(&self, session_id: &str) -> impl Future<Output = afd_tenant::Result<Waiting>> + Send {
        Self::poll(self, session_id)
    }

    fn approve(
        &self,
        session_id: &str,
        approval: &input::Approval<'_>,
        approver: &str,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<()>> + Send {
        Self::approve(self, session_id, approval, approver, now)
    }

    fn verify(
        &self,
        session_id: &str,
        code: &input::Code<'_>,
        fingerprint: &Fingerprint,
        now: UnixMillis,
    ) -> impl Future<Output = afd_tenant::Result<Redeemed>> + Send {
        Self::verify(self, session_id, code, fingerprint, now)
    }

    fn cancel(
        &self,
        session_id: &str,
        owner: &str,
    ) -> impl Future<Output = afd_tenant::Result<Cancelled>> + Send {
        Self::cancel(self, session_id, owner)
    }

    fn cancel_all(
        &self,
        owner: &str,
    ) -> impl Future<Output = afd_tenant::Result<Vec<String>>> + Send {
        Self::cancel_all(self, owner)
    }
}
