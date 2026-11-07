//! The wall, as every route in this family crosses it.
//!
//! One function, because there is exactly one correct order and a second copy
//! of it is a second chance to get the order wrong: the fleet route and the
//! App ingress would drift on which refusal comes first.
//!
//! # The order, and what each step's absence would cost
//!
//! 1. **Resolve the fleet.** No row, or no webhook trigger ⇒ `UZ-WH-001`. Both
//!    are one answer: distinguishing them confirms a fleet id to a guesser.
//!    The binding already carries the recipe selected by the stored trigger.
//! 2. **Open the secret.** Absent, unparseable, or empty ⇒ `UZ-WH-020`.
//! 3. **Verify.** `afd_webhook` decides, in constant time.
//! 4. **Only now** is the body handed back to be parsed.
//!
//! Nothing before step 4 reads the body as anything but bytes to hash.

use std::sync::Arc;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_ingress::Binding;
use afd_webhook::vendor::svix;
use afd_webhook::{Family, Proof, Refusal as WallRefusal, Scheme, Verdict};
use axum::body::Bytes;
use http::HeaderMap;

use crate::handler::Refusal;
use crate::services::{Services, WebhookIngress as _};

/// The scoped event a failed resolution is logged under.
pub(super) const EVENT_BINDING: &str = "webhook_binding_failed";

/// The refusal a delivery to a fleet this daemon does not serve earns.
///
/// The sentence for `UZ-WH-001`. A provider's delivery log shows it to an
/// operator wiring up an integration.
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
    /// A replay key authenticated by the selected recipe.
    pub(crate) replay_id: String,
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
    source: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Verified, Refusal> {
    let binding = services
        .ingress()
        .binding(fleet, source)
        .await
        .map_err(Refusal::at(EVENT_BINDING))?
        .ok_or_else(|| {
            Refusal::coded(error_code::WEBHOOK_FLEET_NOT_FOUND, DETAIL_FLEET_NOT_FOUND)
        })?;

    let family = binding.family();

    let Some(secret) = services
        .ingress()
        .signing_secret(&binding)
        .await
        .map_err(Refusal::at(EVENT_BINDING))?
    else {
        return Err(wall(WallRefusal::Unconfigured));
    };

    let proof = proof_for(family, &binding, headers, &body);
    let id = proof.id;
    match family.verify_at(&secret, proof, services.now().as_second()) {
        Verdict::Verified => {
            // Svix signs this identifier. Other recipes authenticate the body.
            let replay_id = if family == Family::SvixV1 {
                id.unwrap_or_default().to_owned()
            } else {
                afd_ingress::replay_id(&body)
            };
            Ok(Verified {
                binding,
                body,
                replay_id,
            })
        }
        Verdict::Refused(refusal) => Err(wall(refusal)),
    }
}

/// Header names follow the stored recipe, regardless of other offered headers.
fn proof_for<'a>(
    family: Family,
    binding: &'a Binding,
    headers: &'a HeaderMap,
    body: &'a [u8],
) -> Proof<'a> {
    let (signature, timestamp, id) = match family {
        Family::BodyHexSha256 | Family::BodyHex | Family::BodyBase64Sha256 => (
            binding
                .signature_header()
                .and_then(|name| header(headers, name)),
            None,
            None,
        ),
        Family::SlackV0 => (
            header(headers, Scheme::SlackV0.signature_header()),
            Scheme::SlackV0
                .timestamp_header()
                .and_then(|name| header(headers, name)),
            None,
        ),
        Family::SvixV1 => (
            header(headers, svix::SIGNATURE_HEADER),
            header(headers, svix::TIMESTAMP_HEADER),
            header(headers, svix::ID_HEADER),
        ),
    };
    Proof {
        signature,
        timestamp,
        id,
        body,
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
