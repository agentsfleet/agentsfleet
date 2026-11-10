//! The claim read: what a fleet IS, at the moment a runner takes its slot.
//! Beside it, the renewal's ceiling read (`Leases::stored_config`), which
//! reads the same stored config whatever the fleet's status.
//!
//! The claim read is one statement, run once per claim. [`super::assign`] has
//! already decided WHICH fleet; this reads the installed shape that fleet's
//! lease is built from — the config the gates judge against, the prose the run
//! is given, and the session it continues.
//!
//! # Why the parse is cached by revision id and nothing else
//!
//! A fleet's config is what the money gates price, what the approval gate
//! judges, and what the execution policy is assembled from. An operator who
//! narrows a fleet's tools or lowers its budget expects the NEXT run to obey
//! the change. The claim read returns the revision id beside the document, and
//! the parse is cached under that id ([`crate::lease::cache`]). Revisions are
//! append-only, so a cached entry can only answer for the document it was
//! parsed from, and an edit is a new head revision whose id misses. A cache on
//! a timer would make that "some later run, once an entry expired", and the
//! window would be invisible.

use af_core::id::Uuid7;
use af_fleet_runtime::{Definition, FleetConfig, definition::POLICY_SEMANTICS};

use crate::error::{Result, query};
use crate::lease::cache::{Definitions, RevisionKey};
use crate::lease::sql;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_INSTALLED: &str = "fleet claim read";

/// Statement name for the renewal's ceiling read.
const CONTEXT_STORED_CONFIG: &str = "fleet ceiling read";

/// The session context a fleet with no checkpoint starts from.
///
/// An empty JSON OBJECT rather than `null` or an empty string: the runner
/// reads this as a context document, and a fleet on its first run has an empty
/// one rather than a missing one.
pub const FRESH_CONTEXT: &str = "{}";

/// Immutable revision selection used to assemble the delivered execution policy.
#[derive(Debug, Clone)]
pub struct IssueRevision {
    /// Accepted setup whose text and policy were assembled.
    pub accepted: Uuid7,
    /// Current authority ceiling observed during assembly.
    pub current: Uuid7,
    /// Native generation, absent before the initial interpretation.
    pub task_generation: Option<i64>,
}

/// A fleet as installed, resolved for one lease.
#[derive(Debug, Clone)]
pub struct Installed {
    /// The workspace that owns it.
    pub workspace_id: Uuid7,
    /// Its name, as the operator typed it.
    pub name: String,
    /// The typed config the gates and the policy are built from.
    pub config: FleetConfig,
    /// The behaviour prose the run is given.
    pub instructions: String,
    /// The session context this run continues, or [`FRESH_CONTEXT`].
    pub context_json: String,
    /// Exact owner model reference accepted with this immutable setup.
    pub runtime_model_ref: String,
    /// Revision proof rechecked under the issue transaction.
    pub issue_revision: Option<IssueRevision>,
}

impl Installed {
    /// The credential names the fleet declared, as a vault read wants them.
    #[must_use]
    pub fn credential_names(&self) -> Vec<&str> {
        self.config
            .credentials()
            .iter()
            .map(af_fleet_runtime::CredentialName::as_str)
            .collect()
    }
}

impl Leases {
    /// The installed fleet behind `fleet_id`, if it is still runnable.
    ///
    /// `None` means the fleet is no longer `active`. That is not an error and
    /// not a missing row: the selection pass filters on status, so reaching
    /// here with a stopped fleet means an operator paused it in the window
    /// between selection and this read. The caller answers no-work and the
    /// claim lapses on its own — which is the same thing that happens to every
    /// other fleet an operator stops.
    ///
    /// A fleet whose row is gone answers `None` for the same reason: there is
    /// nothing to run and nothing for an operator to fix.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a `workspace_id` that will
    /// not parse as a UUID, and a revision `config` this daemon cannot read. The
    /// last is deliberately fatal to THIS FLEET's lease rather than skipped: a
    /// fleet whose stored config is unreadable must not run under a config this
    /// daemon guessed at. It is fatal to nothing else — the pull path answers
    /// it with that fleet's terminal row and a documented no-work, because a
    /// runner polls a rotation and refusing the RUNNER would refuse every
    /// fleet. See `super::pull::refuse::refuse_unreadable_config`.
    pub async fn installed(&self, fleet_id: &Uuid7) -> Result<Option<Installed>> {
        self.installed_for_admission(fleet_id, None).await
    }

    pub(crate) async fn installed_for_admission(
        &self,
        fleet_id: &Uuid7,
        event: Option<&Uuid7>,
    ) -> Result<Option<Installed>> {
        let mut connection = self.pool().acquire().await?;
        Self::installed_on(self.definitions(), &mut connection, fleet_id, event).await
    }

    pub(crate) async fn installed_on(
        definitions: &Definitions,
        connection: &mut sqlx::PgConnection,
        fleet_id: &Uuid7,
        event: Option<&Uuid7>,
    ) -> Result<Option<Installed>> {
        let found: Option<FleetWithSession> = sqlx::query_as(sql::fleet::SELECT_FLEET_WITH_SESSION)
            .bind(fleet_id)
            .bind(event)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT_INSTALLED))?;
        match found {
            Some(row) => Self::install(definitions, row).await,
            None => Ok(None),
        }
    }

    /// The fleet the claim statement read, as installed. The claim admits only
    /// an active fleet, so this answers `None` only for a row this daemon would
    /// not lease either.
    ///
    /// # Errors
    /// As [`Self::installed`]: an unreadable document, a setup this daemon
    /// cannot run, or a task whose accepted setup no longer fits.
    pub(crate) async fn installed_claimed(
        &self,
        opened: &crate::lease::envelope::Opened,
    ) -> Result<Option<Installed>> {
        Self::install(self.definitions(), opened.setup.clone()).await
    }

    /// A fleet row, its revision and its session, as a lease runs them.
    async fn install(
        definitions: &Definitions,
        row: FleetWithSession,
    ) -> Result<Option<Installed>> {
        let FleetWithSession {
            workspace_id: workspace,
            definition: document,
            status,
            runtime_model_ref,
            name,
            context_json,
            policy_semantics: policy,
            revision_id,
            head_revision_id,
            task_generation,
            current_definition,
        } = row;
        if status != sql::FLEET_STATUS_ACTIVE {
            return Ok(None);
        }
        if policy.as_deref() != Some(POLICY_SEMANTICS) {
            return Err(crate::error::ErrorKind::SetupUnavailable.into());
        }
        let document = document.ok_or(crate::error::ErrorKind::SetupUnavailable)?;
        let revision = revision_id
            .clone()
            .ok_or(crate::error::ErrorKind::SetupUnavailable)?;
        let definition = definitions
            .get_or_parse(&RevisionKey::observed(revision), &document, parse_for_save)
            .await?;
        if task_generation.is_some() {
            let current = Definition::parse(
                &current_definition.ok_or(crate::error::ErrorKind::SetupUnavailable)?,
            )?;
            if !definition.runtime().same_task_authority(current.runtime()) {
                return Err(crate::task::TaskRefusal::InvalidTarget.into());
            }
        }

        Ok(Some(Installed {
            workspace_id: workspace,
            name,
            config: definition.runtime().clone(),
            instructions: definition.instructions_md().to_owned(),
            context_json: context_json.unwrap_or_else(|| FRESH_CONTEXT.to_owned()),
            runtime_model_ref: runtime_model_ref
                .ok_or(crate::error::ErrorKind::SetupUnavailable)?,
            issue_revision: revision_id
                .zip(head_revision_id)
                .map(|(accepted, current)| IssueRevision {
                    accepted,
                    current,
                    task_generation,
                }),
        }))
    }

    /// The config stored for `fleet_id` now, whatever the fleet's status.
    ///
    /// The renewal's ceiling read, beside the claim read above. Unlike
    /// [`Self::installed`], a stopped or killed fleet still answers: a run in
    /// flight keeps renewing under the fleet's stored ceiling, so the ceiling
    /// has to stay readable after the fleet stops. A paused or stopped fleet's
    /// config can still be edited mid-run and the next renewal reads the edit;
    /// a killed fleet's cannot. `None` only when the row is gone.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a revision `config` this
    /// daemon cannot read, as [`Self::installed`] does.
    pub(crate) async fn stored_config(&self, fleet_id: &Uuid7) -> Result<Option<FleetConfig>> {
        let mut connection = self.pool().acquire().await?;
        Self::stored_config_on(&mut connection, fleet_id, None).await
    }

    pub(crate) async fn stored_config_on(
        connection: &mut sqlx::PgConnection,
        fleet_id: &Uuid7,
        revision_id: Option<&Uuid7>,
    ) -> Result<Option<FleetConfig>> {
        let document: Option<String> = sqlx::query_scalar(sql::fleet::SELECT_FLEET_CONFIG)
            .bind(fleet_id)
            .bind(revision_id)
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT_STORED_CONFIG))?;
        document
            .map(|document| {
                Definition::parse(&document)
                    .map(|definition| definition.runtime().clone())
                    .map_err(Into::into)
            })
            .transpose()
    }
}

/// A stored setup as a lease runs it: parsed, then checked as a save would be.
///
/// Cached only once both pass, so a cache hit never skips the check.
fn parse_for_save(document: &str) -> Result<Definition> {
    let definition = Definition::parse(document)?;
    definition.validate_for_save()?;
    Ok(definition)
}

/// [`sql::fleet::SELECT_FLEET_WITH_SESSION`]'s row, and the same columns of
/// the claim statement's: the fleet, its head revision and its session, each
/// revision column absent until one is installed.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct FleetWithSession {
    pub(crate) workspace_id: Uuid7,
    pub(crate) definition: Option<String>,
    pub(crate) status: String,
    pub(crate) runtime_model_ref: Option<String>,
    pub(crate) name: String,
    pub(crate) context_json: Option<String>,
    pub(crate) policy_semantics: Option<String>,
    pub(crate) revision_id: Option<Uuid7>,
    pub(crate) head_revision_id: Option<Uuid7>,
    pub(crate) task_generation: Option<i64>,
    pub(crate) current_definition: Option<String>,
}

#[cfg(test)]
impl FleetWithSession {
    /// An active fleet with no revision installed.
    pub(crate) fn unread(workspace_id: Uuid7) -> Self {
        Self {
            workspace_id,
            definition: None,
            status: sql::FLEET_STATUS_ACTIVE.to_owned(),
            runtime_model_ref: None,
            name: "Support triage".to_owned(),
            context_json: None,
            policy_semantics: None,
            revision_id: None,
            head_revision_id: None,
            task_generation: None,
            current_definition: None,
        }
    }

    /// An active fleet with the fewest-fields setup installed at `revision`,
    /// for a unit test whose pass must get past the setup.
    pub(crate) fn installed(workspace_id: Uuid7, revision: Uuid7) -> Self {
        Self {
            definition: Some(MINIMAL_DEFINITION.to_owned()),
            runtime_model_ref: Some("fixture-model".to_owned()),
            policy_semantics: Some(POLICY_SEMANTICS.to_owned()),
            revision_id: Some(revision.clone()),
            head_revision_id: Some(revision),
            ..Self::unread(workspace_id)
        }
    }
}

/// The fewest fields a setup needs, as the definition corpus's `minimal.json`.
#[cfg(test)]
const MINIMAL_DEFINITION: &str = r#"{
  "schema_version": 1,
  "description": "The fewest fields a setup needs.",
  "instructions_md": "You are a minimal agent. Instructions go here.",
  "config": {
    "name": "minimal-setup",
    "x-agentsfleet": {
      "triggers": [{ "type": "cron", "schedule": "0 9 * * *" }],
      "tools": ["agentmail"],
      "budget": { "daily_nanos": "1000000000" }
    }
  }
}"#;
