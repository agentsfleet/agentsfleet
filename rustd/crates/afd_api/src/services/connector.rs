//! The seam the connector routes act through.
//!
//! One trait over the whole connector surface, because a handler that held
//! only half of it could not finish a connect: the four round-trip steps and
//! the three reads act through the same vaults, and splitting them would mean
//! two stubs in every suite that arranges either. The same argument
//! [`super::schedule::FleetSchedules`] makes for holding its CRUD and its fire
//! path together.
//!
//! # The ordering crosses the seam as types, not as a comment
//!
//! [`Verified`] is produced only by [`WorkspaceConnectors::verify`], [`Spent`]
//! only by [`WorkspaceConnectors::spend`], and
//! [`WorkspaceConnectors::finish`] takes a [`Spent`]. So a handler cannot
//! redeem a code for a state it has not spent, and cannot spend one it has not
//! verified — and neither can a stub, which is what keeps a suite from proving
//! an order production does not run. `callback.zig` reaches the same ordering
//! by writing the steps in one function and trusting nobody reorders them.
//!
//! # Why `verify` is the one step that is not a future
//!
//! It touches no store: a signature check and a window check over bytes the
//! caller already holds. Keeping it synchronous is what makes a replayed
//! callback cost a hash rather than a round trip, and the signature is where
//! that is stated.

use afd_connector::{
    Catalogued, Connection, Connectors, Finishing, Forgotten, Landed, Provider, Rejected,
    Result as ConnectorResult, Spent, Started, Starting, Verified,
};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;

/// Everything the connector routes act through.
pub trait WorkspaceConnectors: Send + Sync + std::fmt::Debug + 'static {
    /// Starts a connect: a remembered nonce, a signed state, a consent URL.
    ///
    /// # Errors
    /// Reports a host short of entropy, a store that would not remember the
    /// nonce, and a vault that would not answer for this deployment's app. A
    /// provider this deployment has configured no app for is
    /// [`Started::NotConfigured`], not an error.
    fn start(
        &self,
        starting: Starting<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = ConnectorResult<Started>> + Send;

    /// Verifies a presented state and that this is the person who started it.
    ///
    /// # Errors
    /// [`Rejected`], which the caller logs by [`Rejected::reason`] and answers
    /// as one code — see [`afd_core::error_code::CONNECTOR_STATE_INVALID`].
    fn verify(
        &self,
        provider: Provider,
        secret: &SecretBytes,
        presented: &str,
        subject: &str,
        now: UnixMillis,
    ) -> Result<Verified, Rejected>;

    /// Spends this round-trip's single-use slot, once.
    ///
    /// `Ok(None)` for a slot already spent or expired, which the caller answers
    /// exactly as it answers a forged state.
    ///
    /// # Errors
    /// Reports a store that would not answer, and a workspace the state names
    /// that is not a canonical identifier.
    fn spend(
        &self,
        provider: Provider,
        verified: &Verified,
    ) -> impl Future<Output = ConnectorResult<Option<Spent>>> + Send;

    /// Redeems the code and lands the grant in the workspace the state named.
    ///
    /// # Errors
    /// Reports a provider that could not be reached, one that answered and
    /// refused, an answer carrying no readable grant, and a vault or datastore
    /// that would not take the write.
    fn finish(
        &self,
        finishing: Finishing<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = ConnectorResult<Landed>> + Send;

    /// This workspace's connection to `provider`, or nothing.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open. Every shape that is not a landed grant is `Ok(None)`.
    fn connection(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> impl Future<Output = ConnectorResult<Option<Connection>>> + Send;

    /// Every provider, with what this deployment and this workspace hold.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn catalogue(
        &self,
        admin: Option<&Uuid7>,
        workspace: &Uuid7,
    ) -> impl Future<Output = ConnectorResult<Vec<Catalogued>>> + Send;

    /// Forgets this workspace's connection to `provider`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a vault that refused the
    /// delete. A workspace holding no handle is [`Forgotten::AlreadyAbsent`].
    fn forget(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> impl Future<Output = ConnectorResult<Forgotten>> + Send;
}

/// The production flow answers all seven directly.
impl WorkspaceConnectors for Connectors {
    fn start(
        &self,
        starting: Starting<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = ConnectorResult<Started>> + Send {
        Self::start(self, starting, now)
    }

    fn verify(
        &self,
        provider: Provider,
        secret: &SecretBytes,
        presented: &str,
        subject: &str,
        now: UnixMillis,
    ) -> Result<Verified, Rejected> {
        Self::verify(self, provider, secret, presented, subject, now)
    }

    fn spend(
        &self,
        provider: Provider,
        verified: &Verified,
    ) -> impl Future<Output = ConnectorResult<Option<Spent>>> + Send {
        Self::spend(self, provider, verified)
    }

    fn finish(
        &self,
        finishing: Finishing<'_>,
        now: UnixMillis,
    ) -> impl Future<Output = ConnectorResult<Landed>> + Send {
        Self::finish(self, finishing, now)
    }

    fn connection(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> impl Future<Output = ConnectorResult<Option<Connection>>> + Send {
        Self::connection(self, workspace, provider)
    }

    fn catalogue(
        &self,
        admin: Option<&Uuid7>,
        workspace: &Uuid7,
    ) -> impl Future<Output = ConnectorResult<Vec<Catalogued>>> + Send {
        Self::catalogue(self, admin, workspace)
    }

    fn forget(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> impl Future<Output = ConnectorResult<Forgotten>> + Send {
        Self::forget(self, workspace, provider)
    }
}
