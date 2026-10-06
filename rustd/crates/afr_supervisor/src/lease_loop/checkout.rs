//! The fleet's bound repositories, checked out into the lease's workspace from
//! the host side before the turn runs, so the token that fetched them never
//! enters the sandbox (`crate::workspace_clone`).

use afd_core::error_code::Coded;
use afd_core::id::Uuid7;
use afd_wire::report::FailureClass;
use afr_sandbox::Sandbox;
use afr_tools::sandbox::{CREDENTIAL_GITHUB, checkouts};

use super::{LeaseRun, failed};
use crate::credentials;
use crate::error::{self, Result};
use crate::report::Ending;
use crate::workspace_clone::Request;

/// What a lease whose repositories would not check out reports.
const DETAIL_CHECKOUT: &str = "a bound repository could not be checked out into the workspace";
/// What an engine that keeps its workspace out of the host's reach is told.
const DETAIL_NO_HOST_WORKSPACE: &str =
    "the sandbox engine offers no host path to check a repository out into";
const EVENT_CHECKOUT_REFUSED: &str = "repository_checkout_refused";
const EVENT_CHECKOUT_STARTED: &str = "repository_checkout_started";
const EVENT_CHECKOUT_COMPLETED: &str = "repository_checkout_completed";
const EVENT_CHECKOUT_FAILED: &str = "repository_checkout_failed";

impl LeaseRun<'_> {
    /// Checks out every repository the lease's policy binds into `sandbox`'s
    /// workspace when an offered tool runs in the sandbox, and nothing otherwise.
    /// Anything that stops a checkout ends the lease at startup, before any
    /// tool runs. The ending is boxed: it is large, and only a failure
    /// allocates one.
    pub(super) async fn check_out(&self, sandbox: &dyn Sandbox) -> Result<(), Box<Ending>> {
        let wanted = checkouts(&self.lease.policy).map_err(|refusal| self.unready(&refusal))?;
        if wanted.is_empty() {
            return Ok(());
        }
        let workspace = sandbox
            .workspace()
            .ok_or_else(|| self.unready(&error::config(DETAIL_NO_HOST_WORKSPACE)))?;
        // The workspace id becomes a directory of the mirror's path, so it is
        // read as an identifier before it is joined onto anything.
        let scope = Uuid7::parse(&self.lease.event.workspace_id)
            .map_err(|refusal| self.unready(&refusal))?;
        let token = credentials::mint(&self.lessee.plane, &self.ids.lease, CREDENTIAL_GITHUB, None)
            .await
            .map_err(|refusal| self.unready(&refusal))?;
        for checkout in wanted {
            self.check_out_one(Request {
                scope: scope.as_str(),
                checkout,
                token: token.expose(),
                workspace,
            })
            .await?;
        }
        Ok(())
    }

    /// Checks out one repository, logging it from start to end.
    async fn check_out_one(&self, request: Request<'_>) -> Result<(), Box<Ending>> {
        let lease_id = self.ids.lease.as_str();
        let repository = request.checkout.repository;
        let event = EVENT_CHECKOUT_STARTED;
        tracing::info!(lease_id, repository, event);
        match self
            .lessee
            .mirrors
            .check_out(request, &self.interrupt)
            .await
        {
            Ok(fetched) => {
                let fetched = fetched.as_str();
                let event = EVENT_CHECKOUT_COMPLETED;
                tracing::info!(lease_id, repository, fetched, event);
                Ok(())
            }
            Err(failure) => {
                let code = failure.code().as_str();
                let detail = DETAIL_CHECKOUT;
                let reason = failure.to_string();
                let event = EVENT_CHECKOUT_FAILED;
                tracing::warn!(
                    error_code = code,
                    lease_id,
                    repository,
                    reason,
                    event,
                    detail
                );
                Err(Box::new(failed(
                    FailureClass::StartupPosture,
                    DETAIL_CHECKOUT,
                )))
            }
        }
    }

    /// Logs why the lease's repositories could not be checked out, and ends
    /// it at startup.
    fn unready(&self, failure: &impl Coded) -> Box<Ending> {
        Box::new(self.refuse(failure, EVENT_CHECKOUT_REFUSED, DETAIL_CHECKOUT))
    }
}

#[cfg(test)]
#[path = "checkout_tests.rs"]
mod tests;
