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
mod read;

pub use self::config::{BootConfig, BundleStoreConfig, IdentityConfig, PostHogConfig};

use self::read::{bundle_store, classify, identity, is_set, read_kek, required};

use afd_core::env::EnvSource;
use afd_core::id::Uuid7;
use afd_cron::SigningKeys;
use afd_crypto::secret::SecretBytes;
use afd_db::config::{DbRole, PoolConfig};
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

/// This deployment's bearer for the external scheduler.
///
/// Optional, for [`PLATFORM_ADMIN_WORKSPACE_KNOB`]'s reason: a deployment that
/// registers no schedules still serves everything else, and refusing to boot
/// would take the product down for a surface nobody called.
pub const QSTASH_TOKEN_KNOB: &str = "QSTASH_TOKEN";

/// Which scheduler deployment the management calls go to.
///
/// Optional, and its absence resolves to [`afd_cron::qstash::API_BASE`].
/// It exists because the vendor is regional: `qstash_client.zig` took this as a
/// parameter and its "outbound url uses the configured api base, not a hardcoded
/// host" test names `qstash-eu-central-1.upstash.io` as the case a hardcoded US
/// host breaks. The operational half already carries it — `platform_secret_sync.sh`
/// syncs `url|qstash/url` beside the token — so a deployment that set the URL and
/// found it ignored was configuring something the daemon never read.
pub const QSTASH_URL_KNOB: &str = "QSTASH_URL";

/// What a signup event from the identity provider is verified against.
///
/// Optional, and its absence is FAIL-CLOSED rather than a degradation: the
/// route refuses every delivery, because accepting an unverified one on a
/// public endpoint that creates accounts would be strictly worse than serving
/// none. The same posture `QSTASH_CURRENT_KEY_KNOB` takes, for the same reason.
pub const IDENTITY_WEBHOOK_SECRET_KNOB: &str = "CLERK_WEBHOOK_SECRET";

/// The key the scheduler is signing fire callbacks with now.
///
/// Optional AND fail-closed, which is not a contradiction: a deployment without
/// it boots and serves, and refuses every fire — because a daemon that cannot
/// verify a callback must not act on one.
pub const QSTASH_CURRENT_KEY_KNOB: &str = "QSTASH_CURRENT_SIGNING_KEY";

/// The key it will sign with next.
///
/// Both are read because the scheduler rotates by promoting the second, and a
/// daemon that knew one would refuse every delivery between the vendor's
/// rotation and its own redeploy.
pub const QSTASH_NEXT_KEY_KNOB: &str = "QSTASH_NEXT_SIGNING_KEY";

/// Where a person goes to approve a command-line login.
///
/// Optional, with the production dashboard as its default — exactly what
/// `runtime_loader.zig` does, and for the same reason: every deployment but a
/// developer's own points at the same place, and refusing to boot over a knob
/// with one sensible value would be ceremony.
pub const APP_URL_KNOB: &str = "APP_URL";

/// The dashboard [`APP_URL_KNOB`] falls back to.
const APP_URL_DEFAULT: &str = "https://app.agentsfleet.net";

/// This deployment's own base URL, as a minted credential records it.
///
/// Optional with a default, exactly as [`APP_URL_KNOB`] is and for the same
/// reason: every deployment but a developer's own points at the same place, and
/// refusing to boot over a knob with one sensible value would be ceremony.
/// `runtime_loader.zig` reads the same knob with the same default.
pub const API_URL_KNOB: &str = "API_URL";

/// The deployment [`API_URL_KNOB`] falls back to.
const API_URL_DEFAULT: &str = "https://api.agentsfleet.net";

/// How many concurrent event streams one instance carries.
///
/// `SSE_MAX_STREAMS`, and it is a knob rather than a constant because it is the
/// one ceiling an operator tunes against their own host: a stream costs a task
/// and a pub/sub map entry, so the right number is a property of the box.
/// The `PostHog` project this deployment reports product events to.
///
/// `POSTHOG_API_KEY`. Unset is most deployments — every developer's, every
/// test — and it means the daemon reports nothing rather than refusing to boot.
const POSTHOG_KEY_KNOB: &str = "POSTHOG_API_KEY";

/// Where those events are ingested.
///
/// `POSTHOG_HOST`, unset in every deployment the Zig runs: it hardcodes the US
/// ingestion host. A knob rather than a constant because a self-hosted `PostHog`
/// and the EU region are both real, and neither is a code change.
const POSTHOG_HOST_KNOB: &str = "POSTHOG_HOST";

const SSE_MAX_STREAMS_KNOB: &str = "SSE_MAX_STREAMS";

/// `SSE_MAX_STREAMS_DEFAULT`, mirrored.
const SSE_MAX_STREAMS_DEFAULT: usize = 64;

/// Why a stream ceiling that will not parse refuses boot.
const WHY_SSE_MAX_STREAMS: &str = "a whole number of concurrent event streams, at least 1";

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

#[cfg(test)]
mod knob_tests {
    use super::{QSTASH_CURRENT_KEY_KNOB, QSTASH_NEXT_KEY_KNOB, optional, signing_keys};
    use afd_core::env::MapEnv;

    /// A key this fixture configures; the value is never parsed, only carried.
    const CURRENT: &str = "sig_current_fixture";

    /// The key a rotation moves to.
    const NEXT: &str = "sig_next_fixture";

    /// An exported-but-blank knob is unset, not a value.
    ///
    /// The distinction is not cosmetic: a blank that read as configured would
    /// be sent upstream as a bare `Bearer `, which the vendor refuses with a
    /// sentence naming nothing an operator can act on. Whitespace is trimmed
    /// for the same reason — a knob set from a shell heredoc arrives with a
    /// newline attached.
    #[test]
    fn a_blank_or_whitespace_knob_is_absent_rather_than_empty() {
        let source = MapEnv::from_pairs([
            ("SET", "value"),
            ("BLANK", ""),
            ("SPACES", "   "),
            ("PADDED", "  value  "),
        ]);

        assert_eq!(optional(&source, "SET").as_deref(), Some("value"));
        assert_eq!(
            optional(&source, "BLANK"),
            None,
            "an exported empty string meant unset"
        );
        assert_eq!(
            optional(&source, "SPACES"),
            None,
            "whitespace is not a value"
        );
        assert_eq!(
            optional(&source, "PADDED").as_deref(),
            Some("value"),
            "a knob set from a heredoc arrives padded and is still that value"
        );
        assert_eq!(optional(&source, "ABSENT"), None);
    }

    /// Both keys or neither — a half-rotation is no configuration.
    ///
    /// The failure this prevents is delayed and total. A verifier holding only
    /// the current key works right up until the vendor rotates to the next one,
    /// and then refuses EVERY delivery — an outage that begins on the vendor's
    /// schedule rather than on any deploy of ours. Treating half as none makes
    /// it a loud refusal at the first fire instead.
    #[test]
    fn one_signing_key_is_no_configuration_rather_than_half_of_one() {
        let neither = MapEnv::from_pairs([]);
        assert!(
            signing_keys(&neither).is_none(),
            "neither key is unconfigured"
        );

        let current_only = MapEnv::from_pairs([(QSTASH_CURRENT_KEY_KNOB, CURRENT)]);
        assert!(
            signing_keys(&current_only).is_none(),
            "the current key alone is a rotation half-done — it verifies until \
             the vendor rotates and then refuses everything"
        );

        let next_only = MapEnv::from_pairs([(QSTASH_NEXT_KEY_KNOB, NEXT)]);
        assert!(
            signing_keys(&next_only).is_none(),
            "the next key alone is the same half"
        );

        let both = MapEnv::from_pairs([
            (QSTASH_CURRENT_KEY_KNOB, CURRENT),
            (QSTASH_NEXT_KEY_KNOB, NEXT),
        ]);
        let keys = signing_keys(&both).expect("both keys configured is a configuration");
        assert_eq!(keys.current, CURRENT);
        assert_eq!(keys.next, NEXT);
    }
}
