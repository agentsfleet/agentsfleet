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
//! 3. **The exchange last**, holding no datastore connection, because a vendor
//!    round trip must not occupy a pooled connection.
//!
//! Reversing any pair weakens it. Reading the handle before the grant leaks
//! whether an integration is connected to a fleet not permitted to use it;
//! checking the write gate after the mint means a token was minted for a reach
//! no human approved, and discarding it afterwards is not the same thing.
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
//! exists either way. `credentials_mint.zig` carries the same residual and the
//! same reasoning.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet_runtime::config::Access;
use afd_wire::credentials::MintCredentialRequest;
use serde_json::Value;

use crate::credential::broker::Ask;
use crate::credential::outcome::{Minted, Outcome};
use crate::error::{
    Result, binding_drift, connector_mint_failed, connector_reconnect_required, github_mint_failed,
    github_reconnect_required, grant_required, integration_not_connected, lease_not_found,
    mint_unconfigured, vault_data_invalid, write_spend_exhausted, write_unapproved,
};
use crate::gate::WriteApproval;
use crate::lease::pull::Plane;
use crate::lease::scope::MintScope;
use crate::secrets::connector::{Connector, Connectors as _, Exchange, Supply};
use crate::vault::KeyRef;

/// The connector whose write mints are gated on a human's answer.
///
/// Only one, and it is not an oversight: a repository write is the single
/// capability this product lets a run acquire that can change something outside
/// itself. The refresh connectors mint read-shaped API tokens, and there is no
/// per-repository reach for a card to state about them.
const GATED_WRITE_CONNECTOR: &str = "github";

/// The event a rotation write-back is logged under.
const EVENT_REFRESH_ROTATED: &str = "refresh_rotated";

impl Plane {
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
        now: UnixMillis,
    ) -> Result<Minted> {
        let scope = self
            .leases
            .mint_scope(runner_id, &request.lease_id, now)
            .await?
            .ok_or_else(lease_not_found)?;
        // Resolved once, and it answers both questions that follow: whether
        // this connector is gated at all, and which family's refusal copy the
        // runner reads. A name comparison at each site would be two places for
        // the connector set to be described.
        let connector = self.connectors.resolve(&request.integration);

        self.admit_mint(&scope, &request.integration, connector)
            .await?;

        let handle = self.handle(&scope, &request.integration).await?;
        let outcome = self
            .broker
            .mint(Ask {
                workspace_id: &scope.workspace_id,
                handle: &handle,
                binding: scope.binding.as_ref(),
                now_ms: now.as_millis(),
            })
            .await;

        let minted = accept(outcome, connector)?;
        // The exchange consumed the posted refresh token, so the replacement is
        // written back BEFORE the caller is handed anything. A failure here is
        // reported and dropped: the access token in hand is valid either way.
        self.persist_rotation(&scope, &request.integration, &handle, &minted, now)
            .await;
        Ok(minted)
    }

    /// Whether this fleet may mint this integration at all.
    ///
    /// Both gates, in the order that keeps credential bytes untouched until a
    /// human has said yes twice: once standing, for the integration, and once
    /// per event, for a repository write.
    async fn admit_mint(
        &self,
        scope: &MintScope,
        integration: &str,
        connector: Option<&dyn Connector>,
    ) -> Result<()> {
        // Gated on ON-DEMAND connectors only, which mirrors the lease
        // classifier. A `static` handle carries its own token and an unknown
        // name is not a grantable service at all, so both fall through to the
        // vault path — where they surface as "not connected" rather than as a
        // grant nobody can ever request.
        if connector.is_some_and(|connector| connector.supply() == Supply::OnDemand)
            && !self
                .gates
                .approved_integrations(&scope.fleet_id)
                .await?
                .holds(integration)
        {
            return Err(grant_required());
        }

        // A write binding spends a human's answer, and only GitHub has a
        // repository write to hold. A read binding, or none, needs no card.
        let gated_write = scope
            .binding
            .as_ref()
            .filter(|binding| binding.access() == Access::Write)
            .filter(|_| integration == GATED_WRITE_CONNECTOR);
        let Some(binding) = gated_write else {
            return Ok(());
        };
        match self
            .gates
            .reserve_write_approval(&scope.fleet_id, &scope.event_id, binding)
            .await?
        {
            WriteApproval::Approved => Ok(()),
            WriteApproval::Unapproved => Err(write_unapproved()),
            WriteApproval::BindingDrift => Err(binding_drift()),
            WriteApproval::Exhausted => Err(write_spend_exhausted()),
        }
    }

    /// The stored handle this mint exchanges.
    ///
    /// Opened only after both gates have passed. An absent row and a body that
    /// is not an object are different failures: the first is an integration
    /// nobody connected, and the second is a stored credential this daemon
    /// cannot read at all.
    async fn handle(&self, scope: &MintScope, integration: &str) -> Result<Value> {
        let held = self
            .vault
            .open(KeyRef {
                workspace_id: &scope.workspace_id,
                name: integration,
            })
            .await?
            .ok_or_else(integration_not_connected)?;
        serde_json::from_slice(held.expose()).map_err(|_shape| vault_data_invalid())
    }

    /// Writes a rotated refresh token back to the handle it came from.
    ///
    /// Best effort BY DESIGN, and the only step here that cannot fail the
    /// request: the mint already succeeded and the child holds a working token,
    /// so a failed write-back costs at most one forced reconnect when that
    /// token expires (RULE ECL). What it must never do is turn a successful
    /// mint into a refusal.
    async fn persist_rotation(
        &self,
        scope: &MintScope,
        integration: &str,
        handle: &Value,
        minted: &Minted,
        now: UnixMillis,
    ) {
        let Some(replacement) = minted.rotated_refresh_token.as_deref() else {
            return;
        };
        // What this exchange actually posted, read from the pre-exchange
        // snapshot — the guard the write-back compares the stored row against.
        let Some(posted) = handle
            .get(afd_core::credential::FIELD_REFRESH_TOKEN)
            .and_then(Value::as_str)
        else {
            return;
        };

        let key = KeyRef {
            workspace_id: &scope.workspace_id,
            name: integration,
        };
        match self
            .vault
            .rotate_refresh_token(key, posted, replacement, now)
            .await
        {
            Ok(outcome) => tracing::debug!(
                event = EVENT_REFRESH_ROTATED,
                workspace_id = scope.workspace_id.as_str(),
                integration,
                ?outcome,
                "a rotated refresh token was written back"
            ),
            // The operator's breadcrumb, and nothing more. No token bytes ride
            // this line, and the request is already successful.
            Err(failed) => tracing::warn!(
                event = EVENT_REFRESH_ROTATED,
                workspace_id = scope.workspace_id.as_str(),
                integration,
                error = %failed,
                "a rotated refresh token could not be written back"
            ),
        }
    }
}

/// The credential, or the refusal this outcome earns.
///
/// Pure, and separate from the verb for that reason: the whole outcome-to-code
/// matrix is provable here without a lease, a vault or a vendor.
///
/// `connector` selects the FAMILY's copy. GitHub keeps two codes where the
/// refresh connectors share one, which is `credentials_mint.zig`'s asymmetry
/// and is kept deliberately — a Zoho refresh that failed must never tell a
/// runner to reconnect a GitHub App.
fn accept(outcome: Outcome, connector: Option<&dyn Connector>) -> Result<Minted> {
    let is_github =
        connector.is_some_and(|connector| matches!(connector.exchange(), Exchange::GithubApp));
    match outcome {
        Outcome::Ok(minted) => Ok(minted),
        // Provider-neutral: nothing was exchanged, so there is no provider to
        // be specific about.
        Outcome::UnknownIntegration => Err(integration_not_connected()),
        Outcome::Unconfigured => Err(mint_unconfigured()),
        Outcome::ReconnectRequired if is_github => Err(github_reconnect_required()),
        Outcome::ReconnectRequired => Err(connector_reconnect_required()),
        // Both retry classes answer one code. The distinction is the broker's
        // own — whether it is worth trying again — and a runner reacts to a
        // failed mint the same way regardless of whose fault it was.
        Outcome::MintFailed(_) if is_github => Err(github_mint_failed()),
        Outcome::MintFailed(_) => Err(connector_mint_failed()),
    }
}

#[cfg(test)]
mod tests;
