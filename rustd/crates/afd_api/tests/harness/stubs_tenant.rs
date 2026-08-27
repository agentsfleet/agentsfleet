//! The one seam a suite answers HONESTLY rather than refusing through.
//!
//! Everything else this file used to hold — six stubs whose every method
//! returned `Err(Error::datastore_unavailable())` — is gone. The harness builds
//! the real stores over datastores that answer nothing, so the refusal now
//! comes from the crate that owns it rather than from a copy kept here (see
//! [`super`]).
//!
//! What remains cannot be replaced that way, and the reason is the test it
//! serves: the ownership layer is the thing UNDER test in the router's refusal
//! matrix, so it has to answer. A real resolver over a dead pool would refuse,
//! and then every workspace route would be unreachable for the wrong reason and
//! the matrix would prove nothing.

use afd_api::services::WorkspaceOwnership;
use afd_core::id::Uuid7;

/// The identifier of the one workspace [`OneWorkspace`] answers for.
///
/// A constant rather than a fixture, so a suite asserting the DENIED half can
/// name a workspace it knows is foreign without coordinating with the allow
/// half. Any other well-formed identifier is somebody else's.
pub(crate) const OWNED_WORKSPACE: &str = "01924f4e-0000-7000-8000-00000000beef";

/// The deployment every fixture credential records.
pub(crate) const DEPLOYMENT: &str = "https://api.fixture.test";

/// A workspace-ownership resolver that owns exactly one workspace.
///
/// Answers honestly rather than uniformly, and it has to: a resolver that
/// allowed everything would make the deny path unreachable, and one that denied
/// everything would make every workspace handler unreachable. Owning one and
/// refusing the rest gives a suite both halves with no Postgres in it.
///
/// This is the line between a stub worth keeping and the eight that were
/// deleted. Those encoded no decision — they answered one error whatever they
/// were asked, which is exactly what a real store over a dead pool does, only
/// with the error invented instead of raised. This encodes a DECISION, and no
/// datastore state can stand in for it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OneWorkspace;

impl WorkspaceOwnership for OneWorkspace {
    fn authorize(
        &self,
        principal: &afd_auth::principal::Principal,
        workspace: &Uuid7,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        // A runner has no tenant authority, exactly as in production: the
        // statement binds nothing that could match, so the answer is a denial
        // rather than an error.
        let tenant = principal.tenant().cloned();
        let owned = workspace.as_str() == OWNED_WORKSPACE;
        std::future::ready(Ok(tenant.filter(|_| owned)))
    }

    fn tenant_of(
        &self,
        principal: &afd_auth::principal::Principal,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        std::future::ready(Ok(principal.tenant().cloned()))
    }
}
