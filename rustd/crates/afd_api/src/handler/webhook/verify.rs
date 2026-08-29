//! The wall, as every route in this family crosses it.
//!
//! One function, because there is exactly one correct order and a second copy
//! of it is a second chance to get the order wrong. `webhook_sig.zig` and
//! `github.zig` each carry their own copy today, which is why the fleet route
//! and the App ingress can drift on which refusal comes first.
//!
//! # The order, and what each step's absence would cost
//!
//! 1. **Resolve the fleet.** No row, or no webhook trigger ⇒ `UZ-WH-001`. Both
//!    are one answer: distinguishing them confirms a fleet id to a guesser.
//! 2. **Resolve the scheme.** A source this daemon ships no scheme for ⇒
//!    `UZ-WH-020`, never a pass. The Zig carries the same fail-closed note.
//! 3. **Open the secret.** Absent, unparseable, or empty ⇒ `UZ-WH-020`.
//! 4. **Verify.** `afd_webhook` decides, in constant time.
//! 5. **Only now** is the body handed back to be parsed.
//!
//! Nothing before step 5 reads the body as anything but bytes to hash.

use std::sync::Arc;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_ingress::Binding;
use afd_webhook::{Refusal as WallRefusal, Verdict};
use axum::body::Bytes;
use http::HeaderMap;

use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _};

/// The scoped event a failed resolution is logged under.
pub(super) const EVENT_BINDING: &str = "webhook_binding_failed";

/// The refusal a delivery to a fleet this daemon does not serve earns.
///
/// `error_entries.zig:133`'s sentence for `UZ-WH-001`, verbatim: a provider's
/// delivery log shows it to an operator wiring up an integration.
pub(super) const DETAIL_FLEET_NOT_FOUND: &str = "No fleet is registered for this webhook endpoint.";

/// A delivery that proved itself, with the fleet it proved itself to.
///
/// Constructible only by [`verified`], which is the guarantee: a route holding
/// one of these is holding a body the wall has already passed, and a route that
/// wants the body has no other way to get it.
#[derive(Debug)]
pub(crate) struct Verified {
    /// The fleet, its workspace, and the trigger the delivery was measured on.
    pub(crate) binding: Binding,
    /// The RAW bytes, exactly as received.
    ///
    /// Never re-serialized: the tag was computed over these, and a body that
    /// went through a parse and back is a different byte string that would not
    /// verify — which is why this is handed on rather than a parsed tree.
    pub(crate) body: Bytes,
}

/// Proves a delivery, or answers the refusal it earned.
///
/// # Errors
/// `UZ-WH-001` for a fleet this daemon does not serve, `UZ-WH-020` for one with
/// no usable secret, `UZ-WH-010` for a signature that did not match and
/// `UZ-WH-011` for one that arrived outside its window. Every internal failure —
/// a datastore that would not answer, a stored document that no longer parses —
/// is rendered from the ingress crate's own error rather than restated here.
pub(crate) async fn verified<D: Services>(
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

    // A source with no declared scheme is refused BEFORE the vault is asked.
    // Reading a secret first would spend a decrypt on a delivery that cannot be
    // verified either way, and would let a probe measure the difference.
    let Some(scheme) = binding.scheme() else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    let Some(secret) = services
        .ingress()
        .signing_secret(&binding)
        .await
        .map_err(Refusal::at(EVENT_BINDING))?
    else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    let presented = header(headers, scheme.signature_header());
    let timestamp = scheme
        .timestamp_header()
        .and_then(|name| header(headers, name));

    match scheme.verify_at(
        &secret,
        presented,
        timestamp,
        &body,
        services.now().as_seconds(),
    ) {
        Verdict::Verified => Ok(Verified { binding, body }),
        Verdict::Refused(refusal) => Err(wall(refusal)),
    }
}

/// A delivery the platform's own App secret proved.
///
/// Carries no [`Binding`], and that absence is the App ingress in one type: the
/// delivery has proved it came from the App, which says nothing yet about which
/// fleets it is for. Those are resolved AFTER this, from the payload's own
/// installation id. Constructible only by [`super::verify_platform`], for the same reason
/// [`Verified`] is.
#[derive(Debug)]
pub(crate) struct ProvenApp {
    /// The RAW bytes, exactly as received — see [`Verified::body`].
    pub(crate) body: Bytes,
}

/// One header's value, when it is one this daemon can read as text.
///
/// A header carrying bytes that are not visible ASCII resolves to `None` and so
/// to `Refusal::Signature` — the same answer an absent header earns, which is
/// correct: neither is a proof, and telling them apart narrows a forger's
/// search for no honest sender's benefit.
pub(super) fn header<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// The wall's own refusal, rendered.
///
/// The code and the sentence both come from [`WallRefusal`] rather than from
/// this call site, which is what keeps the two ingress families answering one
/// delivery the same way. `afd_webhook::verdict` owns the mapping and the
/// `scheme_matrix` suite pins it.
pub(super) fn wall(refusal: WallRefusal) -> Refusal {
    Refusal::coded(refusal.code(), refusal.detail())
}
