//! One retained fleet identity moves through bounded setup requests and checked activation.

mod activate;
pub(crate) mod chat_save;
mod conversations;
mod delete;
mod patch;
mod progress;
mod read;
mod reserve;
mod revision;
mod schedules;
mod source;
mod sql;
mod start;
mod stored;
mod system;

use crate::Setup;
use afd_approval::IntegrationGrants;
use afd_turns::Turns;

pub use progress::{SetupCheck, SetupProgress};
pub use read::SavedFleet;
pub use system::SystemSetup;

/// Native Save and explicit setup recovery; it never spawns a creation worker.
#[derive(Debug, Clone)]
pub struct Creations {
    pub(crate) setup: Setup,
    admissions: Turns,
    grants: IntegrationGrants,
    schedules: afd_cron::Schedules,
    /// What a chat save registers an active fleet's trigger schedules
    /// through; without one they wait for `:sync`.
    registrar: Option<afd_cron::ScheduleService>,
}

impl Creations {
    /// Registers a chat-saved fleet's trigger schedules through `scheduler`,
    /// the one the schedule routes push through, as Save's route does.
    #[must_use]
    pub fn with_scheduler(mut self, scheduler: std::sync::Arc<dyn afd_cron::Scheduler>) -> Self {
        let store = self
            .schedules
            .clone()
            .with_activity_streams(self.activity_streams());
        self.registrar = Some(afd_cron::ScheduleService::new(store, scheduler));
        self
    }

    /// Bind authoring settlement and Save/Apply to the confirmed native owner.
    #[must_use]
    pub fn for_connection(mut self, authority: afd_runner::capacity::authority::Authority) -> Self {
        self.setup.drafts = self.setup.drafts.for_connection(authority);
        self
    }

    pub(crate) fn activity_streams(&self) -> afd_bus::Handle {
        self.admissions.streams()
    }

    /// Shares the daemon's ledger, queue, pool and credential inspection service.
    #[must_use]
    pub fn new(setup: Setup, admissions: Turns) -> Self {
        Self {
            schedules: afd_cron::Schedules::new(
                setup.drafts.database.clone(),
                setup.drafts.entropy.clone(),
            ),
            grants: IntegrationGrants::new(
                setup.drafts.database.clone(),
                setup.drafts.entropy.clone(),
            ),
            setup,
            admissions,
            registrar: None,
        }
    }
}

const CONTEXT: &str = "advance reviewed fleet setup";
// Spelled by the lifecycle's own enum, so the authoring checks and the fleet
// row can never name a status two ways.
const INSTALLING: &str = afd_fleet_lifecycle::FleetStatus::Installing.as_str();
const ACTIVE: &str = afd_fleet_lifecycle::FleetStatus::Active.as_str();
const ARCHIVED: &str = afd_fleet_lifecycle::FleetStatus::Archived.as_str();
const INITIAL_VERSION: i64 = 1;
/// How long a closed draft's private transcript outlives activation.
const PRIVATE_RETENTION: std::time::Duration = af_core::timing::DAY.saturating_mul(30);

fn etag(id: &af_core::id::Uuid7, version: i64) -> String {
    crate::draft::etag(id, version)
}
