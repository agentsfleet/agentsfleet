//! The Svix crossing, which is the same order over a different verifier.
//!
//! # Why this is not a [`Scheme`] variant
//!
//! Every scheme in [`afd_webhook::Scheme`] answers "is this hex digest the tag
//! over these bytes". Svix answers a wider question: its header carries a SET
//! of `v1,<base64>` entries, because a secret roll signs with both the old key
//! and the new one and a receiver that accepted only one would go down for the
//! length of the roll. A verifier returning a verdict over a set does not fit a
//! table whose rows are one digest each, so it lives in
//! [`afd_webhook::vendor::svix`] and this file crosses to it.
//!
//! # The secret is a different stored SHAPE, not a different policy
//!
//! The HMAC family stores a JSON object and reads one field; Svix stores the
//! raw `whsec_…` string. [`WebhookIngress::svix_secret`] is the reader for the
//! second, and the parse that turns it into key material is
//! [`SvixSecret::parse`]'s — a secret that will not parse is `Unconfigured`,
//! never a failed verification, for the reason an absent one is.

use std::sync::Arc;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_webhook::vendor::svix::{self, SvixHeaders, SvixSecret};
use afd_webhook::{Refusal as WallRefusal, Verdict};
use axum::body::Bytes;
use http::HeaderMap;

use super::verify::{DETAIL_FLEET_NOT_FOUND, EVENT_BINDING, Verified, header, wall};
use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _};

/// Proves a Svix delivery, or answers the refusal it earned.
///
/// The order is [`super::verify::verified`]'s, step for step: resolve the
/// fleet, open the secret, verify, and only then hand the body back. What
/// changes is which reader opens the secret and which verifier decides — and
/// keeping the ORDER identical is what makes the two surfaces answer one
/// forged delivery the same way.
///
/// # Errors
/// `UZ-WH-001` for a fleet this daemon does not serve, `UZ-WH-020` for one
/// whose Svix secret is absent or unreadable, `UZ-WH-010` for a signature that
/// did not match and `UZ-WH-011` for one that arrived outside its window.
pub(crate) async fn verified_svix<D: Services>(
    services: &Arc<D>,
    fleet: &Uuid7,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Verified, Refusal> {
    let binding = services
        .ingress()
        .binding(fleet)
        .await
        .map_err(Refusal::at(EVENT_BINDING))?
        .ok_or_else(|| {
            Refusal::coded(error_code::WEBHOOK_FLEET_NOT_FOUND, DETAIL_FLEET_NOT_FOUND)
        })?;

    let Some(stored) = services
        .ingress()
        .svix_secret(&binding)
        .await
        .map_err(Refusal::at(EVENT_BINDING))?
    else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    let Some(secret) = std::str::from_utf8(stored.expose())
        .ok()
        .and_then(SvixSecret::parse)
    else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    // Absent headers resolve to empty rather than to a short-circuit, because
    // the vendored verifier already refuses an empty field as `Signature` — and
    // one place deciding that is one place to get it wrong.
    let presented = SvixHeaders {
        id: header(headers, svix::ID_HEADER).unwrap_or_default(),
        timestamp: header(headers, svix::TIMESTAMP_HEADER).unwrap_or_default(),
        signature: header(headers, svix::SIGNATURE_HEADER).unwrap_or_default(),
    };

    match svix::verify_at(&secret, presented, &body, services.now().as_seconds()) {
        Verdict::Verified => Ok(Verified { binding, body }),
        Verdict::Refused(refusal) => Err(wall(refusal)),
    }
}
