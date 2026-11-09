//! Credential admission precedes handle access and exchange.

use af_core::error_code::{Coded as _, Logged};
use afd_credential::credential::broker::Ask;
use afd_credential::credential::outcome::{Minted, Outcome};
use afd_credential::secrets::connector::{Connector, Exchange, Supply};
use afd_credential::vault::KeyRef;
use jiff::Timestamp;
use serde_json::Value;

use super::claim::{Claim, claim};
use crate::error::{
    Result, connector_mint_failed, connector_reconnect_required, github_mint_failed,
    github_reconnect_required, grant_required, integration_not_connected, mint_unconfigured,
    vault_data_invalid,
};
use crate::lease::{pull::LeaseService, scope::MintScope};

pub(super) const EVENT_REFRESH_ROTATED: &str = "refresh_rotated";
const EVENT_HANDLE_REFUSED: &str = "credential_handle_refused";

impl LeaseService {
    pub(super) async fn mint_scoped(
        &self,
        scope: &MintScope,
        integration: &str,
        now: Timestamp,
        expected_installation: Option<u64>,
    ) -> Result<Minted> {
        // Resolved once, and it answers both questions that follow: whether
        // this connector is gated at all, and which family's refusal copy the
        // runner reads. A name comparison at each site would be two places for
        // the connector set to be described.
        let connector = self.broker.connectors().resolve(integration);

        self.admit_mint(scope, integration, connector).await?;

        let handle = self.handle(scope, integration).await?;
        if expected_installation.is_some_and(|expected| {
            afd_credential::credential::github::installation(&handle) != Some(expected)
        }) {
            return Err(crate::task::TaskRefusal::InvalidTarget.into());
        }
        self.admit_handle(scope, integration, &handle).await?;
        let outcome = self
            .broker
            .mint(Ask {
                workspace_id: &scope.workspace_id,
                handle: &handle,
                binding: scope.binding.as_ref(),
                now,
            })
            .await;

        let minted = accept(outcome, connector)?;
        // The exchange consumed the posted refresh token, so the replacement is
        // written back BEFORE the caller is handed anything. A failure here is
        // reported and dropped: the access token in hand is valid either way.
        self.persist_rotation(scope, integration, &handle, &minted)
            .await;
        Ok(minted)
    }

    /// Whether this fleet may mint this integration at all.
    ///
    /// ONE question, and the grant is it: an approved, unrevoked standing
    /// decision for this fleet and this service. The per-event repository-write
    /// gate that used to sit above this read asked a person again on every
    /// model turn, which bought nothing the grant did not already carry — the
    /// grant names the fleet, the fleet's binding names the repositories and
    /// the access level, and `ScopedRequest::for_binding` narrows the token to
    /// exactly those. What bounds the damage is the App installation, the
    /// fleet's `budget.daily_nanos`, and `agentsfleet grant delete`.
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

        Ok(())
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

    /// Whether this workspace may spend what its stored handle names.
    ///
    /// After the vault read, because the claim is IN the handle; before the
    /// exchange, because a token minted for an installation this workspace
    /// never connected cannot be un-minted. A miss answers the same reconnect
    /// a handle naming no installation does, so the refusal says nothing about
    /// whether some other workspace holds that installation.
    async fn admit_handle(
        &self,
        scope: &MintScope,
        integration: &str,
        handle: &Value,
    ) -> Result<()> {
        let held = match claim(self.broker.connectors(), integration, handle) {
            Ok(Claim::Unowned) => true,
            Ok(Claim::Installation { provider, id }) => {
                self.leases
                    .holds_install(&scope.workspace_id, provider.id(), &id.to_string())
                    .await?
            }
            Err(refusal) => return Err(refused(scope, integration, refusal)),
        };
        if held {
            Ok(())
        } else {
            Err(refused(scope, integration, github_reconnect_required()))
        }
    }

    /// Writes a rotated refresh token back to the handle it came from.
    ///
    /// Best effort BY DESIGN, and the only step here that cannot fail the
    /// request: the mint already succeeded and the child holds a working token,
    /// so a failed write-back costs at most one forced reconnect when that
    /// token expires (RULE ECL). What it must never do is turn a successful
    /// mint into a refusal.
    pub(super) async fn persist_rotation(
        &self,
        scope: &MintScope,
        integration: &str,
        handle: &Value,
        minted: &Minted,
    ) {
        let Some(replacement) = minted.rotated_refresh_token.as_deref() else {
            return;
        };
        // What this exchange actually posted, read from the pre-exchange
        // snapshot — the guard the write-back compares the stored row against.
        let Some(posted) = handle
            .get(af_core::credential::FIELD_REFRESH_TOKEN)
            .and_then(Value::as_str)
        else {
            return;
        };

        let key = KeyRef {
            workspace_id: &scope.workspace_id,
            name: integration,
        };
        let workspace_id = scope.workspace_id.as_str();
        match self
            .vault
            .rotate_refresh_token(key, posted, replacement)
            .await
        {
            Ok(outcome) => tracing::debug!(
                event = EVENT_REFRESH_ROTATED,
                workspace_id,
                integration,
                ?outcome,
                "a rotated refresh token was written back"
            ),
            // The operator's breadcrumb, and nothing more. No token bytes ride
            // this line, and the request is already successful.
            Err(failed) => {
                let Logged { error_code, reason } = failed.logged();
                tracing::warn!(
                    event = EVENT_REFRESH_ROTATED,
                    workspace_id,
                    integration,
                    error_code,
                    reason,
                    "a rotated refresh token could not be written back"
                );
            }
        }
    }
}

/// Records a stored handle the mint admission refused, and hands it back.
///
/// The same refusal serves a forged handle and one naming a connection the
/// workspace no longer holds, so the runner learns nothing from it; this line
/// gives an operator the workspace and the integration to look at, never a
/// byte of the handle.
fn refused(scope: &MintScope, integration: &str, refusal: crate::Error) -> crate::Error {
    let workspace_id = scope.workspace_id.as_str();
    let error_code = refusal.code().as_str();
    tracing::warn!(
        event = EVENT_HANDLE_REFUSED,
        error_code,
        workspace_id,
        integration,
        "a stored handle names a connection its workspace does not hold"
    );
    refusal
}

/// The credential, or the refusal this outcome earns.
///
/// Pure, and separate from the verb for that reason: the whole outcome-to-code
/// matrix is provable here without a lease, a vault or a vendor.
///
/// `connector` selects the FAMILY's copy. GitHub keeps two codes where the
/// refresh connectors share one, deliberately — a Zoho refresh that failed
/// must never tell a runner to reconnect a GitHub App.
pub(super) fn accept(outcome: Outcome, connector: Option<&dyn Connector>) -> Result<Minted> {
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
