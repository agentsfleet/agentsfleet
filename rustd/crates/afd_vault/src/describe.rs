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
//! One credential legitimately backs several model rows, so the names asked for
//! repeat. A positional slot per entry, matched back by index, would need
//! deduplicating in two quadratic passes; the GUARANTEE those passes buy —
//! every entry resolves its own credential, duplicates included — is a map's by
//! construction. So the map is what this returns, and the caller looks up per
//! row.

use std::collections::HashMap;

use afd_core::id::Uuid7;

use crate::classified::Classified;
use crate::error::{Result, query};
use crate::projection::Kind;
use crate::{Directory, sql};

/// The context a failed describe reports under.
const CONTEXT_DESCRIBE: &str = "describe named secrets";

/// What one credential is, for a caller that already knows its name.
///
/// The non-secret half of [`crate::projection::Projection`], as it comes back
/// OUT of the `meta_*` columns rather than on its way in. There is no field a
/// key would fit in, which is the same guarantee the columns themselves carry.
///
/// The classification is a sum type, not a struct of options: the fields a
/// kind carries are exactly the fields it can carry, and a degraded row sheds
/// its descriptors by construction. See [`Classified`]'s module note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// What the credential is, as the server classified it at write time.
    pub classified: Classified,
    /// Whether a non-empty key is stored. Never the key.
    pub has_key: bool,
}

impl Descriptor {
    /// The kind this row answers on the wire.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.classified.kind()
    }

    /// The provider label, for the kinds that carry one.
    #[must_use]
    pub fn provider(&self) -> Option<&str> {
        self.classified.provider()
    }

    /// The custom endpoint, where one may be displayed.
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.classified.base_url()
    }
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
        let rows = sqlx::query_as::<_, DescriptorRow>(sql::SELECT_SECRET_DESCRIPTORS)
            .bind(workspace.as_str())
            .bind(names)
            .fetch_all(connection.as_mut())
            .await
            .map_err(query(CONTEXT_DESCRIBE))?;

        Ok(rows.into_iter().map(DescriptorRow::into_entry).collect())
    }
}

/// One descriptor row, read by column name: a renamed or dropped column fails
/// the decode, and `test_every_sql_file_prepares` catches it before merge.
#[derive(Debug, sqlx::FromRow)]
struct DescriptorRow {
    key_name: String,
    meta_kind: Option<String>,
    meta_provider: Option<String>,
    meta_base_url: Option<String>,
    meta_has_key: Option<bool>,
}

impl DescriptorRow {
    /// The row as its map entry.
    fn into_entry(self) -> (Box<str>, Descriptor) {
        let descriptor = Descriptor {
            classified: Classified::classify(
                self.meta_kind.as_deref(),
                &self.key_name,
                self.meta_provider,
                self.meta_base_url,
            ),
            // The column is nullable. Absent is "no key we can prove", which is
            // what the page renders, and never a key this read failed to look for.
            has_key: self.meta_has_key.unwrap_or_default(),
        };
        (self.key_name.into_boxed_str(), descriptor)
    }
}
