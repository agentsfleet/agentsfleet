//! Publisher and restorer use the same complete link-graph rules.

use crate::{Result, error};

/// Check individual targets before the shared graph resolver follows their components.
pub(super) fn validate<'a>(links: impl Iterator<Item = (&'a str, &'a str)>) -> Result<()> {
    let links = links
        .map(|(path, target)| {
            super::files::valid_link(path, target)?;
            Ok((path, target))
        })
        .collect::<Result<Vec<_>>>()?;
    af_validate::checkpoint_links(links.into_iter())
        .map_err(|source| error::boundary("checkpoint link graph is invalid", source))
}
