//! An ingress that ANSWERS, beside the production one that cannot.
//!
//! # Why this stub exists when [`super`] argues against stubs
//!
//! That header's rule is that a store which INVENTS a refusal keeps agreeing
//! with the suite after the real store stops producing it, so every seam holds
//! the production store over a datastore that answers nothing. The rule holds,
//! and this does not break it: an [`Ingress`] over a dead pool refuses at its
//! first acquire, which proves the refusal matrix in front of the signed routes
//! and NOTHING on the far side of it. Every ordering decision the ingress
//! handlers own — what is refused before the body is hashed, which trigger a
//! delivery is measured on, what identifier a redelivery carries — lives past
//! that acquire and is unreachable without a seam that says yes.
//!
//! So this is the same shape [`super::Directory`] already is: one enum, two
//! arms, the production store in one of them. A suite picks the arm its
//! dimension needs and the router is the real router either way.
//!
//! # What the scripted arm proves, and what it deliberately does not
//!
//! It proves what the HANDLER did: the order it refused in, the values it
//! passed across the seam, the response it rendered. [`Scripted::deliveries`]
//! is the record of that — a test asserts on the `event_id` the route composed,
//! which is the route's own output.
//!
//! It does not prove the at-most-once claim. That is one Lua script on Redis
//! ([`afd_redis::streams::FleetStreams::append_once`]), and a claim
//! re-implemented here would agree with the suite whatever the script did. The
//! claim below exists only so a redelivery reaches the route's replay-rendering
//! branch; the guarantee itself is the integration lane's, under `#[ignore]`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use afd_api::services::WebhookIngress;
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_ingress::{Appended, Binding, Delivery, Fanout, Ingress, Result as IngressResult, Surface};
use afd_redis::streams::EventId;

/// The shape Redis renders an entry id in, which a stub id has to share.
///
/// A route reads the id back out and puts it in a response body, so an id of
/// another shape would let a renderer that mangled it still pass.
const ENTRY_ID_SEQUENCE: &str = "-0";

/// Which ingress a suite is driving.
///
/// [`super::Directory`]'s shape, one seam over. The `Unreachable` arm is the
/// default and is the production [`Ingress`] over a pool that answers nothing;
/// `Scripted` is chosen per test by [`super::Fleet::with_ingress`].
#[derive(Debug)]
pub(crate) enum HarnessIngress {
    /// The production store, over a datastore that is not there.
    Unreachable(Box<Ingress>),
    /// A scripted store, for the decisions that live past the first acquire.
    ///
    /// Shared rather than owned: a test arranges the answers, hands the router
    /// a handle, and then asks the SAME value what the route did with them.
    /// An owned stub would be readable only by moving it back out of a router
    /// the request is still borrowing.
    Scripted(Arc<Scripted>),
}

/// One append, as the route asked for it.
///
/// Every field is the HANDLER's, which is what makes asserting on them worth
/// doing: the surface it chose, the fleet it resolved, the identifier it
/// composed, the actor it recorded, and the digest it built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Recorded {
    /// Which claim window the route asked for.
    pub(crate) surface: Surface,
    /// The fleet the append was made against.
    pub(crate) fleet: String,
    /// The identifier a redelivery has to repeat.
    pub(crate) event_id: String,
    /// Who the history records as having woken the fleet.
    pub(crate) actor: String,
    /// The digest the fleet's prose reasons over.
    pub(crate) request_json: String,
}

/// An ingress whose answers a test writes.
///
/// Every resolution field is what the corresponding production read WOULD have
/// answered, so a test says "this fleet has no secret" rather than "this call
/// fails" — the difference between arranging a state and arranging an outcome.
#[derive(Debug, Default)]
pub(crate) struct Scripted {
    /// What [`WebhookIngress::binding`] answers.
    binding: Option<Binding>,
    /// What [`WebhookIngress::signing_secret`] answers.
    secret: Option<SecretBytes>,
    /// What [`WebhookIngress::platform_secret`] answers.
    platform_secret: Option<SecretBytes>,
    /// What [`WebhookIngress::installation_workspace`] answers.
    installation: Option<Uuid7>,
    /// The fleets [`WebhookIngress::subscribers`] fans out to.
    subscribers: Vec<Binding>,
    /// A matched count above the ceiling, when the test is about the ceiling.
    ///
    /// Separate from [`Self::subscribers`] because `Fanout::TooMany` carries a
    /// COUNT and no bindings — building a hundred and one real bindings to
    /// reach a branch that discards them would be arranging the wrong thing.
    over_ceiling: Option<usize>,
    /// Every append, in the order the route made them.
    log: Mutex<Vec<Recorded>>,
    /// The first id each claim key was answered with.
    claimed: Mutex<BTreeMap<String, String>>,
}

impl Scripted {
    /// A store that resolves nothing, for a test that arranges only what it
    /// needs.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The binding a per-fleet delivery resolves to.
    #[must_use]
    pub(crate) fn resolving(mut self, binding: Binding) -> Self {
        self.binding = Some(binding);
        self
    }

    /// The secret a per-fleet delivery is verified against.
    #[must_use]
    pub(crate) fn signing(mut self, secret: &[u8]) -> Self {
        self.secret = Some(SecretBytes::new(secret.to_vec()));
        self
    }

    /// The deployment's own App secret.
    #[must_use]
    pub(crate) fn app_signing(mut self, secret: &[u8]) -> Self {
        self.platform_secret = Some(SecretBytes::new(secret.to_vec()));
        self
    }

    /// The workspace an App installation resolves to.
    #[must_use]
    pub(crate) fn installed_in(mut self, workspace: Uuid7) -> Self {
        self.installation = Some(workspace);
        self
    }

    /// The fleets an App delivery fans out to.
    #[must_use]
    pub(crate) fn subscribed(mut self, fleets: Vec<Binding>) -> Self {
        self.subscribers = fleets;
        self
    }

    /// A matched set larger than one delivery may wake.
    #[must_use]
    pub(crate) const fn matching(mut self, count: usize) -> Self {
        self.over_ceiling = Some(count);
        self
    }

    /// Every append this store was asked for, in order.
    pub(crate) fn deliveries(&self) -> Vec<Recorded> {
        self.log
            .lock()
            .expect("no test holds this lock across a panic")
            .clone()
    }
}

impl WebhookIngress for HarnessIngress {
    async fn binding(&self, fleet: &Uuid7) -> IngressResult<Option<Binding>> {
        match self {
            Self::Unreachable(ingress) => ingress.binding(fleet).await,
            Self::Scripted(scripted) => Ok(scripted.binding.clone()),
        }
    }

    async fn signing_secret(&self, binding: &Binding) -> IngressResult<Option<SecretBytes>> {
        match self {
            Self::Unreachable(ingress) => ingress.signing_secret(binding).await,
            Self::Scripted(scripted) => Ok(scripted.secret.clone()),
        }
    }

    async fn platform_secret(
        &self,
        admin_workspace: &Uuid7,
        key: &str,
    ) -> IngressResult<Option<SecretBytes>> {
        match self {
            Self::Unreachable(ingress) => ingress.platform_secret(admin_workspace, key).await,
            Self::Scripted(scripted) => Ok(scripted.platform_secret.clone()),
        }
    }

    async fn installation_workspace(
        &self,
        provider: &str,
        installation: &str,
    ) -> IngressResult<Option<Uuid7>> {
        match self {
            Self::Unreachable(ingress) => {
                ingress.installation_workspace(provider, installation).await
            }
            Self::Scripted(scripted) => Ok(scripted.installation.clone()),
        }
    }

    async fn subscribers(
        &self,
        workspace: &Uuid7,
        provider: &str,
        repository: &str,
        event: &str,
    ) -> IngressResult<Fanout> {
        match self {
            Self::Unreachable(ingress) => {
                ingress
                    .subscribers(workspace, provider, repository, event)
                    .await
            }
            Self::Scripted(scripted) => Ok(scripted.fanout()),
        }
    }

    async fn deliver(
        &self,
        surface: Surface,
        binding: &Binding,
        delivery: &Delivery<'_>,
    ) -> IngressResult<Appended> {
        match self {
            Self::Unreachable(ingress) => ingress.deliver(surface, binding, delivery).await,
            Self::Scripted(scripted) => Ok(scripted.append(surface, binding, delivery)),
        }
    }
}

impl Scripted {
    /// The three answers a route acts on, from what this store was told.
    ///
    /// The ceiling wins over the subscriber list when both are arranged, which
    /// is the only order that makes sense: a test setting a matched count is
    /// asking for the refusal, and a list beside it is the fleets that count
    /// stands for.
    fn fanout(&self) -> Fanout {
        match (self.over_ceiling, self.subscribers.is_empty()) {
            (Some(count), _) => Fanout::TooMany(count),
            (None, true) => Fanout::Nobody,
            (None, false) => Fanout::To(self.subscribers.clone()),
        }
    }

    /// Records one append and answers it, repeating the first id for a repeat.
    ///
    /// The claim key is [`afd_ingress`]'s own — `{fleet}:{provider event id}` —
    /// composed here rather than imported because the production one is built
    /// inside `deliver` and never crosses a seam. That duplication is the
    /// reason this cannot stand in for the script: the two could drift, and
    /// only the integration lane would notice.
    fn append(&self, surface: Surface, binding: &Binding, delivery: &Delivery<'_>) -> Appended {
        let fleet = binding.fleet().as_str().to_owned();
        let key = format!("{fleet}:{}", delivery.event_id);

        let mut log = self
            .log
            .lock()
            .expect("no test holds this lock across a panic");
        let mut claimed = self
            .claimed
            .lock()
            .expect("no test holds this lock across a panic");

        log.push(Recorded {
            surface,
            fleet,
            event_id: delivery.event_id.to_owned(),
            actor: delivery.actor.to_owned(),
            request_json: delivery.request_json.to_owned(),
        });

        let next = format!("{}{ENTRY_ID_SEQUENCE}", log.len());
        let replayed = claimed.contains_key(&key);
        let id = claimed.entry(key).or_insert(next).clone();

        Appended {
            id: EventId::of(&id),
            replayed,
        }
    }
}
