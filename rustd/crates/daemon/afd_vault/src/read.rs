//! The list, served from the projection columns and from nothing else.
//!
//! One statement, one round trip, no key unwrap and no AES-GCM open — the cost
//! of a page no longer tracks the number of credentials stored, and no
//! plaintext enters the process for a request that displays none.
//!
//! The four `meta_*` columns exist precisely so a read does not have to decrypt
//! every row to project its body, and spec Invariant 3 says this one does not.
//! [`Directory`] holds no key, so it could not decrypt even if the statement
//! gave it something to decrypt.
//!
//! # A row this build cannot describe still lists
//!
//! A row written by a newer daemon may carry a `meta_kind` this build has no
//! variant for. It lists as an opaque `custom_secret`, because a page of twenty
//! credentials must not fail over one it cannot label, and it is not healed by
//! decrypting here: that would put an envelope open back on this path.

use afd_core::id::Uuid7;

use crate::classified::Classified;
use crate::error::{Result, query};
use crate::projection::Kind;
use crate::{Directory, sql};

/// The context a failed list reports under.
const CONTEXT_LIST: &str = "list workspace secrets";

/// One credential as the list shows it — every field non-secret by construction.
///
/// There is no `data` field and no `has_key`. The stored body is never returned
/// on any route, and key presence is the tenant Models page's question, asked
/// through its own statement.
///
/// `model` is absent for a reason worth reading once: `vault.secrets` has no
/// column for it, so the only way to answer it here would be to decrypt every
/// row. It is optional in `SecretSummary` and in the dashboard's `Secret`
/// union, and no client reads it — see [`crate::projection`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretSummary {
    /// The name the secret is stored and interpolated under.
    pub name: String,
    /// When it was first stored.
    pub created_at: jiff::Timestamp,
    /// What it is, as the server classified it at write time.
    ///
    /// A sum type, not a kind beside two options: the dashboard narrows on a
    /// tagged union where a `custom_secret` has no provider field, and this is
    /// that union — a degraded row sheds its descriptors by construction. See
    /// [`Classified`]'s module note.
    pub classified: Classified,
}

impl SecretSummary {
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
    /// Every secret `workspace` holds, by name, oldest name first.
    ///
    /// Unpaginated, matching `SELECT_SECRETS_FOR_WORKSPACE` and the documented
    /// shape: the route takes no page parameters, and a workspace holds tens of
    /// credentials rather than thousands. Adding a cursor here would be adding
    /// an endpoint parameter in a milestone whose rule is parity.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row whose columns are
    /// not the types this build reads. A row this build cannot LABEL is not an
    /// error — see the module note.
    pub async fn list(&self, workspace: &Uuid7) -> Result<Vec<SecretSummary>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query_as::<_, ProjectionRow>(sql::SELECT_SECRET_PROJECTIONS)
            .bind(workspace.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map_err(query(CONTEXT_LIST))?;

        Ok(rows.into_iter().map(SecretSummary::from).collect())
    }
}

/// One projection row, read by column name: a renamed or dropped column fails
/// the decode, and `test_every_sql_file_prepares` catches it before merge.
#[derive(Debug, sqlx::FromRow)]
struct ProjectionRow {
    key_name: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: jiff::Timestamp,
    meta_kind: Option<String>,
    meta_provider: Option<String>,
    meta_base_url: Option<String>,
}

impl From<ProjectionRow> for SecretSummary {
    fn from(row: ProjectionRow) -> Self {
        let classified = Classified::classify(
            row.meta_kind.as_deref(),
            &row.key_name,
            row.meta_provider,
            row.meta_base_url,
        );
        Self {
            name: row.key_name,
            created_at: row.created_at,
            classified,
        }
    }
}
