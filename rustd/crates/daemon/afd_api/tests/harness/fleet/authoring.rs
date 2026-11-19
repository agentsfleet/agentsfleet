//! Native authoring stores share one fixture database, key and admission ledger.

use super::*;

pub(super) struct Stores {
    pub(super) drafts: afd_setup::Drafts,
    pub(super) setup: afd_setup::Setup,
    pub(super) models: afd_setup::ModelCalls,
    pub(super) creations: afd_setup::Creations,
}

pub(super) fn stores(
    database: &Db,
    ledger: &afd_turns::Turns,
    providers: &HarnessProviders,
) -> Stores {
    let drafts = afd_setup::Drafts::new(
        database.clone(),
        Entropy::new(),
        Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
            database.clone(),
        )),
    );
    let setup = afd_setup::Setup::new(drafts.clone(), Arc::new(Kek::from_bytes(FIXTURE_KEK)));
    Stores {
        creations: afd_setup::Creations::new(setup.clone(), ledger.clone()),
        models: afd_setup::ModelCalls::new(
            drafts.clone(),
            providers.store().clone(),
            Arc::new(
                afd_setup::Network::new().expect("fixed provider client builds without sending"),
            ),
        ),
        drafts,
        setup,
    }
}

impl Fleet {
    pub(crate) fn with_authoring_transport(
        mut self,
        transport: Arc<dyn afd_setup::Transport>,
    ) -> Self {
        self.authoring_models = afd_setup::ModelCalls::new(
            self.authoring.clone(),
            self.providers.store().clone(),
            transport,
        );
        self
    }
}
