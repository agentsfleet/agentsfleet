//! Every knob boot reads, and the sentence each one refuses with.
//!
//! Split from `preflight` because the list grows with the product while the
//! collecting logic beside it does not, and a name plus the reason it exists
//! is the part an operator reading a refusal actually meets.

/// R2 fixes the AWS Signature V4 region label to `auto`, and account endpoints address the
/// bucket in the path rather than in the hostname.
pub(super) const R2_REGION: &str = "auto";

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
pub(super) const APP_URL_DEFAULT: &str = "https://app.agentsfleet.net";

/// This deployment's own base URL, as a minted credential records it.
///
/// Optional with a default, exactly as [`APP_URL_KNOB`] is and for the same
/// reason: every deployment but a developer's own points at the same place, and
/// refusing to boot over a knob with one sensible value would be ceremony.
/// `runtime_loader.zig` reads the same knob with the same default.
pub const API_URL_KNOB: &str = "API_URL";

/// The deployment [`API_URL_KNOB`] falls back to.
pub(super) const API_URL_DEFAULT: &str = "https://api.agentsfleet.net";

/// How many concurrent event streams one instance carries.
///
/// `SSE_MAX_STREAMS`, and it is a knob rather than a constant because it is the
/// one ceiling an operator tunes against their own host: a stream costs a task
/// and a pub/sub map entry, so the right number is a property of the box.
/// The `PostHog` project this deployment reports product events to.
///
/// `POSTHOG_API_KEY`. Unset is most deployments — every developer's, every
/// test — and it means the daemon reports nothing rather than refusing to boot.
pub(super) const POSTHOG_KEY_KNOB: &str = "POSTHOG_API_KEY";

/// Where those events are ingested.
///
/// `POSTHOG_HOST`, unset in every deployment the Zig runs: it hardcodes the US
/// ingestion host. A knob rather than a constant because a self-hosted `PostHog`
/// and the EU region are both real, and neither is a code change.
pub(super) const POSTHOG_HOST_KNOB: &str = "POSTHOG_HOST";

pub(super) const SSE_MAX_STREAMS_KNOB: &str = "SSE_MAX_STREAMS";

/// `SSE_MAX_STREAMS_DEFAULT`, mirrored.
pub(super) const SSE_MAX_STREAMS_DEFAULT: usize = 64;

/// Why a stream ceiling that will not parse refuses boot.
pub(super) const WHY_SSE_MAX_STREAMS: &str =
    "a whole number of concurrent event streams, at least 1";

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
pub(super) const R2_KNOBS: [&str; 4] = [
    R2_ACCOUNT_ID_KNOB,
    R2_ACCESS_KEY_ID_KNOB,
    R2_SECRET_ACCESS_KEY_KNOB,
    R2_BUCKET_KNOB,
];

/// Why a set platform-admin workspace has to be an identifier.
pub(super) const WHY_PLATFORM_ADMIN: &str = "the workspace holding this deployment's platform credentials, as a UUIDv7; unset it to serve without on-demand credential minting";

/// Why the daemon needs the API database role.
pub(super) const WHY_DATABASE: &str = "the API role's Postgres connection URL";

/// Why the daemon needs the API Redis role.
pub(super) const WHY_REDIS: &str = "the API role's Redis connection URL";

/// Why the daemon needs the master key.
pub(super) const WHY_KEK: &str = "64 hex characters; every stored credential is sealed under it";

/// Why the login pepper is required.
pub(super) const WHY_SESSION_PEPPER: &str = "the key a device-flow verification code's digest is taken under; without it a readable queue is a usable login";

/// Why an identity provider needs an issuer once any of its knobs is set.
pub(super) const WHY_ISSUER: &str =
    "the identity provider's issuer URL; the key-set endpoint is derived from it";

/// Why it needs an audience.
pub(super) const WHY_AUDIENCE: &str = "the audience this daemon accepts, checked strictly so a token minted for a sibling service is refused";

/// Why it needs an API base.
pub(super) const WHY_API_BASE: &str =
    "the provider API base a subject's capability claim is read from";

/// Why it needs a secret.
pub(super) const WHY_SECRET: &str = "the provider secret that authorises reading a subject's claim";

/// Why a half-configured snapshot store refuses boot.
///
/// One sentence for all four knobs, because the fault is never about one of
/// them in isolation — it is that some are set and some are not.
pub(super) const WHY_R2: &str = "Fleet Bundle snapshot storage needs all four R2 knobs or none; set the rest, or unset them all to serve without snapshots";
