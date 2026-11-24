//! Fixtures the policy modules' unit tests share.
//!
//! A resolved provider and a fleet's approved grants are what every assertion
//! in this directory starts from, spelled once so a shape change is found in
//! one place.

use af_crypto::secret::SecretString;
use afd_api_wire::grant::kind;
use afd_billing::rates::Posture;
use afd_credential::provider::{Dialled, Resolved};

use crate::policy::approved::{Approved, ApprovedGrant};

/// The provider key a resolution carries, so a test can assert it travelled.
pub(super) const KEY: &str = "sk-the-billed-key";

/// The context ceiling a resolved provider carries.
///
/// Deliberately NOT a tier boundary: what the fixtures prove is that the cap is
/// INHERITED, and a value that also happens to select a tier would let a
/// tiering bug pass as an inheritance success.
pub(super) const RESOLVED_CAP_TOKENS: u32 = 256_000;

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

/// A fleet whose approved grants are `secrets` as secrets, `integrations` as
/// integrations, and GitHub under `github_scope` when one is given.
pub(super) fn approved(
    secrets: &[&str],
    integrations: &[&str],
    github_scope: Option<&str>,
) -> Approved {
    let grant = |name: &str, kind: &str, scope: Option<&str>| ApprovedGrant {
        name: name.to_owned(),
        kind: kind.to_owned(),
        scope: scope.map(str::to_owned),
    };
    secrets
        .iter()
        .map(|name| grant(name, kind::SECRET, None))
        .chain(
            integrations
                .iter()
                .map(|name| grant(name, kind::INTEGRATION, None)),
        )
        .chain(github_scope.map(|scope| grant("github", kind::INTEGRATION, Some(scope))))
        .collect()
}
