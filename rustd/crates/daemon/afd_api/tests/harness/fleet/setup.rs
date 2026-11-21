//! Native setup stores share one fixture database, key and turns ledger.

use super::*;

pub(super) struct Stores {
    pub(super) drafts: afd_setup::Drafts,
    pub(super) setup: afd_setup::Setup,
    pub(super) creations: afd_setup::Creations,
}

pub(super) fn stores(database: &Db, ledger: &afd_turns::Turns) -> Stores {
    let drafts = afd_setup::Drafts::new(database.clone(), Entropy::new());
    let setup = afd_setup::Setup::new(drafts.clone(), Arc::new(Kek::from_bytes(FIXTURE_KEK)));
    Stores {
        creations: afd_setup::Creations::new(setup.clone(), ledger.clone()),
        drafts,
        setup,
    }
}
