//! What a lease authorises a mint to reach.
//!
//! The mint verb's authorisation, and the whole of it. A runner presents a
//! `lease_id` and the name of an integration; everything else — which
//! workspace's vault is opened, which fleet's grant is checked, which event's
//! approval may be spent, and how far the minted token may reach — is read from
//! ONE row here.
//!
//! # One row, because two would be a second answer
//!
//! The binding comes back from the same `SELECT` as the workspace, joined
//! through the fleet the lease names. Read separately, the binding could belong
//! to a fleet the lease does not authorise — and it is the value that decides
//! how wide a GitHub token is scoped, so a mismatch there is a token minted for
//! repositories nobody approved.
//!
//! # An unreadable config withholds, it does not widen
//!
//! A `config_json` this daemon cannot parse yields `None` for the binding, and
//! a repository-scoped mint refuses on `None` rather than minting the
//! installation's full scope. That is the direction `credentials_mint_scope.zig`
//! fails in too, and it is the only safe one: the binding is what NARROWS the
//! request, so its absence must never be the permissive branch.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet_runtime::config::{FleetConfig, RepositoryBinding};
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_SCOPE: &str = "lease mint scope";

/// The event a diagnostic names when a fleet's stored config will not parse.
const EVENT_CONFIG_UNPARSED: &str = "credential_mint_config_unparsed";

/// Everything the mint reads out of the lease it was handed.
#[derive(Debug, Clone)]
pub struct MintScope {
    /// Whose vault is opened. Derived here, never taken from the wire.
    pub workspace_id: Uuid7,
    /// Whose standing grant is checked.
    pub fleet_id: Uuid7,
    /// Which event's approval a write mint may spend.
    ///
    /// The gate that parked THIS event is the answer this lease's mint spends
    /// — an approval given for one event cannot fund another.
    pub event_id: Box<str>,
    /// How far a repository-scoped token may reach, if the fleet declared it.
    pub binding: Option<RepositoryBinding>,
}

impl Leases {
    /// The scope `lease_id` authorises, for the runner presenting it.
    ///
    /// `None` is every way a lease fails to authorise — belonging to another
    /// runner, expired, cancelled, or never existing — and they are one answer
    /// on purpose: telling them apart would tell a caller holding a foreign
    /// lease id that it exists.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored identifier that
    /// is not one. A fleet whose config will not parse is NOT an error — it is
    /// a scope with no binding, which refuses a write mint one step later.
    pub async fn mint_scope(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        now: UnixMillis,
    ) -> Result<Option<MintScope>> {
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::lease::SELECT_LEASE_SCOPE_FOR_MINT)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SCOPE))?;
        let Some(row) = row else {
            return Ok(None);
        };

        let workspace_id: String = row.try_get(0).map_err(query(CONTEXT_SCOPE))?;
        let fleet_id: String = row.try_get(1).map_err(query(CONTEXT_SCOPE))?;
        let config_json: String = row.try_get(2).map_err(query(CONTEXT_SCOPE))?;
        let event_id: String = row.try_get(3).map_err(query(CONTEXT_SCOPE))?;

        // A stored identifier that is not one is a datastore this daemon cannot
        // reason about, not a lease that failed to authorise — so it propagates
        // rather than becoming a `None` a caller would read as "no such lease".
        Ok(Some(MintScope {
            workspace_id: Uuid7::parse(&workspace_id)?,
            fleet_id: Uuid7::parse(&fleet_id)?,
            event_id: event_id.into(),
            binding: binding_of(&config_json),
        }))
    }
}

/// The fleet's declared repository reach, if its config states one readably.
///
/// Both failures answer `None` and mean the same thing to the caller — this
/// fleet declared no reach this mint may honour — but only one of them is worth
/// a diagnostic, because a config that will not parse is an operator's problem
/// and a fleet that simply declared no repositories is not.
fn binding_of(config_json: &str) -> Option<RepositoryBinding> {
    match FleetConfig::stored(config_json) {
        Ok(config) => config.repository_binding().cloned(),
        Err(unreadable) => {
            tracing::warn!(
                event = EVENT_CONFIG_UNPARSED,
                error = %unreadable,
                "a fleet's stored config would not parse; a repository-scoped mint will refuse"
            );
            None
        }
    }
}
