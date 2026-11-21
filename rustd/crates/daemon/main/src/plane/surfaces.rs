//! The serving plane's three configured surfaces: setup, the outbound
//! family that dials vendors, and device-flow login.
//!
//! Each takes something [`PlaneParts`](super::PlaneParts) carries by value — a
//! transport, a schedule configuration, a login configuration — which is what
//! separates them from the pool-only stores in [`super::stores`]: the order the
//! plane builds them in is decided by who moves what, and each builder below
//! documents its own.

use std::sync::Arc;

use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_db::Db;
use afd_tenant::session::Sessions as Logins;
use afd_turns::Turns;

use super::config::{LoginConfig, ScheduleConfig};
use super::{config, platform};

/// The setup drafts, the setup over them, and the creations that admit a
/// finished draft. Setup's model calls are the runner's, so none is built
/// here.
#[derive(Debug)]
pub(super) struct SetupStores {
    pub(super) drafts: afd_setup::Drafts,
    pub(super) setup: afd_setup::Setup,
    pub(super) creations: afd_setup::Creations,
}

/// Builds the setup surfaces over `drafts`, each over the one before it.
///
/// `drafts` already carries the pool and the analytics reporter, so every
/// surface below reports its funnel steps the same way. The setup wraps the
/// drafts, and the creations wrap the setup and the shared turns ledger — so
/// the order below is the dependency order, not a preference.
pub(super) fn setup(drafts: afd_setup::Drafts, kek: &Arc<Kek>, turns: &Turns) -> SetupStores {
    let setup = afd_setup::Setup::new(drafts.clone(), Arc::clone(kek));
    let creations = afd_setup::Creations::new(setup.clone(), turns.clone());
    SetupStores {
        drafts,
        setup,
        creations,
    }
}

/// What the plane sends outbound: schedules registered with the external
/// scheduler, the connect flow, and the interim message poster.
#[derive(Debug)]
pub(super) struct Outbound {
    pub(super) schedules: afd_http::services::SchedulePlane,
    /// The scheduler the fire route verifies through, and a chat save
    /// registers its trigger schedules through.
    pub(super) scheduler: Arc<dyn afd_cron::Scheduler>,
    pub(super) connectors: afd_connector::Connectors,
    pub(super) interjector: afd_outbound::Interjector,
}

/// What [`outbound`] consumes, moved out of the plane's parts whole.
///
/// Grouped so the call site names the five values the builder consumes apart
/// from the two it only borrows.
#[derive(Debug)]
pub(super) struct OutboundParts {
    pub(super) schedule: ScheduleConfig,
    pub(super) admin: Option<af_core::id::Uuid7>,
    pub(super) turns: Turns,
    pub(super) vendor_client: reqwest::Client,
    pub(super) slack_api_base: String,
}

/// Builds the outbound family.
///
/// The scheduler is shared BEFORE the schedule plane takes it, because the
/// plane keeps it to verify a fire: it reads the signing keys from the
/// `qstash` bag on each one. Its callback is derived from the API url this
/// deployment already knows, so a schedule registered upstream calls back to
/// the daemon that registered it; preflight refuses an API url that cannot
/// form one — see `qstash::destination_url`.
///
/// The exchange dials the vendor and the Jira site listing dials it again, so
/// the interjector and the connect flow both take `vendor_client`: one pool for
/// everything this family sends outbound — see `crate::credentials`. The
/// interjector clones it, so the connect flow can take it last.
pub(super) fn outbound(database: &Db, kek: &Arc<Kek>, parts: OutboundParts) -> Outbound {
    let OutboundParts {
        schedule,
        admin,
        turns,
        vendor_client,
        slack_api_base,
    } = parts;
    let upstream = platform::qstash(database, kek, admin, schedule);
    let bus = turns.streams();
    let schedules = config::schedule_plane(database, Arc::clone(&upstream), turns);
    let interjector = platform::interjector(database, kek, vendor_client.clone(), slack_api_base);
    let connectors = platform::connect_flow(database, kek, &bus, vendor_client);
    Outbound {
        schedules,
        scheduler: upstream,
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
pub(super) fn login(login: LoginConfig, database: &Db) -> Login {
    let logins = Logins::new(
        afd_tenant::session::DeviceSessions::new(database.clone()),
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
