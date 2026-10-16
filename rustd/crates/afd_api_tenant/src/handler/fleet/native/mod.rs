//! Browser and terminal Save share retained setup, live checks and strong versions.

pub(crate) mod create;
mod input;
pub(crate) mod patch;
pub(crate) mod read;
mod render;
pub(crate) mod revision;
mod schedules;

use crate::auth::{Owned, PersonIdentity};
use afd_authoring::Access;

fn access<'a>(owned: &'a Owned, person: &'a PersonIdentity) -> Access<'a> {
    Access {
        workspace: &owned.workspace,
        subject: person.subject(),
    }
}
