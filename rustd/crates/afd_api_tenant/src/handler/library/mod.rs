//! Native reusable setup is published explicitly from private chat candidates.

pub(crate) mod list;
pub(crate) mod publish;
pub(crate) mod publish_revision;
pub(crate) mod read;
pub(crate) mod remove;
mod render;

use crate::auth::{Owned, PersonIdentity};
use afd_authoring::Access;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct LibraryPath {
    library_id: String,
}
#[derive(Deserialize)]
pub(crate) struct RevisionPath {
    library_id: String,
    revision_id: String,
}

fn access<'a>(owned: &'a Owned, person: &'a PersonIdentity) -> Access<'a> {
    Access {
        workspace: &owned.workspace,
        subject: person.subject(),
    }
}
