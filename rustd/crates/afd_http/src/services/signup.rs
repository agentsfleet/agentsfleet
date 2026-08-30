//! The seam the identity provider's signup event acts through.
//!
//! One trait with one method, because the route does exactly one thing that
//! touches a store: hand a verified event to provisioning and render whatever
//! comes back. Everything before it — the signature, the event type, the
//! address — is decided on bytes the handler already holds.
//!
//! # Why the secret is a boot value and not a vault read
//!
//! Every other ingress secret is reached through the fleet or workspace the
//! delivery names. This one cannot be: a signup event names nobody yet — that
//! is the point of it — so there is no binding to read a secret through. It is
//! configured once for the deployment and resolved at boot, exactly as the App
//! ingress secret is.

use afd_core::clock::UnixMillis;
use afd_crypto::secret::SecretBytes;
use afd_tenant::Result as TenantResult;

// Re-exported so a handler names this seam rather than the store crate behind
// it: the ingress plane has no business depending on `afd_tenant` to spell the
// argument of a trait it already imports.
pub use afd_tenant::signup::{Bootstrapped, NewAccount, personal_tenant_name};

/// Opening a personal account from a verified signup event.
pub trait Signups: Send + Sync + std::fmt::Debug + 'static {
    /// Resolves the event to an account, opening one if there is none.
    ///
    /// A replay is a success carrying `created: false`, never an error — an
    /// identity provider retries, and a retry must answer as the first
    /// delivery did.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement that failed, and
    /// an entropy source that would not answer.
    fn bootstrap(
        &self,
        account: NewAccount<'_>,
        tenant_name: &str,
        now: UnixMillis,
    ) -> impl Future<Output = TenantResult<Bootstrapped>> + Send;
}

/// What this deployment verifies a signup event's signature against.
///
/// `None` is fail-closed and a real deployment state: a daemon configured with
/// no secret refuses every delivery, because accepting an unverified one on a
/// public endpoint that CREATES ACCOUNTS is strictly worse than serving none.
pub trait IdentityWebhookSecret: Send + Sync + 'static {
    /// The configured secret, when this deployment has one.
    fn identity_webhook_secret(&self) -> Option<&SecretBytes>;
}

impl Signups for afd_tenant::signup::Signups {
    fn bootstrap(
        &self,
        account: NewAccount<'_>,
        tenant_name: &str,
        now: UnixMillis,
    ) -> impl Future<Output = TenantResult<Bootstrapped>> + Send {
        Self::bootstrap(self, account, tenant_name, now)
    }
}

impl IdentityWebhookSecret for Option<SecretBytes> {
    fn identity_webhook_secret(&self) -> Option<&SecretBytes> {
        self.as_ref()
    }
}
