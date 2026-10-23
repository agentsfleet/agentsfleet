//! Native authoring stores share one fixture database, key and admission ledger.

use super::*;

pub(super) struct Stores {
    pub(super) drafts: afd_authoring::Drafts,
    pub(super) setup: afd_authoring::Setup,
    pub(super) models: afd_authoring::ModelCalls,
    pub(super) creations: afd_authoring::Creations,
}

pub(super) fn stores(
    database: &Db,
    ledger: &afd_admission::Admissions,
    providers: &HarnessProviders,
) -> Stores {
    let drafts = afd_authoring::Drafts::new(
        database.clone(),
        Entropy::new(),
        std::sync::Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
            database.clone(),
        )),
    );
    let setup = afd_authoring::Setup::new(drafts.clone(), Arc::new(Kek::from_bytes(FIXTURE_KEK)));
    Stores {
        creations: afd_authoring::Creations::new(setup.clone(), ledger.clone()),
        models: afd_authoring::ModelCalls::new(
            drafts.clone(),
            providers.store().clone(),
            Arc::new(
                afd_authoring::Network::new()
                    .expect("fixed provider client builds without sending"),
            ),
        ),
        drafts,
        setup,
    }
}

impl Fleet {
    pub(crate) fn with_authoring_transport(
        mut self,
        transport: Arc<dyn afd_authoring::Transport>,
    ) -> Self {
        self.authoring_models = afd_authoring::ModelCalls::new(
            self.authoring.clone(),
            self.providers.store().clone(),
            transport,
        );
        self
    }
}
