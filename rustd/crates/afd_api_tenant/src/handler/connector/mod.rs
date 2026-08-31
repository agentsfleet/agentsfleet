//! `/v1/connectors/**` and a workspace's `/connectors/**` — connecting to a
//! third party, and reading what is connected.
//!
//! Split by what a route DOES rather than by provider: [`catalogue`] lists what
//! could be connected, [`connect`] starts a round-trip, [`callback`] finishes
//! one, and [`status`] reads or forgets what landed. Per-provider difference
//! lives in `afd_connector`'s registry and nowhere here — adding a connector is
//! an arm in that crate's matches, never a file in this directory. The Zig
//! daemon carries `connectors/slack/`, `connectors/jira/`, `connectors/zoho/`,
//! `connectors/linear/` and `connectors/github/` for the same five, each with
//! its own callback file.
//!
//! # The provider segment is parsed once, at the edge
//!
//! Every handler below takes a [`Provider`], never a `&str`. `registry.lookup`
//! is re-run at the top of each Zig handler and each one has to remember to
//! answer `respondUnknown` when it misses; here the miss is one function and
//! the enum is what travels inward.
//!
//! # A refusal names the provider to the OPERATOR, not to the person
//!
//! `connect.zig` and `callback.zig` interpolate the display name into the
//! problem detail — *"Slack connect is not configured"* — because the sentence
//! a person reads and the line an operator greps are the same string there.
//! They are not here: the person gets the registry code, and `provider` is a
//! field on the `tracing` event. So the detail is the Zig's own fallback
//! wording, and nothing has to build a sentence per provider to say what the
//! log already says structurally.

pub(crate) mod callback;
pub(crate) mod catalogue;
pub(crate) mod connect;
pub(crate) mod status;

use std::sync::Arc;

use afd_connector::Provider;
// Aliased: this module has a `callback` of its own — the two HANDLERS — and the
// crate's is where the URLs those handlers travel through are composed.
use afd_connector::callback as connector_urls;
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
/// `callback.zig`'s `NOT_CONFIGURED_FALLBACK`. An operator's fault rather than
/// a tenant's, which is why [`error_code::CONNECTOR_NOT_CONFIGURED`] is a 503.
pub(crate) const DETAIL_NOT_CONFIGURED: &str = "Connector is not configured";

/// Where a provider sends the browser back, for this deployment.
///
/// Built from the dashboard's own base URL rather than from the request, so the
/// value a code is minted against and the value the exchange echoes are one
/// fact — see [`Services::dashboard`] and [`afd_connector::callback`].
///
/// # Errors
/// `UZ-CONN-001` for a dashboard base that is not a URL. A boot-time
/// misconfiguration, refused rather than sending somebody to a page that cannot
/// exist, and it answers as unconfigured because that is what it is.
pub(crate) fn relay_uri<D: Services>(
    services: &Arc<D>,
    provider: Provider,
) -> Result<String, Refusal> {
    connector_urls::relay_uri(services.dashboard(), provider).ok_or_else(|| {
        tracing::error!(
            provider = provider.id(),
            event = "connector_dashboard_base_unusable",
        );
        unconfigured()
    })
}

/// What this deployment signs connector install states with.
///
/// The SAME secret the approval callback is verified against, which is the
/// Zig's `approval_signing_secret` serving both surfaces. One secret because
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
