//! Live authoring services share their existing pools, keys and provider boundary with the gateway.

use super::super::*;
use afd_fleet::lease::{LeaseService, Leases};
use afd_gateway::{Gateway, GatewayDependencies};

impl Fleet {
    pub(crate) fn with_native_authoring(mut self, database: Db) -> Self {
        self.native = Some(database);
        self
    }

    fn authoring_gateway(&self, database: Db) -> Gateway {
        let bus = self.runner_commands.clone();
        let biller = Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
            database.clone(),
        ));
        let key = Arc::new(Kek::from_bytes(FIXTURE_KEK));
        let leases = LeaseService {
            task_reports: None,
            leases: Leases::new(database.clone(), bus.clone(), Entropy::new()),
            gates: afd_gate::gate::Gates::new(database.clone(), bus.clone(), Entropy::new()),
            accounts: afd_billing::Accounts::new(database.clone()),
            biller,
            memories: Memories::new(database.clone(), Entropy::new()),
            providers: self.providers.store().clone(),
            vault: afd_credential::vault::Vault::new(database.clone(), key),
            broker: Arc::new(afd_credential::credential::Broker::new(
                Arc::new(afd_credential::secrets::Registry::default()),
                Arc::new(afd_credential::credential::Vendors::new(
                    afd_credential::credential::platform::Platform::empty(),
                    afd_identity::direct(af_core::timing::STREAM_ANSWER_TIMEOUT)
                        .build()
                        .expect("unused vendor boundary"),
                )),
            )),
            grants: IntegrationGrants::new(database.clone(), Entropy::new()),
            thread: Arc::new(self.events.clone()),
        };
        Gateway::new(GatewayDependencies {
            database,
            runners: self.runners.clone(),
            leases,
            drafts: self.authoring.clone(),
            models: self.authoring_models.clone(),
            creations: self.creations.clone(),
            bus,
            stream_limit: self.live.stream_limit().clone(),
            wake: afd_bus::wake::Wake::default(),
            frame_memory: af_stream::MemoryLimits::default(),
            wall_clock: Arc::new(af_core::clock::SystemClock),
            analytics: self.analytics.clone(),
        })
    }
}

impl afd_api::services::RunnerStreams for Fleet {
    async fn open(
        &self,
        runner: afd_auth::principal::Runner,
        request: axum::extract::Request,
    ) -> axum::response::Response {
        use afd_api::services::RunnerStreams as _;
        match &self.native {
            Some(database) => {
                self.authoring_gateway(database.clone())
                    .open(runner, request)
                    .await
            }
            None => self.leases.open(runner, request).await,
        }
    }
}
