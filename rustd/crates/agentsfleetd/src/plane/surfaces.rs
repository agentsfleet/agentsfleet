//! The serving plane's three configured surfaces: authoring, the outbound
//! family that dials vendors, and device-flow login.
//!
//! Each takes something [`PlaneParts`](super::PlaneParts) carries by value — a
//! transport, a schedule configuration, a login configuration — which is what
//! separates them from the pool-only stores in [`super::stores`]: the order the
//! plane builds them in is decided by who moves what, and each builder below
//! documents its own.

use std::sync::Arc;

use afd_admission::Admissions;
use afd_credential::provider::Providers;
use afd_cron::SigningKeys;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_dragonfly::Dragonfly;
use afd_tenant::session::Sessions as Logins;

use super::config::{LoginConfig, ScheduleConfig};
use super::{config, platform};

/// The authoring drafts, the setup over them, the model calls they make, and
/// the creations that admit a finished draft.
#[derive(Debug)]
pub(super) struct Authoring {
    pub(super) drafts: afd_authoring::Drafts,
    pub(super) setup: afd_authoring::Setup,
    pub(super) creations: afd_authoring::Creations,
    pub(super) models: afd_authoring::ModelCalls,
}

/// Builds the authoring surfaces over `drafts`, each over the one before it.
///
/// `drafts` already carries the pool, the biller and the analytics reporter,
/// so every surface below reports its funnel steps and moves money the same
/// way. The setup wraps the drafts, the model calls wrap the drafts and the
/// shared provider store, and the creations wrap the setup and the shared
/// admissions ledger — so the order below is the dependency order, not a
/// preference.
pub(super) fn authoring(
    drafts: afd_authoring::Drafts,
    queue: &Dragonfly,
    kek: &Arc<Kek>,
    providers: &Providers,
    admissions: &Admissions,
    transport: afd_authoring::Network,
) -> Authoring {
    let setup = afd_authoring::Setup::new(drafts.clone(), Arc::clone(kek));
    let models =
        afd_authoring::ModelCalls::new(drafts.clone(), providers.clone(), Arc::new(transport))
            .with_progress(afd_dragonfly::FleetStreams::new(queue.clone()));
    let creations = afd_authoring::Creations::new(setup.clone(), queue.clone(), admissions.clone());
    Authoring {
        drafts,
        setup,
        creations,
        models,
    }
}

/// What the plane sends outbound: schedules registered with the external
/// scheduler, the connect flow, and the interim message poster.
#[derive(Debug)]
pub(super) struct Outbound {
    pub(super) schedules: afd_api::SchedulePlane,
    pub(super) schedule_keys: Option<SigningKeys>,
    pub(super) schedule_destination: String,
    pub(super) connectors: afd_connector::Connectors,
    pub(super) interjector: afd_outbound::Interjector,
}

/// What [`outbound`] consumes, moved out of the plane's parts whole.
///
/// Grouped so the call site names the four values the builder consumes apart
/// from the three it only borrows.
#[derive(Debug)]
pub(super) struct OutboundParts {
    pub(super) schedule: ScheduleConfig,
    pub(super) admissions: Admissions,
    pub(super) vendor_client: reqwest::Client,
    pub(super) slack_api_base: String,
}

/// Builds the outbound family.
///
/// The destination is copied and the signing keys taken BEFORE the schedule
/// plane consumes the configuration, because the plane keeps both to verify a
/// fire. The destination is derived from the API url this deployment already
/// knows, so a schedule registered upstream calls back to the daemon that
/// registered it; a url carrying a query or a fragment is refused at
/// construction rather than silently truncating the callback — see
/// `qstash::destination_url`.
///
/// The exchange dials the vendor and the Jira site listing dials it again, so
/// the interjector and the connect flow both take `vendor_client`: one pool for
/// everything this family sends outbound — see `crate::credentials`. The
/// interjector clones it, so the connect flow can take it last.
pub(super) fn outbound(
    database: &Db,
    queue: &Dragonfly,
    kek: &Arc<Kek>,
    parts: OutboundParts,
) -> Outbound {
    let OutboundParts {
        mut schedule,
        admissions,
        vendor_client,
        slack_api_base,
    } = parts;
    let schedule_destination = schedule.destination.clone();
    let schedule_keys = schedule.keys.take();
    let schedules = config::schedule_plane(database, schedule, admissions);
    let interjector = platform::interjector(database, kek, vendor_client.clone(), slack_api_base);
    let connectors = platform::connect_flow(database, kek, queue, vendor_client);
    Outbound {
        schedules,
        schedule_keys,
        schedule_destination,
        connectors,
        interjector,
    }
}

/// The device-flow login surface and the two deployment urls beside it.
#[derive(Debug)]
pub(super) struct Login {
    pub(super) logins: Logins,
    pub(super) app_url: afd_connector::Dashboard,
    pub(super) api_url: Box<str>,
}

/// Builds the login surface over its own session store.
///
/// The surface BORROWS the dashboard url, so it is built before the url moves
/// into the returned value.
pub(super) fn login(login: LoginConfig, queue: &Dragonfly) -> Login {
    let logins = Logins::new(
        afd_dragonfly::SessionStore::new(queue.clone()),
        login.code_pepper,
        Entropy::new(),
        login.app_url.as_str(),
    );
    Login {
        logins,
        app_url: login.app_url,
        api_url: login.api_url,
    }
}
