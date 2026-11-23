//! What a run authorises a mint to reach.
//!
//! The mint verb's authorisation, and the whole of it. A runner presents a
//! `run_id` and the name of an integration; everything else — which
//! workspace's vault is opened, which fleet's grant is checked, which event's
//! approval may be spent, and how far the minted token may reach — is read from
//! ONE row here.
//!
//! # One row, because two would be a second answer
//!
//! The binding comes back from the same `SELECT` as the workspace, joined
//! through the fleet the run names. Read separately, the binding could belong
//! to a fleet the run does not authorise — and it is the value that decides
//! how wide a GitHub token is scoped, so a mismatch there is a token minted for
//! repositories nobody approved.
//!
//! # An unreadable config withholds, it does not widen
//!
//! A revision `config` this daemon cannot parse yields `None` for the binding, and
//! a repository-scoped mint refuses on `None` rather than minting the
//! installation's full scope. That is the only safe direction: the binding is
//! what NARROWS the request, so its absence must never be the permissive
//! branch.

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_fleet_runtime::config::RepositoryBinding;
use af_fleet_runtime::{CredentialName, Definition, FleetConfig};

use crate::error::{Result, query};
use crate::run::sql;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_SCOPE: &str = "run mint scope";

/// Statement name for the installation-ownership read.
const CONTEXT_INSTALL_HELD: &str = "run mint installation owner";

/// The event a diagnostic names when a fleet's stored config will not parse.
const EVENT_CONFIG_UNPARSED: &str = "credential_mint_config_unparsed";

/// Everything the mint reads out of the run it was handed.
#[derive(Debug, Clone)]
pub struct MintScope {
    /// Whose vault is opened. Derived here, never taken from the wire.
    pub workspace_id: Uuid7,
    /// Whose standing grant is checked.
    pub fleet_id: Uuid7,
    /// Which event's approval a write mint may spend.
    ///
    /// The gate that parked THIS event is the answer this run's mint spends
    /// — an approval given for one event cannot fund another.
    pub turn_id: Box<str>,
    /// How far a repository-scoped token may reach, if the fleet declared it.
    pub binding: Option<RepositoryBinding>,
    /// Credentials present in both the assignment and current setup.
    pub(crate) runner_credentials: Vec<CredentialName>,
}

impl Runs {
    /// The scope `run_id` authorises, for the runner presenting it.
    ///
    /// `None` is every way a run fails to authorise — belonging to another
    /// runner, expired, cancelled, or never existing — and they are one answer
    /// on purpose: telling them apart would tell a caller holding a foreign
    /// run id that it exists.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored identifier that
    /// is not one. A fleet whose config will not parse is NOT an error — it is
    /// a scope with no binding, which refuses a write mint one step later.
    pub async fn mint_scope(&self, runner_id: &Uuid7, run_id: &Uuid7) -> Result<Option<MintScope>> {
        let mut connection = self.pool().acquire().await?;
        let found: Option<ScopeRow> = sqlx::query_as(sql::run::SELECT_RUN_SCOPE_FOR_MINT)
            .bind(run_id)
            .bind(runner_id)
            .bind(sql::RUN_STATUS_ACTIVE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SCOPE))?;
        let Some(found) = found else {
            return Ok(None);
        };

        let current = config_of(&found.definition);
        let previous = found.issued_definition.as_deref().and_then(config_of);
        let issued = if found.issued_is_current {
            current.as_ref()
        } else {
            previous.as_ref()
        };
        let runner_credentials = match (current.as_ref(), issued) {
            (Some(current), Some(issued)) => issued
                .credentials()
                .iter()
                .filter(|name| current.credentials().contains(name))
                .filter(|name| {
                    name.as_str() != "github"
                        || current.repository_binding() == issued.repository_binding()
                })
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        Ok(Some(MintScope {
            workspace_id: found.workspace_id.clone(),
            fleet_id: found.fleet_id.clone(),
            turn_id: found.turn_id.into(),
            binding: current.and_then(|config| config.repository_binding().cloned()),
            runner_credentials,
        }))
    }

    /// Whether `workspace` connected `provider`'s external account `account`.
    ///
    /// The id a stored handle names is bytes the workspace wrote; the routing
    /// row is what the connect flow wrote after the provider proved the
    /// workspace's access to it. A mint spends the first only when the second
    /// agrees.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn holds_install(
        &self,
        workspace: &Uuid7,
        provider: &str,
        account: &str,
    ) -> Result<bool> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_scalar(sql::run::SELECT_CONNECTOR_INSTALL_HELD)
            .bind(provider)
            .bind(account)
            .bind(workspace)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_INSTALL_HELD))
    }

    pub(crate) async fn hold_install_on(
        connection: &mut sqlx::PgConnection,
        workspace: &Uuid7,
        account: &str,
    ) -> Result<bool> {
        Ok(
            sqlx::query_scalar::<_, String>(include_str!("sql/run/hold_connector_install.sql"))
                .bind("github")
                .bind(account)
                .bind(workspace)
                .fetch_optional(connection)
                .await
                .map_err(query(CONTEXT_INSTALL_HELD))?
                .is_some(),
        )
    }
}

/// The fleet's declared repository reach, if its config states one readably.
///
/// Both failures answer `None` and mean the same thing to the caller — this
/// fleet declared no reach this mint may honour — but only one of them is worth
/// a diagnostic, because a config that will not parse is an operator's problem
/// and a fleet that simply declared no repositories is not.
fn config_of(definition: &str) -> Option<FleetConfig> {
    match Definition::parse(definition) {
        Ok(definition) => Some(definition.runtime().clone()),
        Err(unreadable) => {
            let Logged { error_code, reason } = unreadable.logged();
            {
                let (event, message) = (
                    &EVENT_CONFIG_UNPARSED,
                    format_args!(
                        "a fleet's stored config would not parse; runner credential authority is unavailable"
                    ),
                );
                tracing::warn!(message, event, error_code, reason);
            };
            None
        }
    }
}

/// The mint's current and issued setup, with typed identifiers.
#[derive(sqlx::FromRow)]
struct ScopeRow {
    workspace_id: Uuid7,
    fleet_id: Uuid7,
    definition: String,
    issued_definition: Option<String>,
    issued_is_current: bool,
    turn_id: Uuid7,
}

#[cfg(test)]
mod tests {
    use super::{EVENT_CONFIG_UNPARSED, config_of};

    #[test]
    fn an_unparsed_scope_config_reason_carries_no_code_prefix() {
        let log = af_core::test_util::trace::Capture::install();

        assert!(config_of("not a document").is_none());

        let line = log.only(EVENT_CONFIG_UNPARSED);
        let reason = line.field("reason").unwrap_or_default();
        assert!(
            line.field("error_code")
                .is_some_and(|code| !code.is_empty())
        );
        assert!(
            !reason.is_empty() && !reason.contains("[UZ-"),
            "the code is the line's `error_code`, never the reason's prefix: {reason}"
        );
    }
}
