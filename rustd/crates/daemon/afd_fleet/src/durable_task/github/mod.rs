//! Native GitHub access shares the run's grants, binding and credential mint.

#[cfg(test)]
mod double;
mod http;
mod preflight;
mod publish;
pub(super) mod recovery;
pub(in crate::durable_task) mod status;
mod tree;
mod types;
mod workflow;

use super::DurableTaskRun;
use crate::{Result, run::RunService};
use af_core::clock;
use af_wire::credentials::MintCredentialRequest;
use std::borrow::Cow;

pub(super) use publish::Reconciled;
pub(super) use types::{BoundPr, Workflow};

/// Trusted publisher transport; credentials never reach the coding model.
#[derive(Debug, Clone)]
pub struct Github {
    plane: RunService,
    client: reqwest::Client,
    master: std::sync::Arc<af_crypto::secret::Kek>,
}

impl Github {
    pub(super) async fn hold_authority(
        connection: &mut sqlx::PgConnection,
        scope: &super::DurableTaskScope,
        installation: u64,
    ) -> Result<()> {
        if !afd_gate::gate::Gates::hold_approved_on(connection, &scope.fleet_id, "github").await?
            || !crate::run::Runs::hold_install_on(
                connection,
                &scope.workspace_id,
                &installation.to_string(),
            )
            .await?
        {
            return Err(super::DurableTaskRefusal::InvalidTarget.into());
        }
        Ok(())
    }
    /// Reuses the daemon's guarded client and current credential plane.
    #[must_use]
    pub const fn new(
        plane: RunService,
        client: reqwest::Client,
        master: std::sync::Arc<af_crypto::secret::Kek>,
    ) -> Self {
        Self {
            plane,
            client,
            master,
        }
    }

    pub(super) async fn session(&self, run: DurableTaskRun<'_>) -> Result<http::Session<'_>> {
        self.session_with(run, None).await
    }

    pub(super) async fn read_session(
        &self,
        run: DurableTaskRun<'_>,
        repository: &str,
    ) -> Result<http::Session<'_>> {
        self.session_with(run, Some(repository)).await
    }

    async fn session_with(
        &self,
        run: DurableTaskRun<'_>,
        repository: Option<&str>,
    ) -> Result<http::Session<'_>> {
        let request = MintCredentialRequest {
            run_id: Cow::Borrowed(run.run_id.as_str()),
            integration: Cow::Borrowed("github"),
            scope: None,
        };
        let credential = if let Some(repository) = repository {
            self.plane
                .mint_durable_task_reader(run.runner_id, &request, clock::now(), repository)
                .await?
        } else {
            self.plane
                .mint_durable_task_publisher(run.runner_id, &request, clock::now())
                .await?
        };
        let scope = self
            .plane
            .runs
            .mint_scope(run.runner_id, run.run_id)
            .await?
            .ok_or(super::DurableTaskRefusal::StaleState)?;
        let handle = self
            .plane
            .vault
            .find(&scope.workspace_id, "github")
            .await?
            .ok_or(super::DurableTaskRefusal::InvalidTarget)?;
        let handle: serde_json::Value =
            serde_json::from_slice(handle.expose()).map_err(af_core::json::Unreadable::from)?;
        let installation = afd_credential::credential::github::installation(&handle)
            .ok_or(super::DurableTaskRefusal::InvalidTarget)?;
        if !self
            .plane
            .runs
            .holds_install(&scope.workspace_id, "github", &installation.to_string())
            .await?
        {
            return Err(super::DurableTaskRefusal::InvalidTarget.into());
        }
        Ok(http::Session::new(
            &self.client,
            credential,
            installation,
            repository.is_some(),
        ))
    }
}
