//! What boot opens, and what it starts beside it.
//!
//! Split from `serve` on the same line `exporting` was: boot's job is to open
//! things and hand them on, and these are the pools, the plane and the
//! background workers it opens. `serve` keeps the sequence; this keeps what
//! each step in it constructs.

use std::sync::Arc;
use std::time::Duration;

use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_observability::Analytics;
use afd_redis::Redis;

use super::optional::{announce_identity, open_live};
use crate::error::BootFailure;
use crate::plane::{ServingPlane, Shared};
use crate::preflight::BootConfig;
use crate::supervisor::Supervisor;

/// How long boot spends establishing the pool's floor before it serves.
///
/// The floor is a quarter of the ceiling and each connection costs the 147-337
/// ms this lane measures, so the whole warm-up fits here several times over.
/// It is a deadline and not a requirement: [`Db::warm`] cannot fail, and a pool
/// that did not fill is a slower pool rather than a broken one — the datastore
/// was already proven reachable by `Db::connect`. Boot proceeds either way.
const POOL_WARM_DEADLINE: Duration = Duration::from_secs(5);

pub(super) struct Runtime {
    pub(super) database: Db,
    pub(super) queue: Redis,
    /// The live-stream surface, kept beside the plane it was moved into.
    ///
    /// A `Live` is two handles, so this is the same ceiling the routes admit
    /// against — which is the point: a gauge reading a different value from
    /// the one that decides admission would report a number no shed agrees
    /// with.
    pub(super) live: afd_sse::Live,
    pub(super) plane: Shared,
    pub(super) hub: Option<afd_redis::SubscriptionHub>,
    /// The same key the plane seals with. The outbound worker opens its own
    /// grant store over it, so boot hands one key to both rather than reading
    /// the knob twice.
    pub(super) kek: Arc<Kek>,
}

pub(super) async fn open_runtime(config: &BootConfig, analytics: &Analytics) -> Result<Runtime, BootFailure> {
    let database = Db::connect(config.api_pool()).await?;
    // Before the router exists, so the first request finds live connections
    // instead of paying a handshake inside an acquire budget sized for a wait.
    // sqlx does not do this itself: it bootstraps `min_connections` only when
    // `idle_timeout` and `max_lifetime` are both unset, and its own defaults
    // set both. `warm` reports its shortfall through `pool_warm_incomplete`.
    database.warm(POOL_WARM_DEADLINE).await;
    let queue = Redis::connect(config.redis()).await?;
    let (capabilities, sessions) = crate::identity::resolve(config.identity());
    announce_identity(&capabilities);
    let kek = Arc::new(config.kek().clone());
    let broker = crate::credentials::resolve(
        &afd_credential::vault::Vault::new(database.clone(), Arc::clone(&kek)),
        config.platform_admin_workspace(),
    )
    .await;
    let live = open_live(config.redis(), config.sse_max_streams()).await;
    let hub = live.hub().cloned();
    let observed = live.clone();
    let plane = Arc::new(ServingPlane::new(crate::plane::PlaneParts {
        database: database.clone(),
        queue: queue.clone(),
        // Cloned rather than moved: the outbound worker below opens its own
        // grant store over the same key, for the reason `crate::outbound`
        // gives — it runs beside the plane, not through it.
        kek: Arc::clone(&kek),
        capabilities,
        sessions,
        stores: crate::bundles::resolve(config.bundles()),
        platform_admin_workspace: config.platform_admin_workspace().cloned(),
        // Fail-closed: a deployment that named no secret refuses every signup
        // delivery rather than trusting an unverified one to open an account.
        identity_webhook_secret: config
            .identity_webhook_secret()
            .map(|secret| afd_crypto::secret::SecretBytes::new(secret.as_bytes().to_vec())),
        broker,
        live,
        analytics: analytics.clone(),
        // A destination that will not build is a deployment that cannot
        // register schedules, and it fails CLOSED rather than registering a
        // truncated callback: the empty string matches no token's subject, so
        // every fire is refused until the api url is corrected.
        schedule: crate::plane::ScheduleConfig {
            client: reqwest::Client::new(),
            token: config.qstash_token().unwrap_or_default().to_owned(),
            destination: afd_cron::qstash::destination_url(config.api_url()).unwrap_or_default(),
            // The one place the vendor's US region is chosen, and only when this
            // deployment named no scheduler of its own.
            api_base: config
                .qstash_url()
                .unwrap_or(afd_cron::qstash::API_BASE)
                .to_owned(),
            keys: config.qstash_signing_keys().cloned(),
        },
        login: crate::plane::LoginConfig {
            code_pepper: config.session_code_pepper().clone(),
            app_url: config.app_url().to_owned(),
            api_url: config.api_url().into(),
        },
    }));
    Ok(Runtime {
        database,
        queue,
        live: observed,
        plane,
        hub,
        kek,
    })
}

pub(super) async fn spawn_background(
    supervisor: &mut Supervisor,
    config: &BootConfig,
    database: &Db,
    queue: &Redis,
    kek: &Arc<Kek>,
    hub: Option<afd_redis::SubscriptionHub>,
) {
    if let Some(hub) = hub {
        supervisor.spawn(crate::HUB_PUMP, move |token| async move {
            token.cancelled().await;
            hub.shutdown();
        });
    }
    // The sweepers read through pools that are open by now. Starting them
    // after the socket would leave a window where the plane serves while
    // nothing is noticing dead runners.
    crate::sweepers::spawn(supervisor, database, queue);
    // The connector answer-delivery worker, beside them and for the same
    // reason. Its own Redis connection, because it blocks on the stream — see
    // `crate::outbound`. It opens its own grant store over the SAME key the
    // plane seals with, which is why the KEK arrives shared rather than
    // rebuilt here.
    crate::outbound::spawn(
        supervisor,
        config.redis(),
        database,
        queue,
        afd_connector::Grants::new(
            afd_vault::Vault::new(database.clone(), Arc::clone(kek), Entropy::new()),
            database.clone(),
            Entropy::new(),
        ),
        crate::credentials::vendor_exchange_client(),
    )
    .await;
}