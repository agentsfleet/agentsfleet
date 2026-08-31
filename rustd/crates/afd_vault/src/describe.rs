//! The descriptors for a NAMED SET of credentials, in one statement.
//!
//! [`crate::read`]'s list answers "everything this workspace holds". The tenant
//! model registry asks a different question — describe exactly these names, the
//! ones a page of entries happens to reference — and answering it by listing the
//! workspace would read every credential a tenant owns to render at most a
//! hundred rows.
//!
//! Same guarantee as the list, for the same structural reason: [`Directory`]
//! holds no key, and the statement projects no ciphertext column. A page of
//! model entries performs zero decrypts, and it does so because there is
//! nothing here to decrypt with and nothing to decrypt.
//!
//! # Why this one carries `has_key` and the list does not
//!
//! Key PRESENCE is the registry page's question: a row whose credential holds
//! no key renders differently from one whose credential does, and neither
//! rendering involves the key. The secrets list has no such column and never
//! asked for one — see [`crate::SecretSummary`], which says so. So the two
//! reads project different column sets on purpose, and `meta_has_key` appears
//! on exactly the one that displays it.
//!
//! # A map, not a positional slot per name
//!
//! `tenant_model_entries_view.zig` allocates one slot per entry and matches
//! rows back by index, because one credential legitimately backs several model
//! rows and deduplicating them would cost two quadratic passes. The index
//! arithmetic is a workaround for not having a map at hand; the GUARANTEE it
//! buys — every entry resolves its own credential, duplicates included — is a
//! map's by construction. So the map is what this returns, and the caller looks
//! up per row.

use std::collections::HashMap;

use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Result, query};
use crate::projection::Kind;
use crate::read::labelled;
use crate::{Directory, sql};

/// The context a failed describe reports under.
const CONTEXT_DESCRIBE: &str = "describe named secrets";

/// What one credential is, for a caller that already knows its name.
///
/// The non-secret half of [`crate::projection::Projection`], as it comes back
/// OUT of the `meta_*` columns rather than on its way in. There is no field a
/// key would fit in, which is the same guarantee the columns themselves carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// What the credential is, as the server classified it at write time.
    pub kind: Kind,
    /// The provider label, for the kinds that carry one.
    pub provider: Option<Box<str>>,
    /// The custom endpoint, where one may be displayed.
    pub base_url: Option<Box<str>>,
    /// Whether a non-empty key is stored. Never the key.
    pub has_key: bool,
}

impl Directory {
    /// Describes each of `names` this workspace holds, by name.
    ///
    /// A name with no row is simply ABSENT from the map. That is not an error
    /// and not a blank descriptor: an entry naming a credential deleted out of
    /// band still lists, degraded to an opaque secret with no key, and the
    /// caller decides that rather than this read guessing at it.
    ///
    /// Empty input answers an empty map without a round trip. A page with no
    /// rows has nothing to describe, and the statement count a registry read
    /// costs is pinned — a degenerate page must not spend one.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row whose columns are
    /// not the types this build reads. A row this build cannot LABEL is not an
    /// error — it degrades, exactly as it does on the list.
    pub async fn describe(
        &self,
        workspace: &Uuid7,
        names: &[&str],
    ) -> Result<HashMap<Box<str>, Descriptor>> {
        if names.is_empty() {
            return Ok(HashMap::new());
        }
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_SECRET_DESCRIPTORS)
            .bind(workspace.as_str())
            .bind(names)
            .fetch_all(connection.as_mut())
            .await
            .map_err(query(CONTEXT_DESCRIBE))?;

        rows.iter().map(read_descriptor).collect()
    }
}

/// Reads one descriptor row into its map entry.
///
/// Positional, matching every other read in this workspace: the statement's
/// projection and this function are one contract, and reading by name would
/// hide a projection that had drifted out of order.
fn read_descriptor(row: &PgRow) -> Result<(Box<str>, Descriptor)> {
    let unreadable = query(CONTEXT_DESCRIBE);
    let name: String = row.try_get(0).map_err(&unreadable)?;
    let stored_kind: Option<String> = row.try_get(1).map_err(&unreadable)?;
    let provider: Option<String> = row.try_get(2).map_err(&unreadable)?;
    let base_url: Option<String> = row.try_get(3).map_err(&unreadable)?;
    // NULL on a row written before the projection columns existed. Absent is
    // "no key we can prove", which is what the page renders, and never a key
    // this read failed to look for.
    let has_key: Option<bool> = row.try_get(4).map_err(&unreadable)?;

    let descriptor = match labelled(stored_kind.as_deref(), &name) {
        Some(kind) => Descriptor {
            kind,
            provider: provider.map(String::into_boxed_str),
            base_url: base_url.map(String::into_boxed_str),
            has_key: has_key.unwrap_or_default(),
        },
        None => Descriptor {
            kind: Kind::CustomSecret,
            provider: None,
            base_url: None,
            has_key: has_key.unwrap_or_default(),
        },
    };
    Ok((name.into_boxed_str(), descriptor))
}
