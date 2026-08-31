//! What a validated environment resolved to.
//!
//! The TYPES, split from the reading beside them: `preflight.rs` answers "is
//! this environment usable and which knob is wrong", and this answers "what did
//! it turn out to be". A value of any type here means every knob it needs was
//! present and well formed, which is the property every accessor below relies
//! on — nothing downstream re-validates, and nothing downstream can.

use afd_core::id::Uuid7;
use afd_cron::SigningKeys;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::PoolConfig;
use afd_identity::ProviderSecret;
use afd_redis::RedisConfig;

use super::R2_REGION;

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
    pub(super) api_pool: PoolConfig,
    pub(super) redis: RedisConfig,
    pub(super) kek: Kek,
    pub(super) session_code_pepper: SecretBytes,
    pub(super) app_url: Box<str>,
    pub(super) api_url: Box<str>,
    pub(super) identity: IdentityConfig,
    pub(super) bundles: Option<BundleStoreConfig>,
    pub(super) platform_admin_workspace: Option<Uuid7>,
    pub(super) qstash_token: Option<Box<str>>,
    pub(super) qstash_url: Option<Box<str>>,
    pub(super) identity_webhook_secret: Option<Box<str>>,
    pub(super) qstash_keys: Option<SigningKeys>,
    pub(super) sse_max_streams: usize,
    pub(super) posthog: Option<PostHogConfig>,
}

/// Where product events go, when this deployment sends any.
///
/// A value of this type means a project key was present. The host stays an
/// `Option` inside it because the client's own default is the right answer for
/// every deployment that does not self-host.
#[derive(Debug, Clone)]
pub struct PostHogConfig {
    /// The project token events are captured against.
    pub project_key: Box<str>,
    /// The ingestion host, when this deployment names one.
    pub host: Option<Box<str>>,
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

    /// How many concurrent event streams this instance carries.
    #[must_use]
    pub const fn sse_max_streams(&self) -> usize {
        self.sse_max_streams
    }

    /// Where this deployment's product events go, when it sends any.
    #[must_use]
    pub const fn posthog(&self) -> Option<&PostHogConfig> {
        self.posthog.as_ref()
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

    /// This deployment's own base URL, as a minted credential records it.
    ///
    /// Read from configuration rather than from a request's `Host`, because a
    /// credential and the deployment that minted it are ONE fact: a
    /// client-asserted host would let the two disagree, and the row would then
    /// name a deployment that never issued it.
    #[must_use]
    pub const fn api_url(&self) -> &str {
        &self.api_url
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

    /// This deployment's bearer for the external scheduler, when it has one.
    #[must_use]
    pub fn qstash_token(&self) -> Option<&str> {
        self.qstash_token.as_deref()
    }

    /// Which scheduler deployment the management calls go to.
    ///
    /// `None` means this deployment named none and the boot path resolves
    /// [`afd_cron::qstash::API_BASE`] — see [`super::QSTASH_URL_KNOB`].
    #[must_use]
    pub fn qstash_url(&self) -> Option<&str> {
        self.qstash_url.as_deref()
    }

    /// What a signup event from the identity provider is verified against.
    ///
    /// `None` refuses every delivery — see [`super::IDENTITY_WEBHOOK_SECRET_KNOB`].
    #[must_use]
    pub fn identity_webhook_secret(&self) -> Option<&str> {
        self.identity_webhook_secret.as_deref()
    }

    /// The scheduler's signing keys, when this deployment configured them.
    ///
    /// `None` refuses every fire — see [`super::QSTASH_CURRENT_KEY_KNOB`].
    #[must_use]
    pub const fn qstash_signing_keys(&self) -> Option<&SigningKeys> {
        self.qstash_keys.as_ref()
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
