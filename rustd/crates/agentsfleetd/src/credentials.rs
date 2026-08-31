//! The credential broker, assembled here and nowhere else.
//!
//! The composition root's half of `afd_credential::credential`: that module knows
//! how to mint, cache and single-flight, and this one knows where this
//! deployment's own App and OAuth clients are kept and which HTTP client the
//! exchanges go out through. Nothing else in the daemon names either.
//!
//! # Platform credentials are vault ROWS, not environment knobs
//!
//! `serve_broker.zig`'s rule, kept: each connector's platform credential is one
//! row in the admin workspace — `github-app`, `zoho-app`, and so on — so adding
//! a connector adds a row an operator writes through the product, never a
//! deployment variable, a schema change, or an edit here. The one knob is WHICH
//! workspace holds them.
//!
//! # An unconfigured connector is not a boot refusal
//!
//! The rule [`crate::bundles`] and [`crate::identity::resolve`] both follow. A
//! deployment that connected only GitHub is an ordinary deployment; one that
//! connected nothing still serves every other verb. A mint for a connector this
//! deployment holds no credential for answers `UZ-CRED-002` — one endpoint
//! refusing — rather than taking a healthy runner plane down at boot.

use std::sync::Arc;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::secrets::Registry;
use afd_credential::vault::Vault;

/// How long one vendor token exchange may take.
///
/// A deadline at the call site, per Invariant 4, and this is the call site: the
/// broker maps any failure to a transient refusal, so what it needs from a hung
/// token endpoint is a BOUND, not a taxonomy. Ten seconds, matching
/// `serve_broker.zig`'s `MINT_DEADLINE_MS` — a cold-cache round trip to a
/// vendor, not an interactive one.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);

/// Builds the broker this deployment mints through.
///
/// Answers a broker holding NOTHING when no admin workspace is configured, and
/// when its rows are absent or unreadable. Both are the same thing from a
/// runner's side — no credential is minted — and the log line is what separates
/// them for an operator.
pub async fn resolve(vault: &Vault, admin_workspace: Option<&Uuid7>) -> Arc<Broker> {
    let Some(workspace) = admin_workspace else {
        tracing::info!(
            event = "credential_broker_static_only",
            "no platform admin workspace is set; no on-demand credentials will be minted"
        );
        return broker(Platform::empty());
    };
    let platform = Platform::load(vault, workspace).await;
    broker(platform)
}

/// The broker over whatever this deployment turned out to hold.
///
/// Shared by both paths above, which is the point: a deployment with no admin
/// workspace and one whose rows would not open must produce the SAME broker —
/// one that resolves every connector and mints for none of them — and two
/// construction sites would be two chances for them to diverge.
fn broker(platform: Platform) -> Arc<Broker> {
    Arc::new(Broker::new(
        Arc::new(Registry::default()),
        Arc::new(Vendors::new(platform, vendor_exchange_client())),
    ))
}

/// The client every refresh exchange is posted through.
///
/// One client per outbound SURFACE, because that is what a connection pool is:
/// a client per mint would open a TLS session per mint, on a path whose whole
/// design is to avoid the round trip.
///
/// The connector token exchange takes one of these too — the same kind of call,
/// against the same vendors, under the same bound — and takes its OWN, because
/// the broker holds this one privately and a shared pool would let a slow
/// connect exchange consume the slots a credential mint needs.
///
/// A builder that will not build cannot happen here — no TLS backend is
/// selected at runtime in this workspace — but it is not worth a panic to say
/// so, so the default client stands in and the broker's own refusals cover a
/// client that cannot reach a vendor.
pub(crate) fn vendor_exchange_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(EXCHANGE_TIMEOUT)
        // The vendor is reached directly. An environment proxy silently
        // rerouting the exchange that mints a tenant's credential is not a
        // convenience worth having — the same call `afd_identity`'s key-set
        // fetcher makes, for the same reason.
        .no_proxy()
        .build()
        .unwrap_or_default()
}
