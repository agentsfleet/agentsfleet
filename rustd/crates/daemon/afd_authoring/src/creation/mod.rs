//! One retained fleet identity moves through bounded setup requests and checked activation.

mod activate;
mod chat_save;
mod conversations;
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
use afd_admission::Admissions;
use afd_approval::IntegrationGrants;

pub use progress::{SetupCheck, SetupProgress};
pub use read::SavedFleet;
pub use system::SystemSetup;

/// Native Save and explicit setup recovery; it never spawns a creation worker.
#[derive(Debug, Clone)]
pub struct Creations {
    setup: Setup,
    admissions: Admissions,
    grants: IntegrationGrants,
    schedules: afd_cron::Schedules,
}

impl Creations {
    pub(crate) fn activity_streams(&self) -> afd_dragonfly::FleetStreams {
        self.admissions.streams()
    }

    /// Shares the daemon's ledger, queue, pool and credential inspection service.
    #[must_use]
    pub fn new(setup: Setup, admissions: Admissions) -> Self {
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
        }
    }
}

const CONTEXT: &str = "advance reviewed fleet setup";
const INSTALLING: &str = "installing";
const ACTIVE: &str = "active";
const KILLED: &str = "killed";
const INITIAL_VERSION: i64 = 1;
/// How long a closed draft's private transcript outlives activation.
const PRIVATE_RETENTION: std::time::Duration = afd_core::timing::DAY.saturating_mul(30);

fn etag(id: &afd_core::id::Uuid7, version: i64) -> String {
    crate::draft::etag(id, version)
}
