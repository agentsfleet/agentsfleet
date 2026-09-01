//! The list, served from the projection columns and from nothing else.
//!
//! One statement, one round trip, no key unwrap and no AES-GCM open — the cost
//! of a page no longer tracks the number of credentials stored, and no
//! plaintext enters the process for a request that displays none.
//!
//! `secret_list.zig` reaches the same page by decrypting every row and
//! projecting the body per row. That was the design before the four `meta_*`
//! columns existed; they were promoted precisely so a read would not have to,
//! and spec Invariant 3 says this one does not. [`Directory`] holds no key, so
//! it could not decrypt even if the statement gave it something to decrypt.
//!
//! # A row this build cannot describe still lists
//!
//! A row written before the projection columns existed carries NULL metadata,
//! and a row written by a newer daemon may carry a `meta_kind` this build has
//! no variant for. Both list as an opaque `custom_secret`, because a page of
//! twenty credentials must not fail over one it cannot label.
//!
//! Neither is HEALED by decrypting here. A heal-on-read path would put an
//! envelope open back on this path and make "reads never decrypt" true only
//! after warm-up, which is not a guarantee. `agentsfleetd backfill` is what
//! fills those rows.

use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

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
    /// When it was first stored, in epoch milliseconds.
    pub created_at_ms: i64,
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
        let rows = sqlx::query(sql::SELECT_SECRET_PROJECTIONS)
            .bind(workspace.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map_err(query(CONTEXT_LIST))?;

        rows.iter().map(read_row).collect()
    }
}

/// Reads one projection row.
///
/// Positional, matching every other read in this workspace: the statement's
/// projection and this function are one contract, and reading by name would
/// hide a projection that had drifted out of order.
fn read_row(row: &PgRow) -> Result<SecretSummary> {
    let unreadable = query(CONTEXT_LIST);
    let name: String = row.try_get(0).map_err(&unreadable)?;
    let created_at_ms: i64 = row.try_get(1).map_err(&unreadable)?;
    let stored_kind: Option<String> = row.try_get(2).map_err(&unreadable)?;
    let provider: Option<String> = row.try_get(3).map_err(&unreadable)?;
    let base_url: Option<String> = row.try_get(4).map_err(&unreadable)?;

    let classified = Classified::classify(stored_kind.as_deref(), &name, provider, base_url);
    Ok(SecretSummary {
        name,
        created_at_ms,
        classified,
    })
}
