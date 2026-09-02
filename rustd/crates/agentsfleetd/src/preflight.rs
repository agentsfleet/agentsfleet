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

mod config;
#[cfg(test)]
mod knob_tests;
mod knobs;
mod otlp;
mod read;

pub use self::config::{BootConfig, BundleStoreConfig, IdentityConfig, PostHogConfig};
pub use self::otlp::{
    GRAFANA_API_KEY_KNOB, GRAFANA_ENDPOINT_KNOB, GRAFANA_INSTANCE_KNOB, OTEL_ENDPOINT_KNOB,
    OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB, OTEL_TIMEOUT_KNOB, OtlpConfig, PROTOCOL_JSON,
};

pub use self::knobs:: {
    API_URL_KNOB,
    APP_URL_KNOB,
    ENCRYPTION_MASTER_KEY_KNOB,
    IDENTITY_WEBHOOK_SECRET_KNOB,
    OIDC_AUDIENCE_KNOB,
    OIDC_ISSUER_KNOB,
    OIDC_JWKS_URL_KNOB,
    PLATFORM_ADMIN_WORKSPACE_KNOB,
    PROVIDER_API_BASE_KNOB,
    PROVIDER_SECRET_KNOB,
    QSTASH_CURRENT_KEY_KNOB,
    QSTASH_NEXT_KEY_KNOB,
    QSTASH_TOKEN_KNOB,
    QSTASH_URL_KNOB,
    R2_ACCESS_KEY_ID_KNOB,
    R2_ACCOUNT_ID_KNOB,
    R2_BUCKET_KNOB,
    R2_SECRET_ACCESS_KEY_KNOB,
    SESSION_CODE_PEPPER_KNOB,
};

use self::knobs:: {
    API_URL_DEFAULT,
    APP_URL_DEFAULT,
    POSTHOG_HOST_KNOB,
    POSTHOG_KEY_KNOB,
    WHY_DATABASE,
    WHY_PLATFORM_ADMIN,
    WHY_REDIS,
    WHY_SESSION_PEPPER,
};

use self::read::{bundle_store, classify, identity, is_set, read_kek, required};

use afd_core::env::EnvSource;
use afd_core::id::Uuid7;
use afd_cron::SigningKeys;
use afd_crypto::secret::SecretBytes;
use afd_db::config::{DbRole, PoolConfig};
use afd_redis::config::{RedisConfig, RedisRole};

#[doc(inline)]
pub use crate::error::{Fault, Refusal};

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
    // Both are optional-with-a-default and resolve identically, so the shape is
    // named once rather than written twice.
    let optional_url = |knob: &str, fallback: &str| {
        env.get(knob)
            .map(|raw| raw.trim().to_owned())
            .filter(|raw| !raw.is_empty())
            .unwrap_or_else(|| fallback.to_owned())
            .into_boxed_str()
    };
    // Set and unparseable is a typo, and a typo here would otherwise surface as
    // a dashboard that silently refuses every stream — the furthest possible
    // point from the mistake. Unset is the default, which is most deployments.
    let sse_max_streams = read::sse_max_streams(env, &mut faults);
    // Absent is analytics off, and it is not a fault: a deployment that reports
    // nothing is the normal case, and refusing to boot over an unset key would
    // make every developer configure a product-analytics project to run the
    // daemon.
    let posthog = env
        .get(POSTHOG_KEY_KNOB)
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
        .map(|project_key| PostHogConfig {
            project_key: project_key.into_boxed_str(),
            host: env
                .get(POSTHOG_HOST_KNOB)
                .map(|raw| raw.trim().to_owned())
                .filter(|raw| !raw.is_empty())
                .map(String::into_boxed_str),
        });
    // Absent is a deployment that exports nothing, which is most of them and
    // is not a fault. What IS a fault is an endpoint with a protocol, timeout
    // or header list this build cannot use — those are typos that would
    // otherwise surface as a collector that never receives anything.
    let otlp = self::otlp::otlp(env, &mut faults);
    let dashboard = optional_url(APP_URL_KNOB, APP_URL_DEFAULT);
    let deployment = optional_url(API_URL_KNOB, API_URL_DEFAULT);
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
                app_url: dashboard,
                api_url: deployment,
                identity,
                bundles,
                platform_admin_workspace,
                qstash_token: optional(env, QSTASH_TOKEN_KNOB),
                qstash_url: optional(env, QSTASH_URL_KNOB),
                identity_webhook_secret: optional(env, IDENTITY_WEBHOOK_SECRET_KNOB),
                qstash_keys: signing_keys(env),
                sse_max_streams,
                posthog,
                otlp,
            })
        }
        // Anything else: a knob that is missing or unusable, the identity
        // provider included. Every one of them has already pushed its own
        // fault, so the refusal names them all rather than the first.
        _refused => Err(Refusal::new(faults)),
    }
}

/// One optional knob, absent when it is unset or blank.
///
/// Blank is treated as absent rather than as a value, because an environment
/// that exports a variable to the empty string is an environment that meant to
/// unset it — and an empty bearer would be sent upstream as `Bearer `.
fn optional<E: EnvSource + ?Sized>(source: &E, knob: &str) -> Option<Box<str>> {
    let value = source.get(knob)?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.into())
}

/// The scheduler's signing keys, when BOTH are configured.
///
/// Both or neither, deliberately. One key configured is a rotation half-done,
/// and a verifier holding the current key alone refuses every delivery the
/// vendor has already moved to the next one — which is the outage the two-key
/// check exists to prevent. Treating a half-configuration as no configuration
/// makes that a loud refusal at the first fire rather than a silent one at the
/// vendor's next rotation.
fn signing_keys<E: EnvSource + ?Sized>(source: &E) -> Option<SigningKeys> {
    let current = optional(source, QSTASH_CURRENT_KEY_KNOB)?;
    let next = optional(source, QSTASH_NEXT_KEY_KNOB)?;
    Some(SigningKeys {
        current: current.into_string(),
        next: next.into_string(),
    })
}
