//! Signed deliveries, as a provider would present them.
//!
//! Every signature here is computed with the SAME primitive the wall verifies
//! with ([`HmacSha256Tag::compute_peppered`]) and carried under the SAME header
//! and prefix the wall reads ([`Scheme::signature_header`],
//! [`Scheme::prefix`]). A fixture that spelled `sha256=` for itself would still
//! pass on the day the scheme changed its prefix, and the suite would report
//! green over a daemon no provider could reach any more (RULE TFX).
//!
//! The two GitHub delivery headers below are the one exception, and they are
//! declared once here rather than per suite. Their production home is
//! `afd_api::handler::webhook`, which is `pub(crate)` — an integration test is
//! a separate crate and cannot see it. One fixture-side declaration shared by
//! every suite is the closest reachable thing to a single owner.

use afd_core::id::Uuid7;
use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use afd_fleet_lifecycle::FleetStatus;
use afd_ingress::Binding;
use afd_webhook::Scheme;
use http::HeaderName;

/// The fleet every signed-ingress fixture addresses.
pub(crate) const FLEET: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8e9f";

/// The workspace that fleet belongs to, and whose vault holds its secret.
pub(crate) const WORKSPACE: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8ea0";

/// A second fleet, for the fan-out cases that need more than one subscriber.
pub(crate) const OTHER_FLEET: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8ea1";

/// The shared secret a fixture delivery is signed with.
///
/// Never a real one, and never read from the environment: a suite that needed
/// a secret from somewhere could not run on a fresh clone.
pub(crate) const SECRET: &[u8] = b"fixture-webhook-shared-secret";

/// A secret that is not [`SECRET`], for the wrong-key half of the matrix.
pub(crate) const WRONG_SECRET: &[u8] = b"fixture-webhook-other-secret";

/// The header GitHub names the delivery's kind in — see the module note.
pub(crate) const HEADER_EVENT: &str = "x-github-event";

/// The header GitHub carries its own delivery identifier in.
pub(crate) const HEADER_DELIVERY: &str = "x-github-delivery";

/// A delivery identifier, as GitHub shapes one.
pub(crate) const DELIVERY_ID: &str = "72d3162e-cc78-11e3-81ab-4c9367dc0958";

/// The repository a fixture delivery reports on.
///
/// The `repository.full_name` every checked-in payload fixture carries, so a
/// subscription written against this constant reaches them and one written
/// against any other name does not.
pub(crate) const REPOSITORY: &str = "example/platform";

/// The App installation the App-delivery fixture arrived for.
///
/// A string because that is how it crosses
/// [`afd_api::services::WebhookIngress::installation_workspace`] — the column
/// it looks up is text, and the provider's own id is an integer, so the
/// conversion happens at the route and a fixture asserts on the converted form.
pub(crate) const INSTALLATION: &str = "48765123";

/// A trigger declaring GitHub with no allow-list — every event, no repository.
pub(crate) const TRIGGER_GITHUB: &str = r#"[{"type":"webhook","source":"github"}]"#;

/// A stored document carrying `triggers` and nothing else varying.
///
/// The surrounding keys are the ones [`afd_fleet_runtime::FleetConfig::stored`]
/// requires. Built here rather than loaded from a file so a reader sees the
/// whole input beside the assertion — the shape
/// `afd_ingress::binding::tests` already uses for the same reason.
pub(crate) fn document(triggers: &str) -> String {
    format!(
        r#"{{
          "name": "ingress-fixture",
          "x-agentsfleet": {{
            "triggers": {triggers},
            "tools": ["bash"],
            "budget": {{ "daily_dollars": 1.0 }}
          }}
        }}"#
    )
}

/// The binding an active fleet declaring `triggers` resolves to.
pub(crate) fn binding(triggers: &str) -> Binding {
    binding_of(FLEET, triggers, FleetStatus::Active.as_str())
}

/// The same, for a fleet in a state a test names.
pub(crate) fn binding_with_status(triggers: &str, status: &str) -> Binding {
    binding_of(FLEET, triggers, status)
}

/// The same, for a fleet a test identifies.
pub(crate) fn binding_of(fleet: &str, triggers: &str, status: &str) -> Binding {
    Binding::stored(id(fleet), id(WORKSPACE), status, &document(triggers), None)
        .expect("the fixture document parses and its status is one this build knows")
        .expect("the fixture document declares a webhook trigger")
}

/// A fixture identifier, parsed once so no test spells the check twice.
pub(crate) fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("the fixture identifiers are canonical")
}

/// The signature header value proving `body` was signed with `secret`.
///
/// Built through the scheme rather than beside it — see the module note.
pub(crate) fn signature(scheme: Scheme, secret: &[u8], body: &[u8]) -> String {
    let tag = HmacSha256Tag::compute_peppered(&SecretBytes::new(secret.to_vec()), &[body]);
    format!("{}{}", scheme.prefix(), tag.to_hex())
}

/// A GitHub delivery's headers: what it is, which delivery, and its proof.
///
/// Returned owned because [`Scheme::signature_header`] answers a `&'static str`
/// while the signature is computed per call, and a caller wants one array
/// rather than three bindings it has to keep alive itself.
pub(crate) fn github_headers<'d>(
    event: &'d str,
    delivery: &'d str,
    signature: &'d str,
) -> Vec<(HeaderName, &'d str)> {
    vec![
        (name(HEADER_EVENT), event),
        (name(HEADER_DELIVERY), delivery),
        (name(Scheme::BodyHex.signature_header()), signature),
    ]
}

/// One header name, as the request builder takes it.
pub(crate) fn name(header: &str) -> HeaderName {
    HeaderName::from_bytes(header.as_bytes()).expect("the fixture header names are well formed")
}
