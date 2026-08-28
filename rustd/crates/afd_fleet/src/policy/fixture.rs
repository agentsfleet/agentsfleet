//! Fixtures the policy modules' unit tests share.
//!
//! A stored fleet document and a resolved provider are what every assertion in
//! this directory starts from, and each test module was building its own. The
//! copies were identical, which is the problem: a document literal repeated per
//! module is one place per module a schema change has to be found, and nothing
//! fails when only some of them are.

#![expect(
    clippy::expect_used,
    reason = "a fixture asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::Mode;
use afd_fleet_runtime::provider::StaticRegistry;

use crate::provider::{Dialled, Resolved, SecretString};
use afd_billing::rates::Posture;

/// The provider key a resolution carries, so a test can assert it travelled.
pub(super) const KEY: &str = "sk-the-billed-key";

/// The context ceiling a resolved provider carries.
///
/// Deliberately NOT a tier boundary: what the fixtures prove is that the cap is
/// INHERITED, and a value that also happens to select a tier would let a
/// tiering bug pass as an inheritance success.
pub(super) const RESOLVED_CAP_TOKENS: u32 = 256_000;

/// A stored config with `runtime` merged into the `x-agentsfleet` block.
///
/// # Panics
/// When `runtime` does not compose into a document the stored mode resolves.
pub(super) fn config(runtime: &str) -> FleetConfig {
    let document = format!(
        r#"{{"name":"probe","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],
           "budget":{{"daily_dollars":1.0}}{runtime}}}}}"#
    );
    FleetConfig::parse(&document, Mode::Stored, &StaticRegistry::default())
        .expect("a stored document resolves")
}

/// A resolved platform provider, optionally dialling a custom endpoint.
pub(super) fn provider(endpoint: Option<Dialled>) -> Resolved {
    resolved_with("some-model", 0, endpoint)
}

/// A resolved provider pinning a model and a context ceiling.
pub(super) fn resolved_with(model: &str, cap_tokens: u32, endpoint: Option<Dialled>) -> Resolved {
    Resolved::new(
        Posture::Platform,
        "anthropic".into(),
        model.into(),
        cap_tokens,
        endpoint,
        SecretString::new(KEY.to_owned()),
    )
}
