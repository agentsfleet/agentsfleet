//! The fleet store shares one fixture database and turns ledger.

use super::*;

pub(super) fn creations(database: &Db, ledger: &afd_turns::Turns) -> afd_setup::Creations {
    afd_setup::Creations::new(database.clone(), Entropy::new(), ledger.clone())
}
