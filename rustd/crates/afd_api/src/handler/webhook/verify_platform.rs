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

use afd_webhook::Scheme;
use afd_webhook::{Refusal as WallRefusal, Verdict};
use axum::body::Bytes;
use http::HeaderMap;

use super::verify::{ProvenApp, header, wall};
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _};

/// The scoped event a failed platform-secret read is logged under.
const EVENT_PLATFORM: &str = "webhook_platform_secret_failed";

/// The header an approval callback carries its proof in.
///
/// `approval.zig`'s `x-signature`, kept byte-for-byte: the sender is a Slack
/// app an operator configured against the running daemon, and a header name is
/// a wire contract that cannot change during a cutover.
pub(crate) const HEADER_APPROVAL_SIGNATURE: &str = "x-signature";

/// The header an approval callback carries its signed instant in.
pub(crate) const HEADER_APPROVAL_TIMESTAMP: &str = "x-signature-timestamp";

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

    let presented = header(headers, signature_header);
    let timestamp = timestamp_header.and_then(|name| header(headers, name));

    match scheme.verify_at(
        &secret,
        presented,
        timestamp,
        &body,
        services.now().as_seconds(),
    ) {
        Verdict::Verified => Ok(ProvenApp { body }),
        Verdict::Refused(refusal) => Err(wall(refusal)),
    }
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
