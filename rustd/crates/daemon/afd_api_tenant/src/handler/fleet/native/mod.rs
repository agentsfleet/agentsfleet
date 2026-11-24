//! A fleet as a person creates, reads and changes it: the first message makes
//! it, its row is read with its triggers and grants, and a change takes the
//! row's strong tag.

pub(crate) mod create;
mod input;
pub(crate) mod patch;
pub(crate) mod read;
mod render;
pub(crate) mod triggers;

use crate::auth::{Owned, PersonIdentity};
use afd_setup::Access;

fn access<'a>(owned: &'a Owned, person: &'a PersonIdentity) -> Access<'a> {
    Access {
        workspace: &owned.workspace,
        subject: person.subject(),
    }
}
