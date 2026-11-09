//! The mint verb: a sandboxed child asks for a credential, mid-run.
//!
//! Every step below exists and is proven on its own — the lease scope, the
//! standing grant, the write gate, the vault, the broker. What is here is the
//! ORDER, and the order is the security property:
//!
//! 1. **The lease is resolved first**, and it is what says which workspace this
//!    is. The wire carries no workspace at all, so a prompt-injected child has
//!    nothing to forge.
//! 2. **The standing grant, then the write gate** — both before the vault is
//!    opened, so a request nobody approved never touches credential bytes.
//! 3. **The handle is held to what the workspace connected** — the name it
//!    was asked for, and for a GitHub App the installation's routing row
//!    (`claim`). The handle is bytes the workspace wrote; the row is what the
//!    connect flow wrote once the provider proved access.
//! 4. **The exchange last**, holding no datastore connection, because a vendor
//!    round trip must not occupy a pooled connection.
//!
//! Reversing any pair weakens it. Reading the handle before the grant leaks
//! whether an integration is connected to a fleet not permitted to use it;
//! checking the write gate after the mint means a token was minted for a reach
//! no human approved, and discarding it afterwards is not the same thing.
//! Exchanging before the ownership read mints another workspace's installation
//! token for whoever typed its id.
//!
//! # Every refusal is an error, not an `Ok`
//!
//! The lease verb answers its refusals as ordinary no-work replies, because a
//! polling runner's only move is to wait. This verb is the opposite: a child is
//! blocked on an answer, and each refusal has a different remedy — reconnect
//! the integration, wait for a human, re-raise a card, retry shortly. So each
//! is a typed error carrying its own registry code, exactly as `renew`'s are.
//!
//! # The residual race, stated
//!
//! The lease is validated before the exchange and the exchange holds no lock on
//! it, so a lease expiring mid-exchange can still be handed the token it asked
//! for. It is bounded to one request, and a re-check afterwards could only
//! WITHHOLD a credential the vendor has already issued — the upstream token
//! exists either way.
//!
//! The installation's routing row is read the same way: before the exchange,
//! holding no lock. A Disconnect that lands between the read and the exchange
//! still sees one token issued — the same one-request bound, and the same
//! reason a later re-check would buy nothing.

use af_core::id::Uuid7;
use af_wire::credentials::MintCredentialRequest;
use jiff::Timestamp;

use crate::error::{Result, lease_not_found};
use crate::lease::pull::LeaseService;
use afd_credential::credential::outcome::Minted;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Runner,
    TaskRead,
    TaskPublish,
}

impl LeaseService {
    /// Mints one short-lived credential for the child behind `runner_id`.
    ///
    /// # Errors
    /// Refuses a lease that is not this runner's or is no longer live, a fleet
    /// with no approved grant, a write mint with no usable approval, an
    /// integration this workspace has not connected, and every way an exchange
    /// can fail to produce a credential — each with its own registry code.
    pub async fn mint(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: Timestamp,
    ) -> Result<Minted> {
        self.mint_for(runner_id, request, now, Purpose::Runner, None)
            .await
    }

    pub(crate) async fn mint_task_publisher(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: Timestamp,
    ) -> Result<Minted> {
        self.mint_for(runner_id, request, now, Purpose::TaskPublish, None)
            .await
    }

    pub(crate) async fn mint_task_reader(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: Timestamp,
        repository: &str,
    ) -> Result<Minted> {
        self.mint_for(runner_id, request, now, Purpose::TaskRead, Some(repository))
            .await
    }

    async fn mint_for(
        &self,
        runner_id: &Uuid7,
        request: &MintCredentialRequest<'_>,
        now: Timestamp,
        purpose: Purpose,
        repository: Option<&str>,
    ) -> Result<Minted> {
        let mut scope = self
            .leases
            .mint_scope(runner_id, &crate::error::wire_lease(&request.lease_id)?)
            .await?
            .ok_or_else(lease_not_found)?;
        if purpose == Purpose::TaskPublish
            && !scope.binding.as_ref().is_some_and(|binding| {
                binding.publication() == af_fleet_runtime::config::RepositoryPublication::ExistingPr
            })
        {
            return Err(crate::task::TaskRefusal::InvalidTarget.into());
        }
        if purpose == Purpose::TaskRead {
            scope.binding = Some(
                scope
                    .binding
                    .as_ref()
                    .and_then(|binding| {
                        let repository = repository?;
                        binding.read_repository(repository)
                    })
                    .ok_or(crate::task::TaskRefusal::InvalidTarget)?,
            );
        } else if request.integration == "github"
            && purpose == Purpose::Runner
            && scope.binding.as_ref().is_some_and(|binding| {
                binding.publication() == af_fleet_runtime::config::RepositoryPublication::ExistingPr
            })
        {
            scope.binding = scope
                .binding
                .as_ref()
                .map(af_fleet_runtime::config::RepositoryBinding::read_only);
        } else {
            // Every other purpose mints over the binding exactly as stored.
        }
        self.mint_scoped(&scope, &request.integration, now, None)
            .await
    }

    pub(crate) async fn mint_wait_reader(
        &self,
        permit: &crate::task::ReadPermit,
        now: Timestamp,
    ) -> Result<Minted> {
        self.mint_scoped(permit.scope(), "github", now, Some(permit.installation()))
            .await
    }
}

mod admission;
mod claim;

#[cfg(test)]
mod tests;
