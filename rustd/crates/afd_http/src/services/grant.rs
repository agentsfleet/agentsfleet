//! The HTTP seam the integration-grant surface acts through.
//!
//! One trait over the list and the revoke, because they are one store and a
//! suite that stubbed them apart would be stubbing an implementation detail.
//!
//! # Both verbs answer "whose fleet is this" in their own return type
//!
//! The ownership LAYER decides whether the caller may act in the workspace. It
//! cannot decide whether the fleet in the path belongs to that workspace, which
//! is a row-level fact — so the store answers it, and both signatures carry the
//! answer rather than collapsing it into an empty list or a bare `false`. That
//! keeps "this workspace holds no such fleet" and "this fleet holds no grants"
//! distinguishable at the edge, where they are two different refusals.

use afd_approval::{GrantRow, IntegrationGrants, Result as ApprovalResult, Revocation};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

/// Everything the integration-grant routes act through.
pub trait FleetGrants: Send + Sync + std::fmt::Debug + 'static {
    /// Every grant `fleet` holds, newest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A fleet the workspace does
    /// not hold is `Ok(None)`, which is not the same as an empty list.
    fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = ApprovalResult<Option<Vec<GrantRow>>>> + Send;

    /// Takes one grant back.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. Every refusal a caller can
    /// cause is a [`Revocation`] arm rather than an error.
    fn revoke(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        grant: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = ApprovalResult<Revocation>> + Send;
}

/// The production store answers both directly.
impl FleetGrants for IntegrationGrants {
    fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> impl Future<Output = ApprovalResult<Option<Vec<GrantRow>>>> + Send {
        Self::page(self, workspace, fleet)
    }

    fn revoke(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        grant: &Uuid7,
        now: UnixMillis,
    ) -> impl Future<Output = ApprovalResult<Revocation>> + Send {
        Self::revoke(self, workspace, fleet, grant, now)
    }
}
