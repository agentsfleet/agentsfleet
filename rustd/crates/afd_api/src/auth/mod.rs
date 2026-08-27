//! Proving a credential for a route, and carrying the result to its handler.
//!
//! `afd_auth` owns the DECISION — which class a value belongs to, what proves
//! it, and what a refusal says. This module owns the two things that decision
//! needs from the HTTP shell: which plane a route authenticates against, and
//! how the proven principal reaches the handler.
//!
//! # The boundary `docs/AUTH.md` states, as data
//!
//! > A runner token must never satisfy a tenant route, and a user/tenant token
//! > must never satisfy a runner route.
//!
//! In the Zig daemon that is enforced by WHICH middleware is mounted where —
//! `runnerBearer` on `/v1/runners/me/*` and `bearer_or_api_key` everywhere
//! else. Mount one of them a route too wide and the boundary is gone with no
//! test failing, because nothing anywhere states the pairing.
//!
//! Here it is [`plane_of`]: a total match from [`Guard`] to
//! [`afd_auth::Plane`], read by the router while it mounts. A new guard fails
//! the BUILD until it says which plane it proves against, and a route cannot
//! be mounted under a guard it does not declare in its own [`RouteMeta`],
//! because the router reads both from the same table row.
//!
//! [`RouteMeta`]: crate::route::RouteMeta

mod guard;
mod identity;

use afd_auth::authenticate::Registry;
use afd_auth::capability::CapabilitySource;
use afd_auth::directory::CredentialDirectory;
use afd_auth::principal::Principal;
use afd_auth::verifier::TokenVerifier;
use afd_auth::{Error, Plane};

use crate::route::Guard;

pub use self::guard::{Gate, prove};
pub use self::identity::{PersonIdentity, RunnerIdentity};

/// What proves a credential presented on a plane.
///
/// One method, and the plane is an ARGUMENT rather than a type parameter: the
/// router mounts routes of both planes from one table walk, so a state value
/// that could only answer for one of them would have to be held twice.
///
/// # Errors
/// [`Error`], carrying the refusing plane's own code — a tenant credential on
/// the runner plane answers `UZ-RUN-001`, not `UZ-AUTH-002`, because the runner
/// client classifies its own plane's codes and has no branch for the other's.
pub trait Authenticator: Send + Sync + std::fmt::Debug + 'static {
    /// Proves `header` for `plane`.
    ///
    /// An absent header is refused exactly as a wrong-class credential is, so a
    /// caller cannot tell "you sent nothing" apart from "you sent the wrong
    /// kind of thing".
    ///
    /// # Errors
    /// The plane's refusal, or the class's own — see the trait documentation.
    fn authenticate(
        &self,
        plane: Plane,
        header: Option<&str>,
    ) -> impl Future<Output = Result<Principal, Error>> + Send;
}

/// One registry per plane, over one directory, capability source and verifier.
///
/// A [`Registry`] is built AROUND a plane — it refuses a class the plane does
/// not admit before any lookup happens — so serving both planes means holding
/// two. They share their seams by cloning: a `Registry` is three handles and a
/// plane tag, and each seam is documented as cheap to clone
/// (`M-SERVICES-CLONE`), so the pair costs a pointer copy rather than a second
/// pool or a second key cache.
#[derive(Debug, Clone)]
pub struct Planes<D, C, V = afd_auth::verifier::NoVerifier> {
    tenant: Registry<D, C, V>,
    runner: Registry<D, C, V>,
}

impl<D, C, V> Planes<D, C, V>
where
    D: CredentialDirectory + Clone,
    C: CapabilitySource + Clone,
    V: TokenVerifier + Clone,
{
    /// Builds both planes over one set of seams.
    pub fn new(directory: D, capabilities: C, verifier: V) -> Self {
        Self {
            tenant: Registry::new(
                Plane::Tenant,
                directory.clone(),
                capabilities.clone(),
                verifier.clone(),
            ),
            runner: Registry::new(Plane::Runner, directory, capabilities, verifier),
        }
    }

    /// The registry `plane` authenticates through.
    ///
    /// Exhaustive, so a new plane fails the build here rather than resolving to
    /// whichever registry happened to be first.
    const fn of(&self, plane: Plane) -> &Registry<D, C, V> {
        match plane {
            Plane::Tenant => &self.tenant,
            Plane::Runner => &self.runner,
        }
    }
}

impl<D, C, V> Authenticator for Planes<D, C, V>
where
    D: CredentialDirectory + Clone + 'static,
    C: CapabilitySource + Clone + 'static,
    V: TokenVerifier + Clone + 'static,
{
    async fn authenticate(&self, plane: Plane, header: Option<&str>) -> Result<Principal, Error> {
        match header {
            Some(value) => self.of(plane).authenticate_header(value).await,
            // The same refusal a wrong-class credential gets. `bearer.zig`
            // makes the same collapse, and it is the one an unauthenticated
            // caller must not be able to distinguish.
            None => Err(plane.refusal()),
        }
    }
}

/// The credential plane a guard proves against, or `None` when the route is
/// authenticated by its payload rather than by a bearer.
///
/// The total match the wiring convention was standing in for. A signed webhook
/// delivery carries its proof in the body and its own signature header, so
/// there is no bearer to classify and no plane to refuse it on — that is an
/// absence with a reason, not a gap.
#[must_use]
pub const fn plane_of(guard: Guard) -> Option<Plane> {
    match guard {
        Guard::Bearer => Some(Plane::Tenant),
        Guard::RunnerBearer => Some(Plane::Runner),
        // No bearer to classify, for two different reasons that reach the same
        // answer. An open route is a probe or a payload-authenticated flow and
        // presents no credential at all; a signed delivery carries its proof in
        // the body and its own signature header, which this layer is the wrong
        // place to verify. Both are an absence with a reason, not a gap.
        Guard::Open | Guard::WebhookHmac | Guard::WebhookSignature | Guard::Svix => None,
    }
}
