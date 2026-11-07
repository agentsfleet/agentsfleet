//! Tenant api-keys: the `agt_t` credentials a tenant manages for itself.
//!
//! # Revealed once, stored as a digest
//!
//! The plaintext exists for the length of one response and is then zeroed —
//! [`Minted`] owns that, including the `Drop` that overwrites the heap a
//! `Box<str>` would otherwise merely free. Nothing here can read a key back,
//! because no statement in [`crate::sql::apikey`] selects the digest column and
//! there is no method that would return it if one did.
//!
//! # The lifecycle is two steps on purpose
//!
//! Revoke, then delete. Revocation is the reversible half — the row survives
//! and can explain itself to whoever is reading an audit trail a week later —
//! and a live credential vanishing in one call leaves nothing behind. The
//! statement enforces it rather than this code: `DELETE_TENANT_KEY` will not
//! delete an active row, so the ordering holds under a race between two
//! operators as well as under a single caller who read the documentation.

mod name;
mod sort;
mod view;
pub use self::view::{KeyRow, Listing};

use afd_auth::credential::CredentialKind;
use afd_core::id::Uuid7;
use afd_core::paging::{BoundaryKind, Cursor, Page, SortOrder as _};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::constraint::violates_unique;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::sql::apikey as sql;
use crate::{Result, error};
use afd_auth::minted::Minted;

pub use self::name::{Deactivation, Description, KeyName};
pub use self::sort::ApiKeySort;

/// The index that holds a tenant to one key per name.
///
/// Must equal the name in `schema/1150_identity_api_keys.sql`: a key-hash or
/// primary-key collision is unique too, and is not a name the caller can change.
const NAME_CONSTRAINT: &str = "uq_api_keys_tenant_id_name";

/// A tenant's api-keys.
#[derive(Debug, Clone)]
pub struct ApiKeys {
    database: Db,
    entropy: Entropy,
}

impl ApiKeys {
    /// A store reading and writing through `database`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// Mints one key, answering the only view of it that will ever exist.
    ///
    /// # Errors
    /// Refuses a name this tenant already uses; reports a host that cannot draw
    /// entropy and a datastore that would not answer.
    pub async fn mint(&self, request: &MintRequest<'_>, now: Timestamp) -> Result<Revealed> {
        let credential = Minted::draw(CredentialKind::TenantApiKey, &self.entropy)?;
        let id = self.entropy.uuid7(now)?;

        let mut connection = self.database.acquire().await?;
        let created_at = sqlx::query_scalar::<_, jiff_sqlx::Timestamp>(sql::INSERT_TENANT_KEY)
            .bind(id.as_str())
            .bind(request.tenant.as_str())
            .bind(request.name.as_str())
            .bind(request.description.as_str())
            .bind(credential.digest().as_str())
            .bind(request.created_by)
            .fetch_one(connection.as_mut())
            .await
            .map_err(classify_insert)?
            .to_jiff();

        // Hoisted: the `log` bridge duplicates every field expression and
        // llvm-cov scores the dead copy. The key itself is NOT among them —
        // `Minted`'s `Debug` renders a length and the word redacted.
        let tenant = request.tenant.as_str();
        let actor = request.created_by;
        let key_id = id.as_str();
        let key_name = request.name.as_str();
        tracing::debug!(tenant, actor, key_id, key_name, event = "apikey_minted");

        Ok(Revealed {
            id,
            name: request.name.as_str().to_owned(),
            credential,
            created_at,
        })
    }

    /// Revokes one key, reporting only when THIS call did it.
    ///
    /// `_intent` is read for its EXISTENCE rather than its value: a
    /// [`Deactivation`] can only be built by refusing `active: true`, so this
    /// signature is what makes re-activation unreachable rather than merely
    /// unimplemented.
    ///
    /// # Errors
    /// Refuses an id naming no key this tenant holds, and one already revoked.
    /// Reports a datastore that would not answer.
    pub async fn revoke(
        &self,
        tenant: &Uuid7,
        key: &Uuid7,
        _intent: Deactivation,
    ) -> Result<Revoked> {
        let mut connection = self.database.acquire().await?;
        let row: RevokedRow = sqlx::query_as(sql::REVOKE_TENANT_KEY)
            .bind(key.as_str())
            .bind(tenant.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("revoke api-key"))?
            .ok_or_else(error::apikey_not_found)?;

        if !row.changed {
            return Err(error::apikey_already_revoked());
        }
        Ok(Revoked {
            id: key.clone(),
            revoked_at: row.revoked_at,
        })
    }

    /// Deletes one already-revoked key.
    ///
    /// # Errors
    /// Refuses an id naming no key this tenant holds, and a key that is still
    /// active. Reports a datastore that would not answer.
    pub async fn delete(&self, tenant: &Uuid7, key: &Uuid7) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let changed: bool = sqlx::query_scalar(sql::DELETE_TENANT_KEY)
            .bind(key.as_str())
            .bind(tenant.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("delete api-key"))?
            .ok_or_else(error::apikey_not_found)?;

        if changed {
            Ok(())
        } else {
            Err(error::apikey_must_revoke_first())
        }
    }

    /// One page of a tenant's keys, and the tenant's whole key count.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn list(&self, tenant: &Uuid7, page: &Page<ApiKeySort>) -> Result<Listing> {
        // `AssertSqlSafe` is sqlx refusing to take a `String` without somebody
        // saying they audited it. The audit is [`Self::page_statement`]'s
        // doc comment and the type system behind it: both interpolated values
        // are `&'static str` returned by methods on `Copy` enums whose only
        // constructor parses against a closed allowlist, so no caller-supplied
        // byte can reach either slot.
        let statement = sqlx::AssertSqlSafe(Self::page_statement(page));
        let mut query = sqlx::query(statement).bind(tenant.as_str());
        query = match page.cursor.as_ref() {
            None => query,
            Some(Cursor::Timestamp { at, id }) => query.bind(at.to_sqlx()).bind(id.as_str()),
            Some(Cursor::Text { value, id }) => query.bind(value.as_str()).bind(id.as_str()),
        };

        let mut connection = self.database.acquire().await?;
        let rows = query
            .bind(i64::from(page.limit))
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query("list api-keys"))?;

        Listing::of(&rows)
    }

    /// The statement one page needs, with its two literal slots filled.
    ///
    /// The slots take [`SortOrder::order_by`] and [`Comparator::as_sql`], both
    /// of which are methods on `Copy` enums whose only constructor is a parse
    /// against a closed allowlist. There is no expression in this workspace
    /// that puts a caller's bytes in either — see [`afd_core::paging`].
    ///
    /// [`Comparator::as_sql`]: afd_core::paging::Comparator::as_sql
    fn page_statement(page: &Page<ApiKeySort>) -> String {
        let template = match page.cursor.as_ref().map(Cursor::kind) {
            None => sql::SELECT_TENANT_KEY_PAGE_FIRST,
            Some(BoundaryKind::Timestamp) => sql::SELECT_TENANT_KEY_PAGE_AFTER_CREATED,
            Some(BoundaryKind::Text) => sql::SELECT_TENANT_KEY_PAGE_AFTER_NAME,
        };
        template
            .replace(sql::SLOT_ORDER, page.sort.order_by())
            .replace(sql::SLOT_COMPARATOR, page.sort.comparator().as_sql())
    }
}

/// What minting one key needs.
#[derive(Debug, Clone, Copy)]
pub struct MintRequest<'a> {
    /// Whose key it is.
    pub tenant: &'a Uuid7,
    /// What it is called, already parsed.
    pub name: KeyName<'a>,
    /// The free text beside it, already bounded.
    pub description: Description<'a>,
    /// The identity provider's subject for whoever minted it.
    ///
    /// A `&str` rather than a parsed subject, because `created_by_subject` is
    /// `TEXT NOT NULL` holding the claim directly — a key outlives the admin who
    /// made it, as `schema/1150_identity_api_keys.sql` says, and it is why the
    /// credential lookup does not join `identity.users` for this class.
    pub created_by: &'a str,
}

/// A key, and the one view of its plaintext that will ever exist.
///
/// No `Clone`: a second copy of a credential is a second thing to zero, and the
/// one that gets missed is the one that stays in the heap.
#[derive(Debug)]
pub struct Revealed {
    /// The key's identifier.
    pub id: Uuid7,
    /// What it is called.
    pub name: String,
    /// The plaintext, which zeroes when this is dropped.
    pub credential: Minted,
    /// When it was minted.
    pub created_at: Timestamp,
}

/// A key that this call revoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revoked {
    /// The key's identifier.
    pub id: Uuid7,
    /// When the row records it stopped working.
    pub revoked_at: Timestamp,
}

/// Turns an insert failure into the refusal it means.
///
/// The name collision is arbitrated by [`NAME_CONSTRAINT`] rather
/// than by a read before the write: a pre-flight `SELECT` leaves a window in
/// which two concurrent mints both pass it, and one of them then loses at the
/// insert anyway, so the window buys nothing and hides the real arbiter.
///
/// The unique index is the arbiter, so its violation is the ONE failure here
/// that is the caller's rather than the datastore's — everything else is
/// reported as the statement failure it is, with the `sqlx::Error` riding
/// through as the source.
fn classify_insert(source: sqlx::Error) -> crate::Error {
    if violates_unique(&source, NAME_CONSTRAINT) {
        error::apikey_name_taken()
    } else {
        error::query("mint api-key")(source)
    }
}

/// Reports a row whose columns are not the shape this daemon reads.
/// What a revoke answers: whether THIS call changed the row, and the instant
/// the row records. Both arms of the statement answer a stamped row — the one
/// this call's `UPDATE` wrote, or the earlier revoke's — so it is never NULL.
#[derive(Debug, sqlx::FromRow)]
struct RevokedRow {
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    revoked_at: Timestamp,
    changed: bool,
}

/// A page row that would not decode, with the listing's context.
fn row_unreadable(source: sqlx::Error) -> crate::Error {
    error::query("read api-key row")(source)
}

#[cfg(test)]
mod tests;
