//! What a run authorises a mint to reach.
//!
//! The mint verb's authorisation, and the whole of it. A runner presents a
//! `run_id` and the name of an integration; everything else — which
//! workspace's vault is opened, which fleet's grant is checked, which event's
//! approval may be spent, and how far the minted token may reach — is read from
//! ONE row here.
//!
//! # The approved grants, read now
//!
//! A mint reads the grants a person approved for the run's fleet as they
//! stand at the mint, not as they stood at the run's issue: a revoke refuses
//! the next mint, and a wider GitHub scope still waiting on a person mints
//! nothing beyond the approved one. The GitHub scope a person approved is what
//! NARROWS a repository-scoped token, so an approved scope this daemon cannot
//! read withholds the binding, and a repository-scoped mint refuses on `None`
//! rather than minting the installation's full scope.

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_fleet_runtime::config::RepositoryBinding;
use afd_api_wire::grant::kind;
use afd_gate::policy::approved::Approved;

use crate::error::{Result, query};
use crate::run::sql;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_SCOPE: &str = "run mint scope";

/// Statement name for the installation-ownership read.
const CONTEXT_INSTALL_HELD: &str = "run mint installation owner";

/// The event a diagnostic names when an approved GitHub scope will not parse.
const EVENT_SCOPE_UNPARSED: &str = "credential_mint_scope_unparsed";

/// Everything the mint reads out of the run it was handed.
#[derive(Debug, Clone)]
pub struct MintScope {
    /// Whose vault is opened. Derived here, never taken from the wire.
    pub workspace_id: Uuid7,
    /// Whose standing grant is checked.
    pub fleet_id: Uuid7,
    /// The turn the run serves.
    pub turn_id: Box<str>,
    /// How far a repository-scoped token may reach: the approved GitHub scope.
    pub binding: Option<RepositoryBinding>,
    /// The integrations a person approved for the fleet, by name.
    pub(crate) integrations: Vec<String>,
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
    /// is not one. An approved scope that will not parse is NOT an error — it
    /// is a scope with no binding, which refuses a repository mint one step
    /// later.
    pub async fn mint_scope(&self, runner_id: &Uuid7, run_id: &Uuid7) -> Result<Option<MintScope>> {
        let mut connection = self.pool().acquire().await?;
        // The run's workspace, fleet and turn, in the statement's column order.
        let found: Option<(Uuid7, Uuid7, Uuid7)> =
            sqlx::query_as(sql::run::SELECT_RUN_SCOPE_FOR_MINT)
                .bind(run_id)
                .bind(runner_id)
                .bind(sql::RUN_STATUS_ACTIVE)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query(CONTEXT_SCOPE))?;
        let Some((workspace_id, fleet_id, turn_id)) = found else {
            return Ok(None);
        };
        let approved = Approved::read_on(&mut connection, &fleet_id).await?;
        let integrations = approved
            .names()
            .into_iter()
            .filter(|name| approved.holds(name, kind::INTEGRATION))
            .map(str::to_owned)
            .collect();
        Ok(Some(MintScope {
            binding: binding_of(&approved),
            workspace_id,
            fleet_id,
            turn_id: turn_id.into(),
            integrations,
        }))
    }

    /// The GitHub scope a person approved for `fleet` as it stands now, read
    /// on the caller's transaction: a task follows the fleet's access at each
    /// step, so a revoke or a narrower approval refuses its next publication.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an approved scope that
    /// does not read back.
    pub(crate) async fn approved_binding_on(
        connection: &mut sqlx::PgConnection,
        fleet: &Uuid7,
    ) -> Result<Option<RepositoryBinding>> {
        Ok(Approved::read_on(connection, fleet)
            .await?
            .repository_binding()?)
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

/// The approved GitHub scope, if one is approved and reads back.
///
/// An unreadable scope answers `None`, which refuses a repository mint, and
/// says so: it is a row this daemon did not write.
fn binding_of(approved: &Approved) -> Option<RepositoryBinding> {
    approved.repository_binding().unwrap_or_else(|unreadable| {
        let Logged { error_code, reason } = unreadable.logged();
        {
            let (event, message) = (
                &EVENT_SCOPE_UNPARSED,
                format_args!("an approved GitHub scope would not parse; no repository is minted"),
            );
            tracing::warn!(message, event, error_code, reason);
        };
        None
    })
}

#[cfg(test)]
mod tests {
    use super::{EVENT_SCOPE_UNPARSED, binding_of};
    use afd_gate::policy::approved::{Approved, ApprovedGrant};

    #[test]
    fn an_unreadable_approved_scope_withholds_the_binding_and_says_so() {
        let log = af_core::test_util::trace::Capture::install();
        let approved: Approved = std::iter::once(ApprovedGrant {
            name: "github".to_owned(),
            kind: afd_api_wire::grant::kind::INTEGRATION.to_owned(),
            scope: Some("not a scope".to_owned()),
        })
        .collect();

        assert!(binding_of(&approved).is_none());

        let line = log.only(EVENT_SCOPE_UNPARSED);
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

    #[test]
    fn a_fleet_with_nothing_approved_has_no_binding() {
        assert!(binding_of(&Approved::none()).is_none());
    }
}
