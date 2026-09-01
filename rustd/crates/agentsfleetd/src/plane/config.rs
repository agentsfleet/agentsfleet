//! The three parameter objects the serving plane is built from.
//!
//! Split from `plane.rs` at the file cap, along the line between what the plane
//! IS and what it is HANDED. Each of these exists for the same reason — a
//! constructor that already takes seven arguments cannot take eight more
//! without a caller being able to transpose two of them silently — so they
//! belong together, and the plane itself is easier to read without them
//! between its constructor and its trait impl.
//!
//! Re-exported from the parent, so every caller still names them there.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_cron::SigningKeys;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_observability::Analytics;
use afd_redis::Redis;
use afd_sse::Live;

use crate::bundles::Stores;
use crate::identity::{Capabilities, Sessions};

/// Everything [`ServingPlane::new`] is assembled from.
///
/// A parameter object rather than eight positional arguments, and not only to
/// satisfy a lint. Each field is CONNECTED or BUILT before it gets here, which
/// is the property the constructor's own note is about — boot has already
/// proven the pools answer, resolved the snapshot store's absence into a value,
/// and read this deployment's platform credentials out of the vault. Naming
/// them at the call site is what makes that readable in one place.
#[derive(Debug)]
pub struct PlaneParts {
    /// The API role's Postgres pool, open and proven.
    pub database: Db,
    /// The API role's Redis, open and proven.
    pub queue: Redis,
    /// The master key every stored credential is sealed under.
    ///
    /// Already shared: `preflight` resolved and validated it and refuses boot
    /// without one, so every store below that opens a sealed row takes the SAME
    /// key — Milestone Invariant 3 as an ownership fact rather than as a rule
    /// about who reads which variable.
    pub kek: Arc<Kek>,
    /// Where a subject's capability claim is read from.
    pub capabilities: Capabilities,
    /// What verifies a browser session token.
    pub sessions: Sessions,
    /// The object-store handles, read and upload, over one owner.
    ///
    /// Split inside [`ServingPlane::new`] rather than out here, because the two
    /// halves are one configuration decision: a deployment either set the R2
    /// knobs or did not, and handing over two independently-built values would
    /// let a caller pair a live reader with an absent writer.
    pub stores: Stores,
    /// The credential broker, built before the plane because it reads the
    /// vault, which is asynchronous where this constructor is not.
    pub broker: Arc<afd_credential::credential::Broker>,
    /// Where product events go, holding its own absence.
    ///
    /// Not an `Option`, for the reason [`PlaneParts::bundles`] is not: a
    /// deployment naming no `PostHog` project reports nothing, and a caller that
    /// had to ask before reporting is a caller that can forget.
    pub analytics: Analytics,
    /// The live-stream surface, holding its own absence.
    ///
    /// Not an `Option`, for the reason [`PlaneParts::bundles`] is not: an
    /// instance whose pub/sub connection could not be opened still SERVES the
    /// stream routes, silently, and `afd_sse::Live::detached` is that case as a
    /// value rather than as a `None` this file would unwrap into a refusal.
    /// Built before the plane because opening the hub is asynchronous where
    /// this constructor is not.
    pub live: Live,
    /// What the device-flow login surface needs from configuration.
    pub login: LoginConfig,
    /// The workspace holding this deployment's own platform secrets.
    ///
    /// `None` for a deployment that configured none. Threaded through rather
    /// than re-read, because `preflight` has already parsed and validated it
    /// and a second reader could disagree with the first.
    pub platform_admin_workspace: Option<Uuid7>,
    /// What a signup event from the identity provider is verified against.
    ///
    /// Threaded through rather than re-read, for the reason
    /// [`PlaneParts::platform_admin_workspace`] is: `preflight` has already
    /// resolved it and a second reader could disagree with the first. `None`
    /// refuses every delivery — see `preflight::IDENTITY_WEBHOOK_SECRET_KNOB`.
    pub identity_webhook_secret: Option<SecretBytes>,
    /// What the schedules surface and the fire ingress need from configuration.
    pub schedule: ScheduleConfig,
}

/// What the schedules surface needs from configuration.
///
/// A struct rather than four more positional parameters, and for a sharper
/// reason than length: `token` and the two signing keys are all opaque strings,
/// so two of them transposed would compile and fail only as a 401 from the
/// vendor that reads like a wrong credential.
#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    /// The client the management calls go out on.
    pub client: reqwest::Client,
    /// This deployment's bearer for the external scheduler.
    pub token: String,
    /// Where a fire is expected to arrive — see [`qstash::destination_url`].
    pub destination: String,
    /// Which scheduler deployment the management calls go to.
    ///
    /// Resolved at boot rather than defaulted in the client, so a deployment
    /// falling back to the vendor's US region is a visible decision in one
    /// place — see [`crate::preflight::QSTASH_URL_KNOB`].
    pub api_base: String,
    /// The scheduler's signing keys, when this deployment configured them.
    ///
    /// `None` is fail-closed: every fire is refused, because a daemon that
    /// cannot verify a callback must not act on one.
    pub keys: Option<SigningKeys>,
}

/// What the device-flow login surface needs from configuration.
///
/// A struct rather than two more positional parameters on a constructor that
/// already takes seven: a `SecretBytes` and a `String` next to each other are
/// two arguments a caller can transpose without the compiler noticing, and the
/// consequence would be a pepper rendered into every login URL.
#[derive(Debug, Clone)]
pub struct LoginConfig {
    /// The key a verification code's digest is taken under.
    pub code_pepper: SecretBytes,
    /// Where a person goes to approve a login.
    pub app_url: String,
    /// This deployment's own base URL, as a minted credential records it.
    ///
    /// Beside `app_url` because the two are read from configuration together
    /// and are the same kind of fact — where a person goes, and where this
    /// daemon answers. Never a request's `Host`: a credential and the
    /// deployment that minted it are one fact, and a client-asserted host
    /// would let them disagree.
    pub api_url: Box<str>,
}
