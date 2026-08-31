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

    /// The Svix `whsec_…` this fleet's trigger names.
    ///
    /// A separate reader from [`Self::signing_secret`] because it is a separate
    /// stored SHAPE, not a separate policy: the HMAC family stores a JSON
    /// object and reads one field out of it, and Svix stores the raw string.
    /// Two readers over one vault, which is what keeps the parsing in the crate
    /// that knows the shape rather than in the route that wanted a key.
    ///
    /// # Errors
    /// As [`Self::signing_secret`].
    fn svix_secret(
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

    fn svix_secret(
        &self,
        binding: &Binding,
    ) -> impl Future<Output = IngressResult<Option<SecretBytes>>> + Send {
        Self::svix_secret(self, binding)
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

/// The vault key the approval signing secret is stored under.
///
/// One deployment-level HMAC key serves two surfaces — the approval callback
/// verifies against it and the connector install state is signed with it — so
/// the name lives beside [`WebhookIngress::platform_secret`], which is what
/// reads it. A second spelling would be a second thing to rotate.
pub const APPROVAL_IDENTITY: &str = "approval-signing";

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
    )]

    use std::sync::Arc;

    use afd_core::env::MapEnv;
    use afd_crypto::entropy::Entropy;
    use afd_crypto::secret::Kek;
    use afd_db::{Db, DbRole, PoolConfig};
    use afd_ingress::{Binding, Delivery, Ingress, Surface};
    use afd_redis::{Redis, RedisConfig, RedisRole};
    use afd_vault::Vault;

    /// A Postgres nobody is listening on. Port 1 is reserved and unbound, so an
    /// acquire fails on connection REFUSAL rather than waiting out a timeout.
    const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

    /// The same, for the queue.
    const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

    /// The knob bounding how long an acquire may spend before it reports.
    const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

    /// Long enough that a refused connect is classified as UNAVAILABLE rather
    /// than as pool capacity, short enough that seven of them cost nothing.
    const ACQUIRE_TIMEOUT_MS: &str = "50";

    /// The production ingress, over three handles that answer nothing.
    fn refusing() -> Ingress {
        let environment = MapEnv::from_pairs([
            (DbRole::Api.url_knob(), NOWHERE),
            (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
        ]);
        let pool = PoolConfig::resolve(&environment, DbRole::Api)
            .expect("the fixture connection string is well formed");
        let database = Db::unreachable(&pool);
        let queue = Redis::unreachable(
            &RedisConfig::from_url(RedisRole::Default, NOWHERE_QUEUE.to_owned())
                .with_request_timeout(std::time::Duration::from_millis(250)),
        )
        .expect("a lazy manager opens no socket, so it cannot fail to open one");
        let vault = Vault::new(
            database.clone(),
            Arc::new(Kek::from_bytes([7u8; 32])),
            Entropy::new(),
        );
        Ingress::new(database, vault, queue)
    }

    /// Whether a reader refused.
    ///
    /// A function rather than `assert!(… .is_err())` at each call site: the
    /// manifest denies `assertions_on_result_states`, and its suggested
    /// `unwrap_err` is denied too. Asking the question once keeps both
    /// satisfied and reads better than either.
    const fn refused<T, E>(answer: &Result<T, E>) -> bool {
        answer.is_err()
    }

    /// A binding to hand the readers that take one.
    fn binding() -> Binding {
        Binding::stored(
            afd_core::id::Uuid7::parse("019329c5-0000-7000-8000-0000000000c1")
                .expect("the fixture fleet is canonical"),
            afd_core::id::Uuid7::parse("019329c5-0000-7000-8000-0000000000c2")
                .expect("the fixture workspace is canonical"),
            "active",
            r#"{"name":"adapter","x-agentsfleet":{"triggers":[{"type":"webhook","source":"github"}],"tools":["bash"],"budget":{"daily_dollars":1.0}}}"#,
            None,
        )
        .expect("the fixture document parses")
        .expect("the fixture document declares a webhook trigger")
    }

    /// Every method on the seam reaches the store it names.
    ///
    /// Seven one-line delegations, and the reason they are worth a test is that
    /// the compiler cannot tell them apart: `svix_secret` forwarding to
    /// `signing_secret` type-checks perfectly and silently verifies a Svix
    /// delivery against the HMAC family's field — a security boundary crossed
    /// by a copied line. Reaching a refusing store proves each one arrives
    /// somewhere rather than at its neighbour.
    ///
    /// The router suites cannot cover this: they substitute a stub FOR the
    /// trait, so the production impl below is reached by the daemon and by
    /// nothing else.
    #[tokio::test]
    async fn every_reader_on_the_seam_reaches_a_store() {
        let ingress = refusing();
        let fleet = afd_core::id::Uuid7::parse("019329c5-0000-7000-8000-0000000000c1")
            .expect("the fixture fleet is canonical");
        let held = binding();

        assert!(refused(&WebhookIngress::binding(&ingress, &fleet).await));
        assert!(refused(
            &WebhookIngress::signing_secret(&ingress, &held).await
        ));
        // The one reader that answers WITHOUT a store, and the asymmetry is the
        // point: a trigger declaring no Svix ref has no Svix secret, which is a
        // configuration fact rather than a failure. Reaching the vault to
        // discover it would make an outage and an unconfigured fleet the same
        // answer, and only one of them is worth waking somebody for.
        assert!(
            matches!(WebhookIngress::svix_secret(&ingress, &held).await, Ok(None)),
            "a trigger with no signature ref short-circuits before the vault"
        );
        assert!(refused(
            &WebhookIngress::platform_secret(&ingress, &fleet, "github-app").await
        ));
        assert!(refused(
            &WebhookIngress::installation_workspace(&ingress, "github", "1").await
        ));
        assert!(refused(
            &WebhookIngress::subscribers(&ingress, &fleet, "github", "o/r", "push").await
        ));
        assert!(refused(
            &WebhookIngress::deliver(
                &ingress,
                Surface::Fleet,
                &held,
                &Delivery {
                    event_id: "adapter",
                    actor: "webhook:github",
                    request_json: "{}",
                },
            )
            .await
        ));
    }

    use super::WebhookIngress;
}
