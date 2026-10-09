//! `/v1/connectors/**` and a workspace's `/connectors/**` — connecting to a
//! third party, and reading what is connected.
//!
//! Split by what a route DOES rather than by provider: [`catalogue`] lists what
//! could be connected, [`connect`] starts a round-trip, [`callback`] finishes
//! one, [`landing`] says where the person goes afterwards, and [`status`]
//! reads or forgets what landed. Per-provider difference
//! lives in `afd_connector`'s registry and nowhere here — adding a connector is
//! an arm in that crate's matches, never a file in this directory.
//!
//! # The provider segment is parsed once, at the edge
//!
//! Every handler below takes a [`Provider`], never a `&str`. An unknown
//! provider is answered once, by `provider_of`, so no handler has to remember
//! to refuse a miss, and the enum is what travels inward.
//!
//! # A refusal names the provider to the OPERATOR, not to the person
//!
//! The sentence a person reads and the line an operator greps are different
//! strings: the person gets the registry code and one provider-neutral detail,
//! and `provider` is a field on the `tracing` event. Nothing has to build a
//! sentence per provider to say what the log already says structurally.

pub(crate) mod callback;
pub(crate) mod catalogue;
pub(crate) mod connect;
mod landing;
pub(crate) mod status;

use std::sync::Arc;

// Aliased: this module has a `callback` of its own — the two HANDLERS — and the
// crate's is where the URLs those handlers travel through are composed.
use afd_core::error_code;
use afd_crypto::secret::SecretBytes;

use super::Refusal;
pub(crate) use super::provider_of;
use crate::services::{APPROVAL_IDENTITY, Services, WebhookIngress as _};

/// The scoped event a failed connector read is logged under.
pub(crate) const EVENT_READ: &str = "connector_read_failed";

/// The scoped event a failed connector write is logged under.
pub(crate) const EVENT_WRITE: &str = "connector_write_failed";

/// The scoped event a failed state-secret read is logged under.
const EVENT_SECRET: &str = "connector_state_secret_failed";

/// The refusal a provider this deployment has not been set up for earns.
///
/// An operator's fault rather than a tenant's, which is why
/// [`error_code::CONNECTOR_NOT_CONFIGURED`] is a 503.
pub(crate) const DETAIL_NOT_CONFIGURED: &str = "Connector is not configured";

/// What this deployment signs connector install states with.
///
/// The SAME secret the approval callback is verified against, the platform
/// secret [`APPROVAL_IDENTITY`] names, serving both surfaces. One secret because
/// there is one deployment-level HMAC key, and a second name for it would be a
/// second thing for an operator to rotate.
///
/// # Errors
/// `UZ-CONN-001` for a deployment holding no such secret, and the datastore's
/// own refusal when the vault would not answer. Fail-closed: without a key
/// there is nothing to sign a state with, and minting an unsigned one would be
/// strictly worse than refusing the connect.
pub(crate) async fn state_secret<D: Services>(services: &Arc<D>) -> Result<SecretBytes, Refusal> {
    let Some(admin) = services.platform_admin_workspace() else {
        return Err(unconfigured());
    };
    services
        .ingress()
        .platform_secret(admin, APPROVAL_IDENTITY)
        .await
        .map_err(Refusal::at(EVENT_SECRET))?
        .ok_or_else(unconfigured)
}

/// The refusal a deployment that cannot connect this provider answers.
///
/// Named once because four call sites raise it — a missing admin workspace, a
/// missing signing secret, a missing app bag at the start of a connect, and the
/// same bag missing when one is finished.
pub(crate) fn unconfigured() -> Refusal {
    Refusal::coded(error_code::CONNECTOR_NOT_CONFIGURED, DETAIL_NOT_CONFIGURED)
}
