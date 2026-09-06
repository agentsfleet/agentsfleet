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
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::paging::{BoundaryKind, Cursor, Page, SortOrder as _};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::Row as _;

use crate::sql::apikey as sql;
use crate::{Result, error};
use afd_auth::minted::Minted;

pub use self::name::{Deactivation, Description, KeyName};
pub use self::sort::ApiKeySort;

/// The column both lifecycle statements report their verdict in.
///
/// Named once rather than spelled at each `try_get`: the two statements answer
/// the same question — did THIS call change the row — and a typo in one of the
/// two spellings would read as a datastore fault rather than as a mistake
/// (RULE UFS).
const COLUMN_CHANGED: &str = "changed";

/// The column carrying the instant a revoke recorded.
const COLUMN_REVOKED_AT: &str = "revoked_at";

/// The Postgres error class for a violated unique index.
///
/// The name collision is arbitrated by `api_keys_name_per_tenant_uniq` rather
/// than by a read before the write: a pre-flight `SELECT` leaves a window in
/// which two concurrent mints both pass it, and one of them then loses at the
/// insert anyway — so the window buys nothing and hides the real arbiter.
const UNIQUE_VIOLATION: &str = "23505";

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
    pub async fn mint(&self, request: &MintRequest<'_>, now: UnixMillis) -> Result<Revealed> {
        let credential = Minted::draw(CredentialKind::TenantApiKey, &self.entropy)?;
        let id = self.mint_id(now)?;

        let mut connection = self.database.acquire().await?;
        let written = sqlx::query(sql::INSERT_TENANT_KEY)
            .bind(id.as_str())
            .bind(request.tenant.as_str())
            .bind(request.name.as_str())
            .bind(request.description.as_str())
            .bind(credential.digest().as_str())
            .bind(request.created_by)
            .bind(now.as_millis())
            .execute(connection.as_mut())
            .await;

        if let Err(source) = written {
            return Err(classify_insert(source));
        }

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
            created_at_ms: now.as_millis(),
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
        now: UnixMillis,
    ) -> Result<Revoked> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::REVOKE_TENANT_KEY)
            .bind(key.as_str())
            .bind(tenant.as_str())
            .bind(now.as_millis())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("revoke api-key"))?
            .ok_or_else(error::apikey_not_found)?;

        let changed: bool = row.try_get(COLUMN_CHANGED).map_err(row_unreadable)?;
        if !changed {
            return Err(error::apikey_already_revoked());
        }
        let revoked_at_ms: Option<i64> = row.try_get(COLUMN_REVOKED_AT).map_err(row_unreadable)?;
        Ok(Revoked {
            id: key.clone(),
            // The statement stamps the instant it wrote, which is the one that
            // matters; `now` is what it was TOLD to write and could differ if a
            // retry ever landed on a different frame.
            revoked_at_ms: revoked_at_ms.unwrap_or_else(|| now.as_millis()),
        })
    }

    /// Deletes one already-revoked key.
    ///
    /// # Errors
    /// Refuses an id naming no key this tenant holds, and a key that is still
    /// active. Reports a datastore that would not answer.
    pub async fn delete(&self, tenant: &Uuid7, key: &Uuid7) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::DELETE_TENANT_KEY)
            .bind(key.as_str())
            .bind(tenant.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("delete api-key"))?
            .ok_or_else(error::apikey_not_found)?;

        let changed: bool = row.try_get(COLUMN_CHANGED).map_err(row_unreadable)?;
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
            Some(Cursor::Timestamp { at_ms, id }) => query.bind(*at_ms).bind(id.as_str()),
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

    /// Draws a fresh key identifier.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        Ok(Uuid7::encode(now, self.entropy.uuid_randomness()?)?)
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
    /// A `&str` rather than a parsed subject, because the column is `TEXT NOT
    /// NULL` holding the claim directly — `schema/240_api_keys.sql` says so
    /// above the column, and it is why the credential lookup does not join
    /// `core.users` for this class.
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
    pub created_at_ms: i64,
}

/// A key that this call revoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revoked {
    /// The key's identifier.
    pub id: Uuid7,
    /// When the row records it stopped working.
    pub revoked_at_ms: i64,
}

/// Turns an insert failure into the refusal it means.
///
/// The unique index is the arbiter, so its violation is the ONE failure here
/// that is the caller's rather than the datastore's — everything else is
/// reported as the statement failure it is, with the `sqlx::Error` riding
/// through as the source.
fn classify_insert(source: sqlx::Error) -> crate::Error {
    let collided = source
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == UNIQUE_VIOLATION);
    if collided {
        error::apikey_name_taken()
    } else {
        error::query("mint api-key")(source)
    }
}

/// Reports a row whose columns are not the shape this daemon reads.
fn row_unreadable(source: sqlx::Error) -> crate::Error {
    error::query("read api-key row")(source)
}

#[cfg(test)]
mod tests;
