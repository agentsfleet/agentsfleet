//! The seam the signed-ingress routes act through.
//!
//! One trait over all three steps — resolve, open the secret, append — because
//! they are one store and a suite that stubbed them apart would be stubbing an
//! implementation detail. The same argument [`super::event::WorkspaceEvents`]
//! makes for holding its listings and its single read together.
//!
//! # Why the whole binding crosses the seam rather than its parts
//!
//! A trait answering `workspace_of(fleet)`, `source_of(fleet)` and
//! `secret_for(workspace, key)` separately would let a handler pair a
//! workspace from one fleet with a key from another, and nothing in the types
//! would notice. [`Binding`] is resolved once and every later step takes it, so
//! the pairing is made in one place and cannot be re-made wrongly.

use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_ingress::{Appended, Binding, Delivery, Fanout, Ingress, Result as IngressResult, Surface};

/// Everything the signed-ingress routes act through.
pub trait WebhookIngress: Send + Sync + std::fmt::Debug + 'static {
    /// What this fleet's row says about receiving a signed delivery.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row this build cannot read,
    /// and a stored document that no longer parses. A fleet with no row and one
    /// with no webhook trigger are both `Ok(None)` — see
    /// [`afd_ingress::Ingress::binding`] on why they are not told apart.
    fn binding(&self, fleet: &Uuid7)
    -> impl Future<Output = IngressResult<Option<Binding>>> + Send;

    /// The shared secret this fleet's provider signs with.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open. Every way a fleet can have no usable secret is `Ok(None)`.
    fn signing_secret(
        &self,
        binding: &Binding,
    ) -> impl Future<Output = IngressResult<Option<SecretBytes>>> + Send;

    /// The App's own signing secret, held by the platform admin workspace.
    ///
    /// Takes the workspace and the key by name rather than a [`Binding`],
    /// because an App delivery has to be verified BEFORE it can be routed to
    /// the fleets it wakes — there is no binding yet to read a secret through.
    ///
    /// # Errors
    /// As [`Self::signing_secret`].
    fn platform_secret(
        &self,
        admin_workspace: &Uuid7,
        key: &str,
    ) -> impl Future<Output = IngressResult<Option<SecretBytes>>> + Send;

    /// The workspace a provider's App installation was connected to.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read. An installation with no row is `Ok(None)`.
    fn installation_workspace(
        &self,
        provider: &str,
        installation: &str,
    ) -> impl Future<Output = IngressResult<Option<Uuid7>>> + Send;

    /// The fleets that subscribed to this repository and event.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row this build cannot read,
    /// and a stored document that no longer parses.
    fn subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        repository: &str,
        event: &str,
    ) -> impl Future<Output = IngressResult<Fanout>> + Send;

    /// Appends one verified delivery, at most once however often it arrives.
    ///
    /// # Errors
    /// Reports a queue that would not take the append.
    fn deliver(
        &self,
        surface: Surface,
        binding: &Binding,
        delivery: &Delivery<'_>,
    ) -> impl Future<Output = IngressResult<Appended>> + Send;
}

/// The production ingress answers all three directly.
impl WebhookIngress for Ingress {
    fn binding(
        &self,
        fleet: &Uuid7,
    ) -> impl Future<Output = IngressResult<Option<Binding>>> + Send {
        Self::binding(self, fleet)
    }

    fn signing_secret(
        &self,
        binding: &Binding,
    ) -> impl Future<Output = IngressResult<Option<SecretBytes>>> + Send {
        Self::signing_secret(self, binding)
    }

    fn platform_secret(
        &self,
        admin_workspace: &Uuid7,
        key: &str,
    ) -> impl Future<Output = IngressResult<Option<SecretBytes>>> + Send {
        Self::platform_secret(self, admin_workspace, key)
    }

    fn installation_workspace(
        &self,
        provider: &str,
        installation: &str,
    ) -> impl Future<Output = IngressResult<Option<Uuid7>>> + Send {
        Self::installation_workspace(self, provider, installation)
    }

    fn subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        repository: &str,
        event: &str,
    ) -> impl Future<Output = IngressResult<Fanout>> + Send {
        Self::subscribers(self, workspace, provider, repository, event)
    }

    fn deliver(
        &self,
        surface: Surface,
        binding: &Binding,
        delivery: &Delivery<'_>,
    ) -> impl Future<Output = IngressResult<Appended>> + Send {
        Self::deliver(self, surface, binding, delivery)
    }
}
