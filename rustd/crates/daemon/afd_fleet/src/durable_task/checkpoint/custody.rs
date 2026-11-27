//! Who may upload an object, and when its bytes count as stored.
//!
//! Nothing reads an object back. Its upload link binds `If-None-Match: *` and
//! the ciphertext's SHA-256, and the bucket proved before activation that it
//! refuses a body unlike that checksum, so a PUT the bucket accepted stored
//! exactly the frozen ciphertext. The commit records that as `verified_at`.

use crate::{Result, error::query};
use af_core::id::Uuid7;
use sqlx::PgConnection;

/// The pin kind an upload is signed under.
const WRITER: &str = "writer";

/// The plan an object's upload belongs to: its checkpoint, run and fence.
pub(crate) type Owner<'a> = (&'a Uuid7, &'a Uuid7, i64);

/// Records `verified_at` for the objects of `owner` in `uploaded` whose
/// ciphertext an upload froze: the runner saw each one's bound PUT accepted.
/// An identifier of another plan, or of an object no upload froze, records
/// nothing, and the commit's completeness check then refuses.
pub(crate) async fn accept(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    owner: Owner<'_>,
    uploaded: &[Uuid7],
) -> Result<()> {
    sqlx::query(include_str!("sql/objects_accept.sql"))
        .bind(uploaded)
        .bind(workspace)
        .bind(owner.0)
        .bind(owner.1)
        .bind(owner.2)
        .execute(connection)
        .await
        .map_err(query("record the uploads the runner saw accepted"))?;
    Ok(())
}

/// Gives back the content of every unverified object of another plan that can
/// no longer upload, because no writer pin of its plan outlives now: a
/// publication that failed after its PUTs left them so. Deleting the row
/// records its purge, which waits out every link it was signed for, and the
/// registration that follows makes a new object in its place.
pub(crate) async fn supersede(
    connection: &mut PgConnection,
    (workspace, repository): (&Uuid7, &str),
    content: &[String],
    owner: Owner<'_>,
) -> Result<()> {
    sqlx::query(include_str!("sql/objects_supersede.sql"))
        .bind(workspace)
        .bind(repository)
        .bind(content)
        .bind(owner.0)
        .bind(owner.1)
        .bind(owner.2)
        .bind(WRITER)
        .execute(connection)
        .await
        .map_err(query("give back content no live writer can upload"))?;
    Ok(())
}
