//! The crossings whose secret belongs to the DEPLOYMENT rather than a fleet.
//!
//! # Why these are together and apart from [`super::verify`]
//!
//! That file's crossing resolves a fleet first and reads a secret through the
//! binding it found. These two cannot: an App signs every installation's
//! deliveries with one secret that belongs to this deployment, and an approval
//! callback is answered before anything has looked up which fleet's gate it
//! resolves. Both therefore read the platform admin workspace, and neither
//! produces a [`super::verify::Verified`] — there is no binding to put in one.
//!
//! # A deployment with no admin workspace refuses every one of these
//!
//! Fail-closed, and not a degradation: without the secret there is nothing to
//! check a signature against, and accepting unverified deliveries on a public
//! endpoint would be strictly worse than serving none. `approval.zig` reaches
//! the same conclusion from its own side — *"No signing secret configured —
//! reject (fail-closed, no insecure fallback)"*.

use std::sync::Arc;

use afd_connector::Provider;
use afd_crypto::secret::SecretBytes;
use afd_webhook::Scheme;
use afd_webhook::{Refusal as WallRefusal, Verdict};
use axum::body::Bytes;
use http::HeaderMap;

use super::verify::{ProvenApp, header, wall};
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _, WorkspaceConnectors as _};

/// The scoped event a failed platform-secret read is logged under.
const EVENT_PLATFORM: &str = "webhook_platform_secret_failed";

/// The header an approval callback carries its proof in.
///
/// `approval.zig`'s `x-signature`, kept byte-for-byte: the sender is a Slack
/// app an operator configured against the running daemon, and a header name is
/// a wire contract that cannot change during a cutover.
pub const HEADER_APPROVAL_SIGNATURE: &str = "x-signature";

/// The header an approval callback carries its signed instant in.
pub const HEADER_APPROVAL_TIMESTAMP: &str = "x-signature-timestamp";

/// The vault key the approval signing secret is stored under.
pub(crate) const APPROVAL_IDENTITY: &str = "approval-signing";

/// Proves a delivery against a secret this DEPLOYMENT holds.
///
/// The five steps [`super::verify::verified`] takes, minus the fleet lookup:
/// resolve the admin workspace, open its secret, verify, and only then hand the
/// body back. The scheme and its two header names are parameters rather than
/// resolved from a source, because the two callers differ in exactly that — an
/// App delivery is read under the provider's own headers, and an approval
/// callback under the ones `approval.zig` published.
///
/// # Errors
/// `UZ-WH-020` for a deployment with no usable secret, `UZ-WH-010` for a
/// signature that did not match and `UZ-WH-011` for one outside its window.
async fn proven<D: Services>(
    services: &Arc<D>,
    scheme: Scheme,
    secret_key: &str,
    signature_header: &str,
    timestamp_header: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProvenApp, Refusal> {
    let Some(admin) = services.platform_admin_workspace() else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    let Some(secret) = services
        .ingress()
        .platform_secret(admin, secret_key)
        .await
        .map_err(Refusal::at(EVENT_PLATFORM))?
    else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    checked(
        services,
        scheme,
        &secret,
        signature_header,
        timestamp_header,
        headers,
        body,
    )
}

/// The verdict half, once the secret is in hand.
///
/// Split from [`proven`] because the two callers below differ only in WHERE the
/// secret comes from — the ingress seam's platform bag for an App, the
/// connector seam's `<provider>-app` bag for Slack — and nothing else. Keeping
/// the header read and the verdict mapping in one place is what stops the two
/// from drifting into answering the same forged signature differently.
fn checked<D: Services>(
    services: &Arc<D>,
    scheme: Scheme,
    secret: &SecretBytes,
    signature_header: &str,
    timestamp_header: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProvenApp, Refusal> {
    let presented = header(headers, signature_header);
    let timestamp = timestamp_header.and_then(|name| header(headers, name));

    match scheme.verify_at(
        secret,
        presented,
        timestamp,
        &body,
        services.now().as_seconds(),
    ) {
        Verdict::Verified => Ok(ProvenApp { body }),
        Verdict::Refused(refusal) => Err(wall(refusal)),
    }
}

/// Proves a connector's inbound delivery against this deployment's app secret.
///
/// Provider-general: the scheme comes from
/// [`Scheme::for_source`] over the provider's own id — the same resolution
/// [`verified_app`] uses — and the secret from the connector seam by provider.
/// Nothing here is per-vendor, so a connector that starts delivering events is
/// a [`afd_webhook::Scheme`] arm and a registry arm, not a second copy of this.
///
/// # Why the secret comes from the connector seam and not the ingress one
///
/// A connector's signing secret is a FIELD of its `<provider>-app` vault bag —
/// the same document the OAuth client id and secret live in, because one
/// registration at the vendor produces all of them. [`proven`]'s reader looks
/// for `webhook_secret` in a bag of its own, which that document does not
/// carry, so pointing it here would fail closed on a correctly configured
/// deployment. `afd_connector::app` is the one reader of that bag on either
/// daemon, which is what stops an operator being able to rotate half an app.
///
/// The headers are the scheme's OWN, unlike [`verified_approval`] which reuses
/// the construction under headers this daemon publishes. Here the sender really
/// is the vendor, so the header names are theirs.
///
/// # Errors
/// `UZ-WH-020` for a deployment with no admin workspace, no app for this
/// connector, an app carrying no signing secret, or a provider this daemon
/// ships no signature scheme for; `UZ-WH-010` for a signature that did not
/// match and `UZ-WH-011` for one outside its window.
pub(crate) async fn verified_connector_events<D: Services>(
    services: &Arc<D>,
    provider: Provider,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProvenApp, Refusal> {
    // Before the vault, as in [`verified_app`]: reading a secret for a provider
    // whose deliveries cannot be verified either way spends a decrypt on a
    // doomed request, and lets a probe measure the difference.
    let Some(scheme) = Scheme::for_source(provider.id()) else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    let Some(admin) = services.platform_admin_workspace() else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    let Some(secret) = services
        .connectors()
        .signing_secret(admin, provider)
        .await
        .map_err(Refusal::at(EVENT_PLATFORM))?
    else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    checked(
        services,
        scheme,
        &secret,
        scheme.signature_header(),
        scheme.timestamp_header(),
        headers,
        body,
    )
}

/// Proves an App delivery against the deployment's own App secret.
///
/// # Errors
/// As [`proven`], plus `UZ-WH-020` for a provider this daemon ships no scheme
/// for — refused before the vault is asked, so a probe cannot measure the
/// decrypt a doomed delivery would have cost.
pub(crate) async fn verified_app<D: Services>(
    services: &Arc<D>,
    source: &str,
    secret_key: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProvenApp, Refusal> {
    let Some(scheme) = Scheme::for_source(source) else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    proven(
        services,
        scheme,
        secret_key,
        scheme.signature_header(),
        scheme.timestamp_header(),
        headers,
        body,
    )
    .await
}

/// Proves an approval callback against the deployment's approval secret.
///
/// Verified under [`Scheme::SlackV0`], which is not an approximation: the
/// construction `approval.zig` computes is `v0` `:` timestamp `:` body behind a
/// `v0=` prefix, byte-for-byte the Slack scheme, because the sender IS Slack —
/// an interactive payload from a button an approver pressed. Only the header
/// names differ, and this daemon publishes its own for them.
///
/// Reusing the scheme rather than declaring a fourth is the whole point: a
/// second copy of the same construction is a second thing to get wrong, and the
/// `scheme_matrix` suite already proves this one against its fixtures.
///
/// # Errors
/// As [`proven`].
pub(crate) async fn verified_approval<D: Services>(
    services: &Arc<D>,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<ProvenApp, Refusal> {
    proven(
        services,
        Scheme::SlackV0,
        APPROVAL_IDENTITY,
        HEADER_APPROVAL_SIGNATURE,
        Some(HEADER_APPROVAL_TIMESTAMP),
        headers,
        body,
    )
    .await
}
