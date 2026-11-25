//! The assignment read: what a fleet IS when a runner takes its thread.
//!
//! [`super::assign`] has already decided WHICH fleet; this reads the row its
//! run is built from — its name, its model, its network toggles and the
//! session it continues. A change a person makes to the row reaches the NEXT
//! run, because every assignment reads it fresh and nothing caches it.

use af_core::id::Uuid7;
use af_fleet_runtime::config::HttpEgress;

use crate::error::{ErrorKind, Result, query};
use crate::run::sql;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_INSTALLED: &str = "fleet assignment read";

/// The session context a fleet with no checkpoint starts from.
///
/// An empty JSON OBJECT rather than `null` or an empty string: the runner
/// reads this as a context document, and a fleet on its first run has an empty
/// one rather than a missing one.
pub const FRESH_CONTEXT: &str = "{}";

/// A fleet as one run runs it.
#[derive(Debug, Clone)]
pub struct Installed {
    /// The workspace that owns it.
    pub workspace_id: Uuid7,
    /// Its name, which the run's fixed prompt text introduces it by.
    pub name: String,
    /// The workspace model its runs resolve.
    pub model_ref: String,
    /// Where its HTTP tools may reach.
    pub http_egress: HttpEgress,
    /// Whether its HTTP tools may also reach the package registries.
    pub package_domains: bool,
    /// The session context this run continues, or [`FRESH_CONTEXT`].
    pub context_json: String,
}

impl Runs {
    /// The fleet behind `fleet_id`, if it is still runnable.
    ///
    /// `None` means the fleet is no longer `active`, or its row is gone. That
    /// is not an error: the selection pass filters on status, so reaching here
    /// with a fleet that is not active means it was archived in the window
    /// between selection and this read. The caller answers no-work.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a stored network scope
    /// this daemon cannot read.
    pub async fn installed(&self, fleet_id: &Uuid7) -> Result<Option<Installed>> {
        let mut connection = self.pool().acquire().await?;
        Self::installed_on(&mut connection, fleet_id).await
    }

    pub(crate) async fn installed_on(
        connection: &mut sqlx::PgConnection,
        fleet_id: &Uuid7,
    ) -> Result<Option<Installed>> {
        let found: Option<FleetWithSession> = sqlx::query_as(sql::fleet::SELECT_FLEET_WITH_THREAD)
            .bind(fleet_id)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT_INSTALLED))?;
        found.map_or(Ok(None), FleetWithSession::install)
    }

    /// The fleet the assignment statement read, as installed. The assignment
    /// admits only an active fleet, so this answers `None` only for a row this
    /// daemon would not run either.
    ///
    /// # Errors
    /// As [`Self::installed`].
    pub(crate) fn installed_assigned(
        opened: &crate::run::envelope::Opened,
    ) -> Result<Option<Installed>> {
        opened.setup.clone().install()
    }
}

/// [`sql::fleet::SELECT_FLEET_WITH_THREAD`]'s row, and the same columns of
/// the assignment statement's: the fleet and its session.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct FleetWithSession {
    pub(crate) workspace_id: Uuid7,
    pub(crate) status: String,
    pub(crate) name: String,
    pub(crate) model_ref: String,
    pub(crate) http_egress: String,
    pub(crate) package_domains: bool,
    pub(crate) context_json: Option<String>,
}

impl FleetWithSession {
    /// The row as a run runs it, or `None` for a fleet that is not active.
    fn install(self) -> Result<Option<Installed>> {
        if self.status != sql::FLEET_STATUS_ACTIVE {
            return Ok(None);
        }
        let http_egress =
            HttpEgress::parse(&self.http_egress).ok_or(ErrorKind::FleetRowUnreadable)?;
        Ok(Some(Installed {
            workspace_id: self.workspace_id,
            name: self.name,
            model_ref: self.model_ref,
            http_egress,
            package_domains: self.package_domains,
            context_json: self
                .context_json
                .unwrap_or_else(|| FRESH_CONTEXT.to_owned()),
        }))
    }
}

#[cfg(test)]
impl FleetWithSession {
    /// An active, private fleet on the fixture model.
    pub(crate) fn active(workspace_id: Uuid7) -> Self {
        Self {
            workspace_id,
            status: sql::FLEET_STATUS_ACTIVE.to_owned(),
            name: "Support triage".to_owned(),
            model_ref: "fixture-model".to_owned(),
            http_egress: HttpEgress::Restricted.as_str().to_owned(),
            package_domains: false,
            context_json: None,
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a refused fixture fails the proof loudly"
)]
mod tests {
    use super::{FRESH_CONTEXT, FleetWithSession};

    fn workspace() -> af_core::id::Uuid7 {
        af_core::id::Uuid7::parse("019a0000-0000-7000-8000-000000000001")
            .expect("a canonical identifier")
    }

    #[test]
    fn an_active_row_installs_with_its_name_model_and_network() {
        let row = FleetWithSession {
            http_egress: "public".to_owned(),
            package_domains: true,
            ..FleetWithSession::active(workspace())
        };

        let installed = row
            .install()
            .expect("a readable row")
            .expect("an active row installs");

        assert_eq!(installed.name, "Support triage");
        assert_eq!(installed.model_ref, "fixture-model");
        assert_eq!(
            installed.http_egress,
            af_fleet_runtime::config::HttpEgress::Public
        );
        assert!(installed.package_domains);
        assert_eq!(installed.context_json, FRESH_CONTEXT);
    }

    #[test]
    fn an_archived_row_installs_nothing_and_an_unknown_scope_is_refused() {
        let archived = FleetWithSession {
            status: "archived".to_owned(),
            ..FleetWithSession::active(workspace())
        };
        assert!(matches!(archived.install(), Ok(None)));

        let unreadable = FleetWithSession {
            http_egress: "allow_all".to_owned(),
            ..FleetWithSession::active(workspace())
        };
        assert_eq!(
            unreadable.install().err().map(|refused| refused.code()),
            Some(af_core::error_code::INTERNAL_DB_QUERY)
        );
    }
}
