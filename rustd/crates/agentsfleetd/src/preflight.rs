//! Every reason boot cannot proceed, gathered before anything opens a socket.
//!
//! # Why this collects rather than exits
//!
//! `serve_boot.zig` calls `std.process.exit(1)` at each check in turn, so an
//! operator holding three unset knobs fixes one, restarts, and learns about the
//! second. Dimension 8.1 asks for all of them in one output, and the shape that
//! gets there is a function that RETURNS its faults instead of ending the
//! process in the middle of one.
//!
//! That is also the only shape a test can drive. `std::process::exit` inside a
//! library is unobservable without spawning a child, so the exit lives in
//! `main` and nowhere else — the library's job is to decide, `main`'s job is to
//! report and set a status.
//!
//! # Order
//!
//! Every knob is read and validated BEFORE any connection is attempted, which
//! is what makes "a malformed key refuses boot" a promise rather than a race:
//! there is no window in which a daemon with an unusable KEK has already
//! opened a listening socket.

use std::fmt;

use afd_core::env::EnvSource;
use afd_core::id::Uuid7;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::config::{DbRole, PoolConfig};
use afd_identity::ProviderSecret;
use afd_redis::config::{RedisConfig, RedisRole};

#[doc(inline)]
pub use crate::error::{Fault, Refusal};

/// R2 fixes the AWS Signature V4 region label to `auto`, and account endpoints address the
/// bucket in the path rather than in the hostname.
const R2_REGION: &str = "auto";

/// The knob carrying the hex master key every vault read is decrypted with.
pub const ENCRYPTION_MASTER_KEY_KNOB: &str = "ENCRYPTION_MASTER_KEY";

/// The identity provider's issuer, and the base the key-set URL is derived from.
pub const OIDC_ISSUER_KNOB: &str = "OIDC_ISSUER";

/// The audience this daemon accepts, checked strictly.
pub const OIDC_AUDIENCE_KNOB: &str = "OIDC_AUDIENCE";

/// An explicit key-set endpoint, overriding the one derived from the issuer.
pub const OIDC_JWKS_URL_KNOB: &str = "OIDC_JWKS_URL";

/// The provider's API base, read for a subject's capability claim.
pub const PROVIDER_API_BASE_KNOB: &str = "CLERK_API_BASE";

/// The secret that authorises this daemon to read a subject's claim.
pub const PROVIDER_SECRET_KNOB: &str = "CLERK_SECRET_KEY";

/// The workspace holding this deployment's own platform credentials.
///
/// The GitHub App and the OAuth clients the credential broker mints through
/// live as ordinary vault rows in ONE workspace, so this knob names which.
/// Optional: a deployment that has connected no third party mints nothing, and
/// `serve_broker.zig` reads the same value with the same default of none.
pub const PLATFORM_ADMIN_WORKSPACE_KNOB: &str = "PLATFORM_ADMIN_WORKSPACE_ID";

/// The key a device-flow verification code's digest is taken under.
///
/// Required, not optional, and it is the one login knob that is: without it the
/// daemon would store an unpeppered digest, and a queue somebody can read would
/// become a queue somebody can log in from. `runtime_validate.zig` refuses boot
/// on the same knob for the same reason.
pub const SESSION_CODE_PEPPER_KNOB: &str = "AUTH_SESSION_CODE_PEPPER";

/// Where a person goes to approve a command-line login.
///
/// Optional, with the production dashboard as its default — exactly what
/// `runtime_loader.zig` does, and for the same reason: every deployment but a
/// developer's own points at the same place, and refusing to boot over a knob
/// with one sensible value would be ceremony.
pub const APP_URL_KNOB: &str = "APP_URL";

/// The dashboard [`APP_URL_KNOB`] falls back to.
const APP_URL_DEFAULT: &str = "https://app.agentsfleet.net";

/// The Cloudflare account the Fleet Bundle bucket lives under.
pub const R2_ACCOUNT_ID_KNOB: &str = "R2_ACCOUNT_ID";

/// The access key id the snapshot GET is signed with.
pub const R2_ACCESS_KEY_ID_KNOB: &str = "R2_ACCESS_KEY_ID";

/// The secret half of that key.
pub const R2_SECRET_ACCESS_KEY_KNOB: &str = "R2_SECRET_ACCESS_KEY";

/// The bucket Fleet Bundle snapshots are stored in.
pub const R2_BUCKET_KNOB: &str = "R2_BUCKET";

/// Every knob the snapshot store needs, in the order an operator sets them.
///
/// Named as a group because the rule is about the group: all four or none.
const R2_KNOBS: [&str; 4] = [
    R2_ACCOUNT_ID_KNOB,
    R2_ACCESS_KEY_ID_KNOB,
    R2_SECRET_ACCESS_KEY_KNOB,
    R2_BUCKET_KNOB,
];

/// Why a set platform-admin workspace has to be an identifier.
const WHY_PLATFORM_ADMIN: &str = "the workspace holding this deployment's platform credentials, as a UUIDv7; unset it to serve without on-demand credential minting";

/// Why the daemon needs the API database role.
const WHY_DATABASE: &str = "the API role's Postgres connection URL";

/// Why the daemon needs the API Redis role.
const WHY_REDIS: &str = "the API role's Redis connection URL";

/// Why the daemon needs the master key.
const WHY_KEK: &str = "64 hex characters; every stored credential is sealed under it";

/// Why the login pepper is required.
const WHY_SESSION_PEPPER: &str = "the key a device-flow verification code's digest is taken under; without it a readable queue is a usable login";

/// Why an identity provider needs an issuer once any of its knobs is set.
const WHY_ISSUER: &str =
    "the identity provider's issuer URL; the key-set endpoint is derived from it";

/// Why it needs an audience.
const WHY_AUDIENCE: &str = "the audience this daemon accepts, checked strictly so a token minted for a sibling service is refused";

/// Why it needs an API base.
const WHY_API_BASE: &str = "the provider API base a subject's capability claim is read from";

/// Why it needs a secret.
const WHY_SECRET: &str = "the provider secret that authorises reading a subject's claim";

/// Why a half-configured snapshot store refuses boot.
///
/// One sentence for all four knobs, because the fault is never about one of
/// them in isolation — it is that some are set and some are not.
const WHY_R2: &str = "Fleet Bundle snapshot storage needs all four R2 knobs or none; set the rest, or unset them all to serve without snapshots";

/// The identity provider a deployment has, when it has one.
///
/// Owned and complete: a value of this type means every knob the provider needs
/// was present and usable, so nothing downstream re-checks. The absence of one
/// is the other half of the rule — see [`identity`].
#[derive(Debug, Clone)]
pub struct IdentityConfig {
    /// The required `iss` claim, and the base the key-set URL derives from.
    pub issuer: Box<str>,
    /// The required `aud` claim.
    pub audience: Box<str>,
    /// An explicit key-set endpoint, when the derived one is not wanted.
    pub jwks_url: Option<Box<str>>,
    /// The provider API base a capability claim is read from.
    pub api_base: Box<str>,
    /// The secret that authorises that read.
    pub secret: ProviderSecret,
}

/// What boot needs resolved before it opens anything.
#[derive(Debug)]
pub struct BootConfig {
    api_pool: PoolConfig,
    redis: RedisConfig,
    kek: Kek,
    session_code_pepper: SecretBytes,
    app_url: Box<str>,
    identity: IdentityConfig,
    bundles: Option<BundleStoreConfig>,
    platform_admin_workspace: Option<Uuid7>,
}

/// The Fleet Bundle snapshot store's credentials, complete.
///
/// Owned and whole, like [`IdentityConfig`]: a value of this type means all
/// four knobs were present, so nothing downstream re-checks.
#[derive(Debug, Clone)]
pub struct BundleStoreConfig {
    /// The Cloudflare account the endpoint is derived from.
    pub account_id: Box<str>,
    /// The access key id the GET is signed with.
    pub access_key_id: Box<str>,
    /// The secret half of that key.
    pub secret_access_key: Box<str>,
    /// The bucket snapshots are stored in.
    pub bucket: Box<str>,
}

impl BundleStoreConfig {
    /// The account-scoped endpoint `r2.zig` builds from the same account id.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("https://{}.r2.cloudflarestorage.com", self.account_id)
    }

    /// The region label R2 requires, which is fixed rather than configured.
    #[must_use]
    pub const fn region() -> &'static str {
        R2_REGION
    }
}

impl BootConfig {
    /// Settings for the API role's Postgres pool.
    #[must_use]
    pub const fn api_pool(&self) -> &PoolConfig {
        &self.api_pool
    }

    /// Settings for the API role's Redis client.
    #[must_use]
    pub const fn redis(&self) -> &RedisConfig {
        &self.redis
    }

    /// The master key every vault read is decrypted with.
    #[must_use]
    pub const fn kek(&self) -> &Kek {
        &self.kek
    }

    /// The key a device-flow verification code's digest is taken under.
    ///
    /// Held as the raw configured BYTES rather than as a decoded key, and that
    /// is a wire-format fact rather than an oversight: the Zig daemon keys its
    /// HMAC with the sixty-four hexadecimal characters as text, both binaries
    /// write the same session blob, and a Lua script compares the two digests
    /// as strings. Decoding here would silently invalidate every session the
    /// other binary approved.
    #[must_use]
    pub const fn session_code_pepper(&self) -> &SecretBytes {
        &self.session_code_pepper
    }

    /// Where a person goes to approve a command-line login.
    #[must_use]
    pub const fn app_url(&self) -> &str {
        &self.app_url
    }

    /// The identity provider, which every boot has.
    ///
    /// Not optional: `runtime_validate.zig` refuses to boot without
    /// `OIDC_ISSUER` and `OIDC_AUDIENCE`, and a daemon that answered a tenant
    /// request differently from the one it replaces would be a cutover
    /// divergence discovered in production. A deployment that wants only the
    /// runner plane still configures a provider; what it does not do is serve
    /// the tenant surface.
    #[must_use]
    pub const fn identity(&self) -> &IdentityConfig {
        &self.identity
    }

    /// The workspace this deployment's own platform credentials live in.
    ///
    /// Optional for [`Self::bundles`]' reason: a deployment that mints no
    /// third-party credentials still serves everything else, and refusing to
    /// boot would take the product down for an endpoint nobody called.
    #[must_use]
    pub const fn platform_admin_workspace(&self) -> Option<&Uuid7> {
        self.platform_admin_workspace.as_ref()
    }

    /// The Fleet Bundle snapshot store, when this deployment has one.
    ///
    /// Optional where [`Self::identity`] is not, and `serve_r2.zig` draws the
    /// same line: it builds an R2 client only when all four knobs are present
    /// and serves everything else regardless. Most deployments run fleets with
    /// no support files and never reach the verb, so refusing to boot would
    /// take the whole product down for an endpoint nobody called.
    #[must_use]
    pub const fn bundles(&self) -> Option<&BundleStoreConfig> {
        self.bundles.as_ref()
    }
}

/// Reads every boot knob, reporting ALL faults rather than the first.
///
/// # Errors
/// Returns a [`Refusal`] naming every knob that is unset or unusable. A caller
/// that gets one has nothing to retry: the process cannot serve, and the
/// message is what the operator needs.
pub fn preflight<E: EnvSource + ?Sized>(env: &E) -> Result<BootConfig, Refusal> {
    let mut faults = Vec::new();

    let database_knob = DbRole::Api.url_knob();
    let api_pool = classify(
        &mut faults,
        is_set(env, database_knob),
        database_knob,
        WHY_DATABASE,
        PoolConfig::resolve(env, DbRole::Api),
    );

    let redis_knob = RedisRole::Api.url_knob();
    let redis = classify(
        &mut faults,
        is_set(env, redis_knob),
        redis_knob,
        WHY_REDIS,
        RedisConfig::resolve(env, RedisRole::Api),
    );

    let kek = read_kek(env, &mut faults);
    let session_code_pepper = required(
        env,
        &mut faults,
        SESSION_CODE_PEPPER_KNOB,
        WHY_SESSION_PEPPER,
    )
    .map(|pepper| SecretBytes::new(pepper.into_bytes()));
    let app_url = env
        .get(APP_URL_KNOB)
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
        .unwrap_or_else(|| APP_URL_DEFAULT.to_owned())
        .into_boxed_str();
    let identity = identity(env, &mut faults);
    // Answers `None` for BOTH "configured nothing" and "configured badly", and
    // only the second pushed a fault — which is why the match below reads the
    // fault list rather than this value to decide whether boot proceeds.
    let bundles = bundle_store(env, &mut faults);
    // Unset is a deployment that mints nothing; SET and unparseable is a typo
    // that would otherwise surface as "not connected" at the first mint, which
    // is the furthest possible point from the mistake.
    let platform_admin_workspace = env
        .get(PLATFORM_ADMIN_WORKSPACE_KNOB)
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
        .and_then(|raw| {
            classify(
                &mut faults,
                true,
                PLATFORM_ADMIN_WORKSPACE_KNOB,
                WHY_PLATFORM_ADMIN,
                Uuid7::parse(&raw),
            )
        });

    match (api_pool, redis, kek, identity, session_code_pepper) {
        (Some(api_pool), Some(redis), Some(kek), Some(identity), Some(session_code_pepper))
            if faults.is_empty() =>
        {
            Ok(BootConfig {
                api_pool,
                redis,
                kek,
                session_code_pepper,
                app_url,
                identity,
                bundles,
                platform_admin_workspace,
            })
        }
        // Anything else: a knob that is missing or unusable, the identity
        // provider included. Every one of them has already pushed its own
        // fault, so the refusal names them all rather than the first.
        _refused => Err(Refusal::new(faults)),
    }
}

/// Resolves the snapshot store, which a boot may legitimately not have.
///
/// Three outcomes, not two. All four knobs set is a store; none set is a
/// deployment that serves no snapshots, which is not a fault and pushes none.
/// SOME set is a fault per missing knob, and that is the case this function
/// exists for: a half-configured store boots fine and then fails at the first
/// bundle fetch, which is the furthest possible point from the mistake — the
/// same rule `cmd/doctor.zig` records for a half-configured identity provider.
fn bundle_store<E: EnvSource + ?Sized>(
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
fn identity<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Option<IdentityConfig> {
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
fn required<E: EnvSource + ?Sized>(
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
fn is_set<E: EnvSource + ?Sized>(env: &E, knob: &str) -> bool {
    env.get(knob).is_some_and(|value| !value.trim().is_empty())
}

/// Records a resolver's failure as missing or invalid, by whether it was set.
///
/// The resolvers answer with one error type for both cases, and they are
/// different operator problems: "you forgot this" is fixed by supplying a
/// value, "what you wrote does not work" by correcting one. Collapsing them
/// would make the second read like the first.
fn classify<T, E: fmt::Display>(
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
fn read_kek<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Option<Kek> {
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
