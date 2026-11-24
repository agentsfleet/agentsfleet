//! A run asking for one secret or integration: `POST L/access {name, scope?}`.
//!
//! The run must be live and hold its fleet at the fence it presents. The name
//! is a workspace vault entry; its stored shape decides the grant's kind, as
//! the run's policy routes it (`afd_credential::secrets`): an on-demand handle
//! is an integration, anything else a secret. A GitHub request names the scope
//! it wants, read by the one scope parser. A request the approved grant already
//! covers answers at once; any other sets the grant pending with the asked
//! scope and raises the card a person answers, carrying the run's turn, and the
//! run then ends waiting for that answer.

use af_core::id::Uuid7;
use af_fleet_runtime::config::{Reach, RepositoryBinding};
use af_wire::access_verb::AccessRequest;
use afd_api_wire::grant::kind;
use afd_approval::{GrantKind, Requested, Wanted};
use afd_credential::secrets::connector;
use afd_gate::policy::approved::Approved;
use jiff::Timestamp;
use serde_json::Value;

use crate::error::{Result, query, rejected, vault_data_invalid};
use crate::run::mint::claim::{Claim, claim};
use crate::run::pull::RunService;
use crate::run::refusal::FleetRefusal;
use crate::run::standing::Standing;

/// Whether a workspace connected a provider's account.
///
/// `$1` workspace, `$2` provider.
pub(crate) const PROVIDER_CONNECTED: &str = include_str!("provider_connected.sql");

/// The connector whose grant takes a repository scope.
const GITHUB: &str = "github";

/// The field of a static secret naming the one host it is sent to.
const FIELD_HOST: &str = "host";

/// Why a request's scope was refused.
const DETAIL_SCOPE_NEEDED: &str = "a GitHub request names its repositories and read or write";
/// See [`DETAIL_SCOPE_NEEDED`].
const DETAIL_SCOPE_UNREAD: &str =
    "the GitHub scope must name repositories as owner/name, read or write, and a write's base";
/// See [`DETAIL_SCOPE_NEEDED`].
const DETAIL_SCOPE_REFUSED: &str = "only a GitHub request takes a scope";
/// Why a secret could not be asked for.
const DETAIL_HOST_UNREAD: &str = "the secret's host field names no host a request could reach";

/// What an access request did, and the channel the grant reaches a run
/// through, which the request's report names.
#[derive(Debug)]
pub struct AccessAsked {
    /// The fleet the run runs, which the grant is for.
    pub fleet: Uuid7,
    /// What the request did: raised a card, found one open, or was covered.
    pub requested: Requested,
    /// Whether the grant is a secret or an integration.
    pub kind: GrantKind,
}

/// What one request asks a person for, once checked.
struct Asked {
    kind: GrantKind,
    scope: Option<String>,
}

impl RunService {
    /// Asks a person for `request.name` on behalf of the run, or answers that
    /// the approved grant already covers it.
    ///
    /// # Errors
    /// Refuses a run that is not this runner's live holder at the presented
    /// fence; a name the vault does not hold (`UZ-AGT-022`); an integration
    /// the workspace has not connected (`UZ-AGT-023`); a GitHub request with
    /// no scope or one the parser refuses, and a scope on any other name; a
    /// secret whose host no request could reach. Reports datastore failures.
    /// Every refusal writes no grant and raises no card.
    pub async fn request_access(
        &self,
        runner_id: &Uuid7,
        run_id: Uuid7,
        request: &garde::Valid<AccessRequest<'_>>,
        now: Timestamp,
    ) -> Result<AccessAsked> {
        let standing = self
            .standing(runner_id, run_id, request.fencing_token)
            .await?;
        let name = request.name.as_ref();
        let asked = self.asked(&standing, name, request.scope.as_ref()).await?;
        let requested = self
            .grants
            .request(
                &standing.workspace_id,
                &standing.fleet_id,
                Wanted {
                    name,
                    kind: asked.kind,
                    scope: asked.scope.as_deref(),
                    turn: &standing.turn_id,
                },
                now,
            )
            .await?;
        Ok(AccessAsked {
            fleet: standing.fleet_id,
            requested,
            kind: asked.kind,
        })
    }

    /// The grant's kind and the scope to ask for, from the vault entry `name`.
    async fn asked(
        &self,
        standing: &Standing,
        name: &str,
        scope: Option<&serde_json::Map<String, Value>>,
    ) -> Result<Asked> {
        let connectors = self.broker.connectors();
        let Some(held) = self.vault.find(&standing.workspace_id, name).await? else {
            // A connector's name the vault does not hold is an integration
            // nobody connected; any other name is a secret nobody stored.
            return Err(if connectors.resolve(name).is_some() {
                FleetRefusal::ProviderNotConnected.into()
            } else {
                FleetRefusal::AccessNameNotFound.into()
            });
        };
        let stored: Value =
            serde_json::from_slice(held.expose()).map_err(|_shape| vault_data_invalid())?;
        if connector::mintable(connectors, &stored).is_none() {
            if scope.is_some() {
                return Err(rejected(DETAIL_SCOPE_REFUSED));
            }
            readable_host(&stored)?;
            return Ok(Asked {
                kind: GrantKind::Secret,
                scope: None,
            });
        }
        self.require_connected(&standing.workspace_id, name, &stored)
            .await?;
        let scope = if name == GITHUB {
            Some(self.github_scope(standing, scope).await?)
        } else if scope.is_some() {
            return Err(rejected(DETAIL_SCOPE_REFUSED));
        } else {
            None
        };
        Ok(Asked {
            kind: GrantKind::Integration,
            scope,
        })
    }

    /// Refuses an on-demand handle whose installation the workspace never
    /// connected, or one filed under another connector's name.
    async fn require_connected(&self, workspace: &Uuid7, name: &str, stored: &Value) -> Result<()> {
        let connected = match claim(self.broker.connectors(), name, stored) {
            Ok(Claim::Unowned) => true,
            Ok(Claim::Installation { provider, id }) => {
                self.runs
                    .holds_install(workspace, provider.id(), &id.to_string())
                    .await?
            }
            Err(_unusable) => false,
        };
        if connected {
            Ok(())
        } else {
            Err(FleetRefusal::ProviderNotConnected.into())
        }
    }

    /// The GitHub scope to ask for, as a grant records it: the approved one
    /// when it already covers the request, so the request raises no card, and
    /// the requested one otherwise.
    async fn github_scope(
        &self,
        standing: &Standing,
        scope: Option<&serde_json::Map<String, Value>>,
    ) -> Result<String> {
        let scope = scope.ok_or_else(|| rejected(DETAIL_SCOPE_NEEDED))?;
        let document = serde_json::to_string(scope).map_err(af_core::json::Unreadable::from)?;
        let asked = RepositoryBinding::parse(&document)
            .map_err(|_refused| rejected(DETAIL_SCOPE_UNREAD))?;
        let approved = {
            let mut connection = self.runs.pool().acquire().await?;
            Approved::read_on(&mut connection, &standing.fleet_id).await?
        };
        let held = approved
            .holds(GITHUB, kind::INTEGRATION)
            .then(|| approved.repository_binding().ok().flatten())
            .flatten()
            .filter(|held| covers(held, &asked));
        let recorded = held
            .as_ref()
            .map_or_else(|| asked.recorded(), RepositoryBinding::recorded);
        Ok(serde_json::to_string(&recorded).map_err(af_core::json::Unreadable::from)?)
    }
}

/// Whether the approved `held` scope already reaches everything `asked`
/// asks for: every repository, and a write only at the base, publication
/// and validation workflows held.
fn covers(held: &RepositoryBinding, asked: &RepositoryBinding) -> bool {
    let repositories = asked.repositories().iter().all(|wanted| {
        held.repositories()
            .iter()
            .any(|have| have.eq_ignore_ascii_case(wanted))
    });
    let reach = match asked.reach() {
        Reach::Read => true,
        Reach::Write { .. } => {
            held.reach() == asked.reach()
                && held.publication() == asked.publication()
                && asked
                    .validation_workflows()
                    .iter()
                    .all(|workflow| held.validation_workflows().contains(workflow))
        }
    };
    repositories && reach
}

/// Refuses a static secret whose `host` names no host a request could reach.
/// A secret with no `host` is sent nowhere on a restricted fleet, which is not
/// a reason to refuse asking for it.
fn readable_host(stored: &Value) -> Result<()> {
    stored.get(FIELD_HOST).map_or_else(
        || Ok(()),
        |host| {
            host.as_str()
                .and_then(af_egress::allowlist_host)
                .map(drop)
                .ok_or_else(|| rejected(DETAIL_HOST_UNREAD))
        },
    )
}

/// Whether `workspace` connected `provider`, on the caller's connection.
///
/// # Errors
/// Reports a statement that would not run.
pub(crate) async fn provider_connected(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
    provider: &str,
) -> Result<bool> {
    sqlx::query_scalar(PROVIDER_CONNECTED)
        .bind(workspace)
        .bind(provider)
        .fetch_one(connection)
        .await
        .map_err(query("read whether the workspace connected a provider"))
}

#[cfg(test)]
mod tests;
