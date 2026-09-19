//! Asking, at install, for the grants the bundle says the fleet will need.
//!
//! # Why here and not at the first delivery
//!
//! A bundle declares credential NAMES; the workspace's stored handle for each
//! says whether it is a connector the daemon must MINT against, and minting
//! needs a standing human decision. Install is the moment both facts are
//! available and a person is present: they chose the fleet, which IS the
//! answer — so the grant lands approved here and no card is raised at all.
//!
//! The lease path still asks, as the backstop for a fleet installed before
//! this existed or one that declared a credential by a later PATCH. It asks at
//! the worst possible moment — nobody is watching a poll — which is the second
//! reason the design is here.
//!
//! # Best effort, and honest about it
//!
//! A failed write does not roll the install back. The fleet is real, its
//! stream exists, and every non-mintable thing it declares works; what it
//! lacks is a grant, which the delivery path asks for the first time it needs
//! one. Rolling back a usable fleet because a grant could not be written would
//! trade a recoverable gap for a lost install.

use afd_approval::{Origin, Wanted};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_credential::secrets::Mintable;

use crate::Fleets;

/// A grant the bundle's declaration authorised could not be written.
const EVENT_REQUEST_FAILED: &str = "install_grant_request_failed";

/// A declared credential could not be classified, so nothing was asked for.
const EVENT_CLASSIFY_FAILED: &str = "install_grant_classify_failed";

impl Fleets {
    /// Write one approved grant per mintable credential the bundle declared.
    ///
    /// Runs after the fleet is active, because the write references the fleet
    /// row by foreign key. Answers nothing: every outcome leaves the install
    /// successful, and what an operator needs is the log line saying which
    /// service was left without one.
    pub(super) async fn request_declared_grants(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        declared: &[String],
        now: UnixMillis,
    ) {
        for want in self.wanted_by(workspace, declared).await {
            let service = want.integration.as_ref();
            let request = Wanted {
                service,
                credential: want.name.as_ref(),
                origin: Origin::Install,
            };
            if let Err(unwritten) = self.grants.grant_at_install(fleet, request, now).await {
                let fleet_id = fleet.as_str();
                let reason = unwritten.to_string();
                tracing::warn!(
                    error_code = unwritten.code().as_str(),
                    event = EVENT_REQUEST_FAILED,
                    fleet_id,
                    service,
                    reason,
                    "the fleet installed without a grant for this integration; \
                     the first delivery that needs it asks for one"
                );
            }
        }
    }

    /// Which of `declared` must be minted, and through which connector.
    ///
    /// The stored values the read also resolves never leave this expression:
    /// the mintable half is a name and a connector id, and the [`Declared`]
    /// carrying the workspace's secrets beside it is dropped where it was
    /// built. That is the property this helper exists to hold — an install
    /// wants the classification, never the credentials.
    ///
    /// [`Declared`]: afd_credential::secrets::Declared
    async fn wanted_by(&self, workspace: &Uuid7, declared: &[String]) -> Vec<Mintable> {
        let names: Vec<&str> = declared.iter().map(String::as_str).collect();
        match self
            .vault
            .declared(workspace, &names, &self.connectors)
            .await
        {
            Ok(resolved) => resolved.mintable().to_vec(),
            // Every name here was proven stored moments ago, so this is a
            // datastore that stopped answering or a handle that is not an
            // object — neither of which the install should fail on, because the
            // fleet is already active and the delivery path asks again.
            Err(unreadable) => {
                let workspace_id = workspace.as_str();
                let reason = unreadable.to_string();
                tracing::warn!(
                    event = EVENT_CLASSIFY_FAILED,
                    workspace_id,
                    reason,
                    "the bundle's credentials could not be classified; no grant was written"
                );
                Vec::new()
            }
        }
    }
}
