//! Reading one knob at a time, and saying which one was wrong.
//!
//! The helpers `preflight` composes: each takes the environment and the fault
//! list, answers `Option`, and pushes a fault when a knob was SET and unusable.
//! That split — absent is a decision, present-and-broken is a fault — is the
//! whole reason these are functions rather than a chain of `?`: a daemon must
//! be able to boot without a snapshot store and must not be able to boot with
//! half of one.

use std::fmt;

use afd_core::env::EnvSource;
use afd_crypto::secret::Kek;
use afd_identity::ProviderSecret;

use super::{
    BundleStoreConfig, ENCRYPTION_MASTER_KEY_KNOB, Fault, IdentityConfig, OIDC_AUDIENCE_KNOB,
    OIDC_ISSUER_KNOB, OIDC_JWKS_URL_KNOB, PROVIDER_API_BASE_KNOB, PROVIDER_SECRET_KNOB, R2_KNOBS,
    WHY_API_BASE, WHY_AUDIENCE, WHY_ISSUER, WHY_KEK, WHY_R2, WHY_SECRET,
};

/// Resolves the snapshot store, which a boot may legitimately not have.
///
/// Three outcomes, not two. All four knobs set is a store; none set is a
/// deployment that serves no snapshots, which is not a fault and pushes none.
/// SOME set is a fault per missing knob, and that is the case this function
/// exists for: a half-configured store boots fine and then fails at the first
/// bundle fetch, which is the furthest possible point from the mistake — the
/// same rule `cmd/doctor.zig` records for a half-configured identity provider.
pub(super) fn bundle_store<E: EnvSource + ?Sized>(
    env: &E,
    faults: &mut Vec<Fault>,
) -> Option<BundleStoreConfig> {
    if R2_KNOBS.iter().all(|knob| !is_set(env, knob)) {
        return None;
    }
    let values: Vec<Option<String>> = R2_KNOBS
        .iter()
        .map(|knob| required(env, faults, knob, WHY_R2))
        .collect();
    let [
        Some(account_id),
        Some(access_key_id),
        Some(secret_access_key),
        Some(bucket),
    ] = values.as_slice()
    else {
        return None;
    };
    Some(BundleStoreConfig {
        account_id: account_id.as_str().into(),
        access_key_id: access_key_id.as_str().into(),
        secret_access_key: secret_access_key.as_str().into(),
        bucket: bucket.as_str().into(),
    })
}

/// Resolves the identity provider, which every boot must have.
///
/// Returns `None` after pushing a fault for each knob that is unset or
/// unusable. There is no "configured nothing" answer: `runtime_validate.zig`
/// exits with `fatal: OIDC is required — set OIDC_ISSUER and OIDC_AUDIENCE`,
/// and this daemon replaces that one. `cmd/doctor.zig` records the narrower
/// half of the same rule — "reject at boot (e.g. `OIDC_JWKS_URL` set but
/// `OIDC_ISSUER` missing)" — because a half-configured provider fails at the
/// first tenant request rather than at boot, which is the furthest possible
/// point from the mistake.
pub(super) fn identity<E: EnvSource + ?Sized>(
    env: &E,
    faults: &mut Vec<Fault>,
) -> Option<IdentityConfig> {
    let issuer = required(env, faults, OIDC_ISSUER_KNOB, WHY_ISSUER);
    let audience = required(env, faults, OIDC_AUDIENCE_KNOB, WHY_AUDIENCE);
    let api_base = required(env, faults, PROVIDER_API_BASE_KNOB, WHY_API_BASE);
    let raw_secret = required(env, faults, PROVIDER_SECRET_KNOB, WHY_SECRET);

    let secret = raw_secret.and_then(|raw| {
        classify(
            faults,
            true,
            PROVIDER_SECRET_KNOB,
            WHY_SECRET,
            ProviderSecret::new(&raw),
        )
    });

    let (Some(issuer), Some(audience), Some(api_base), Some(secret)) =
        (issuer, audience, api_base, secret)
    else {
        return None;
    };
    Some(IdentityConfig {
        issuer: issuer.into(),
        audience: audience.into(),
        // Optional by design: the key-set endpoint is DERIVED from the issuer
        // unless an operator has a reason, which is what keeps the two from
        // ever naming different providers.
        jwks_url: env
            .get(OIDC_JWKS_URL_KNOB)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(Into::into),
        api_base: api_base.into(),
        secret,
    })
}

/// Reads a knob that must be present, recording a fault when it is not.
pub(super) fn required<E: EnvSource + ?Sized>(
    env: &E,
    faults: &mut Vec<Fault>,
    knob: &'static str,
    why: &'static str,
) -> Option<String> {
    let value = env
        .get(knob)
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty());
    if value.is_none() {
        faults.push(Fault::Missing { knob, why });
    }
    value
}

/// Whether `knob` carries a value that is not blank.
pub(super) fn is_set<E: EnvSource + ?Sized>(env: &E, knob: &str) -> bool {
    env.get(knob).is_some_and(|value| !value.trim().is_empty())
}

/// Records a resolver's failure as missing or invalid, by whether it was set.
///
/// The resolvers answer with one error type for both cases, and they are
/// different operator problems: "you forgot this" is fixed by supplying a
/// value, "what you wrote does not work" by correcting one. Collapsing them
/// would make the second read like the first.
pub(super) fn classify<T, E: fmt::Display>(
    faults: &mut Vec<Fault>,
    was_set: bool,
    knob: &'static str,
    why: &'static str,
    outcome: Result<T, E>,
) -> Option<T> {
    match outcome {
        Ok(value) => Some(value),
        Err(error) if was_set => {
            faults.push(Fault::Invalid {
                knob,
                why: error.to_string(),
            });
            None
        }
        Err(_unset) => {
            faults.push(Fault::Missing { knob, why });
            None
        }
    }
}

/// Resolves the master key, which no sibling crate reads from the environment.
///
/// `afd_crypto` deliberately takes hex rather than a knob name — it is the
/// layer that must not know where a key came from — so the read belongs here.
pub(super) fn read_kek<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Option<Kek> {
    let Some(hex) = env
        .get(ENCRYPTION_MASTER_KEY_KNOB)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        faults.push(Fault::Missing {
            knob: ENCRYPTION_MASTER_KEY_KNOB,
            why: WHY_KEK,
        });
        return None;
    };

    classify(
        faults,
        true,
        ENCRYPTION_MASTER_KEY_KNOB,
        WHY_KEK,
        Kek::from_hex(&hex),
    )
}
