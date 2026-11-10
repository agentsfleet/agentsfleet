//! Construction groups retain each shared owner while the serving plane is assembled.

use super::{PlaneParts, lease_plane, surfaces, task_plane};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use af_observability::Analytics;
use afd_admission::Admissions;
use afd_billing::biller::Biller;
use afd_credential::provider::Providers;
use afd_db::Db;
use afd_fleet::lease::LeaseService;
use afd_runner::Runners;
use std::sync::Arc;

type StorageConfig = Option<garde::Valid<afd_fleet::task::storage::CheckpointStorageConfig>>;

pub(super) struct Common {
    pub(super) providers: Providers,
    pub(super) github_client: reqwest::Client,
    pub(super) task_master: Arc<Kek>,
    pub(super) admissions: Admissions,
    pub(super) biller: Arc<dyn Biller>,
    pub(super) drafts: afd_authoring::Drafts,
}

impl Common {
    pub(super) fn new(parts: &PlaneParts) -> Self {
        let providers = Providers::new(
            parts.database.clone(),
            Arc::clone(&parts.kek),
            Entropy::new(),
        );
        let github_client = parts.authoring_transport.guarded_client();
        let task_master = Arc::clone(&parts.kek);
        let admissions = Admissions::new(parts.database.clone(), parts.bus.clone(), Entropy::new());
        let biller = crate::billing::biller(parts.database.clone());
        let drafts =
            afd_authoring::Drafts::new(parts.database.clone(), Entropy::new(), Arc::clone(&biller))
                .with_analytics(parts.analytics.clone());
        Self {
            providers,
            github_client,
            task_master,
            admissions,
            biller,
            drafts,
        }
    }
}

pub(super) struct NativeInputs<'a> {
    pub(super) database: &'a Db,
    pub(super) kek: Arc<Kek>,
    pub(super) providers: Providers,
    pub(super) bus: afd_bus::Handle,
    pub(super) broker: Arc<afd_credential::credential::Broker>,
    pub(super) biller: Arc<dyn Biller>,
    pub(super) wake: afd_bus::wake::Wake,
    pub(super) admissions: Admissions,
    pub(super) admin_workspace: Option<Uuid7>,
    pub(super) storage_config: StorageConfig,
    pub(super) master: Arc<Kek>,
    pub(super) client: reqwest::Client,
    pub(super) authoring: &'a surfaces::Authoring,
    pub(super) stream_limit: afd_sse::StreamLimit,
    pub(super) frame_memory: af_stream::MemoryLimits,
    pub(super) analytics: Analytics,
}

pub(super) struct Native {
    pub(super) leases: LeaseService,
    pub(super) tasks: afd_fleet::task::Tasks,
    pub(super) runners: Runners,
    pub(super) streams: afd_gateway::Gateway,
}

impl NativeInputs<'_> {
    pub(super) fn build(self) -> Native {
        let mut leases = lease_plane(
            self.database.clone(),
            self.kek,
            self.providers,
            self.bus.clone(),
            self.broker,
            self.biller,
        );
        leases.leases = leases.leases.with_wake(self.wake.clone());
        let tasks = task_plane(
            &mut leases,
            self.admissions,
            self.admin_workspace,
            self.storage_config,
            self.master,
            self.client,
        );
        let runners = Runners::new(self.database.clone(), Entropy::new());
        let streams = afd_gateway::Gateway::new(afd_gateway::GatewayDependencies {
            database: self.database.clone(),
            runners: runners.clone(),
            leases: leases.clone(),
            drafts: self.authoring.drafts.clone(),
            models: self.authoring.models.clone(),
            creations: self.authoring.creations.clone(),
            bus: self.bus,
            stream_limit: self.stream_limit,
            wake: self.wake,
            frame_memory: self.frame_memory,
            wall_clock: Arc::new(af_core::clock::SystemClock),
            analytics: self.analytics,
        });
        Native {
            leases,
            tasks,
            runners,
            streams,
        }
    }
}
