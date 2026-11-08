//! Native GitHub access shares the lease's grants, binding and credential mint.

mod http;
mod preflight;
mod publish;
pub(super) mod recovery;
pub(in crate::task) mod status;
mod tree;
mod types;
mod workflow;

use super::TaskLease;
use crate::{Result, lease::Plane};
use af_core::clock;
use af_wire::credentials::MintCredentialRequest;
use std::borrow::Cow;

pub(super) use publish::{ReceiptIdentity, Reconciled};
pub(super) use types::{BoundPr, Workflow};

/// Trusted publisher transport; credentials never reach the coding model.
#[derive(Debug, Clone)]
pub struct Github {
    plane: Plane,
    client: reqwest::Client,
    master: std::sync::Arc<af_crypto::secret::Kek>,
}

impl Github {
    pub(super) async fn hold_authority(
        connection: &mut sqlx::PgConnection,
        scope: &super::TaskScope,
        installation: u64,
    ) -> Result<()> {
        if !afd_gate::gate::Gates::hold_approved_on(connection, &scope.fleet_id, "github").await?
            || !crate::lease::Leases::hold_install_on(
                connection,
                &scope.workspace_id,
                &installation.to_string(),
            )
            .await?
        {
            return Err(super::TaskRefusal::InvalidTarget.into());
        }
        Ok(())
    }
    /// Reuses the daemon's guarded client and current credential plane.
    #[must_use]
    pub const fn new(
        plane: Plane,
        client: reqwest::Client,
        master: std::sync::Arc<af_crypto::secret::Kek>,
    ) -> Self {
        Self {
            plane,
            client,
            master,
        }
    }

    pub(in crate::task) async fn dependency_session(
        &self,
        permit: super::ReadPermit,
    ) -> Result<http::Session<'_>> {
        let installation = permit.installation();
        let deadline = permit.deadline();
        let credential = self.plane.mint_wait_reader(&permit, clock::now()).await?;
        Ok(
            http::Session::new(&self.client, credential, installation, true)
                .with_deadline(deadline),
        )
    }

    pub(super) async fn session(&self, lease: TaskLease<'_>) -> Result<http::Session<'_>> {
        self.session_with(lease, None).await
    }

    pub(super) async fn read_session(
        &self,
        lease: TaskLease<'_>,
        repository: &str,
    ) -> Result<http::Session<'_>> {
        self.session_with(lease, Some(repository)).await
    }

    async fn session_with(
        &self,
        lease: TaskLease<'_>,
        repository: Option<&str>,
    ) -> Result<http::Session<'_>> {
        let request = MintCredentialRequest {
            lease_id: Cow::Borrowed(lease.lease_id.as_str()),
            integration: Cow::Borrowed("github"),
            scope: None,
        };
        let credential = if let Some(repository) = repository {
            self.plane
                .mint_task_reader(lease.runner_id, &request, clock::now(), repository)
                .await?
        } else {
            self.plane
                .mint_task_publisher(lease.runner_id, &request, clock::now())
                .await?
        };
        let scope = self
            .plane
            .leases
            .mint_scope(lease.runner_id, lease.lease_id)
            .await?
            .ok_or(super::TaskRefusal::StaleState)?;
        let handle = self
            .plane
            .vault
            .open(afd_credential::vault::KeyRef {
                workspace_id: &scope.workspace_id,
                name: "github",
            })
            .await?
            .ok_or(super::TaskRefusal::InvalidTarget)?;
        let handle: serde_json::Value =
            serde_json::from_slice(handle.expose()).map_err(af_core::json::Unreadable::from)?;
        let installation = afd_credential::credential::github::installation(&handle)
            .ok_or(super::TaskRefusal::InvalidTarget)?;
        if !self
            .plane
            .leases
            .holds_install(&scope.workspace_id, "github", &installation.to_string())
            .await?
        {
            return Err(super::TaskRefusal::InvalidTarget.into());
        }
        Ok(http::Session::new(
            &self.client,
            credential,
            installation,
            repository.is_some(),
        ))
    }
}
