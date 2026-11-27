//! A generation's delta chain: the generations its pages are applied over,
//! read the same way for a turn's checkpoints and a task's.
//!
//! ```text
//!   full ◄─ delta ◄─ delta ◄─ this     parent_checkpoint_id, followed back
//!   links / pages answered full first, so applying them in order resolves the tree
//! ```

use af_core::id::Uuid7;
use af_wire::durable_task::CheckpointLink;
use futures_util::TryStreamExt as _;
use sqlx::PgConnection;

use crate::{Result, error::query};

/// One stored page of a chain's generation.
#[derive(sqlx::FromRow)]
pub(crate) struct ChainPage {
    /// Its place in its generation's pages.
    pub(crate) page_number: i32,
    /// The digest its writer froze.
    pub(crate) digest: Option<String>,
    /// Its body.
    pub(crate) body: Option<Vec<u8>>,
}

#[derive(sqlx::FromRow)]
struct Link {
    id: Uuid7,
    page_ids: Vec<Uuid7>,
    manifest_root: Option<String>,
}

/// The generations `checkpoint`'s pages are a delta through, full first; an
/// ancestor with no recorded root is `None`, which no committed chain holds.
///
/// # Errors
/// The read failed.
pub(crate) async fn links(
    connection: &mut PgConnection,
    checkpoint: &Uuid7,
) -> Result<Option<Vec<CheckpointLink>>> {
    let rows: Vec<Link> = sqlx::query_as(include_str!("sql/chain_links.sql"))
        .bind(checkpoint)
        .fetch_all(connection)
        .await
        .map_err(query("read a checkpoint's delta chain"))?;
    Ok(rows
        .into_iter()
        .map(|link| {
            Some(CheckpointLink {
                checkpoint_id: link.id.to_string(),
                page_ids: link.page_ids.iter().map(ToString::to_string).collect(),
                manifest_root: link.manifest_root?,
            })
        })
        .collect())
}

/// Hands `each` every stored page of the generations `checkpoint` is a delta
/// through, the full manifest's first, each generation's in page order.
///
/// # Errors
/// The read failed, or `each` refused a page.
pub(crate) async fn pages(
    connection: &mut PgConnection,
    checkpoint: &Uuid7,
    mut each: impl FnMut(ChainPage) -> Result<()>,
) -> Result<()> {
    let mut rows = sqlx::query_as::<_, ChainPage>(include_str!("sql/chain_pages.sql"))
        .bind(checkpoint)
        .fetch(connection);
    while let Some(page) = rows
        .try_next()
        .await
        .map_err(query("stream a checkpoint's delta chain pages"))?
    {
        each(page)?;
    }
    Ok(())
}

/// Page `page` when it belongs to a generation `checkpoint` is a delta
/// through.
///
/// # Errors
/// The read failed.
pub(crate) async fn page(
    connection: &mut PgConnection,
    checkpoint: &Uuid7,
    page: &Uuid7,
) -> Result<Option<ChainPage>> {
    sqlx::query_as(include_str!("sql/chain_page.sql"))
        .bind(page)
        .bind(checkpoint)
        .fetch_optional(connection)
        .await
        .map_err(query("read a page of a checkpoint's delta chain"))
}
