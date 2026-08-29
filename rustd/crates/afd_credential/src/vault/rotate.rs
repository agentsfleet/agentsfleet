//! Writing a rotated refresh token back to the handle it came from.
//!
//! The one path in this crate that WRITES a stored credential, and it exists
//! because the refresh providers rotate: Zoho, Jira and Linear invalidate the
//! posted refresh token the moment they issue a successor. A handle left
//! holding the old value is a connection that mints exactly once more — never —
//! and reads to the tenant as `reconnect_required` when they did nothing wrong.
//!
//! # The guard is a lock, not a comparison
//!
//! `oauth_refresh.zig` reads the handle, compares its refresh token against the
//! one the exchange posted, and writes — with no lock across the three. An
//! administrator reconnecting the integration in that window has their new
//! handle silently overwritten with a refresh token belonging to the grant they
//! just replaced, which kills the connection they were repairing.
//!
//! Here the read takes `FOR UPDATE` inside the same transaction as the write,
//! so the compare and the write see one another. The comparison itself is kept
//! for what it is actually good for: detecting that the row changed BEFORE this
//! transaction started, which no lock can tell you.
//!
//! # A failed write-back is not a failed mint
//!
//! The exchange already succeeded and the caller holds a working access token.
//! So every outcome here is reported, none of them fails the request, and the
//! cost of the worst one is bounded: one forced reconnect when the access token
//! expires (RULE ECL).

use afd_core::clock::UnixMillis;
use afd_crypto::aad::Aad;
use afd_crypto::envelope::{Envelope, Sealer};
use serde_json::Value;
use sqlx::Acquire as _;

use crate::error::{Result, query, vault_data_invalid};
use crate::vault::sql;
use crate::vault::{ENVELOPE_AT, KeyRef, Vault};
use afd_core::credential::FIELD_REFRESH_TOKEN;

/// Statement name, for the context a query failure carries.
pub(crate) const CONTEXT_ROTATE: &str = "vault credential rotation";

/// What one write-back did.
///
/// Three outcomes rather than a `bool`, because an operator reading the log
/// needs to tell "nothing to do" from "somebody else got there first" — the
/// second is a reconnect racing a mint, and it is worth seeing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotated {
    /// The handle now holds the replacement.
    Persisted,
    /// The stored handle is no longer the one this exchange posted from, so the
    /// replacement belongs to a grant that has been superseded. Dropping it is
    /// correct: writing it would undo the reconnect that replaced it.
    SkippedStale,
}

/// The envelope columns one credential row is written from.
///
/// A binder rather than ten loose `.bind()` calls, which is the shape
/// [`sql::gate::PendingRow`] already established in this crate: at this arity
/// the field NAMES are what a reader checks, instead of a `$n` position they
/// have to count to.
struct EnvelopeRow<'a> {
    /// Which row. Scopes the write exactly as the read was scoped.
    key: KeyRef<'a>,
    /// The freshly sealed envelope.
    sealed: &'a Envelope,
    /// The instant the row is stamped with.
    now: UnixMillis,
}

impl<'a> EnvelopeRow<'a> {
    /// Binds this row to [`sql::UPDATE_SECRET_ENVELOPE`] in `$n` order.
    fn bind(
        self,
        statement: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
        statement
            .bind(self.key.workspace_id.as_str())
            .bind(self.key.name)
            .bind(self.sealed.wrapped_dek())
            .bind(self.sealed.dek_nonce().as_slice())
            .bind(self.sealed.dek_tag().as_slice())
            .bind(self.sealed.payload_nonce().as_slice())
            .bind(self.sealed.payload_ciphertext())
            .bind(self.sealed.payload_tag().as_slice())
            .bind(self.sealed.kek_version())
            .bind(self.now.as_millis())
    }
}

impl Vault {
    /// Replaces the handle's refresh token, if it is still the one that was
    /// posted.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, an envelope that will not
    /// open or re-seal, and a stored body that is not a JSON object. None of
    /// them fails the mint that called this — see the module note.
    pub async fn rotate_refresh_token(
        &self,
        key: KeyRef<'_>,
        posted: &str,
        replacement: &str,
        now: UnixMillis,
    ) -> Result<Rotated> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_ROTATE))?;

        let row = sqlx::query(sql::LOCK_SECRET)
            .bind(key.workspace_id.as_str())
            .bind(key.name)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_ROTATE))?;
        // The row was deleted while the exchange ran. There is nothing to write
        // the replacement into, and creating one would resurrect a credential a
        // human removed.
        let Some(row) = row else {
            return Ok(Rotated::SkippedStale);
        };

        let stored = self.decrypt(&row, ENVELOPE_AT, key)?;
        let mut handle: serde_json::Map<String, Value> =
            serde_json::from_slice(stored.expose()).map_err(|_shape| vault_data_invalid())?;
        // Compares against what this exchange actually posted, which is the one
        // thing the lock cannot tell us: a reconnect that landed BEFORE this
        // transaction opened leaves a handle whose refresh token was never the
        // one we redeemed.
        let current = handle.get(FIELD_REFRESH_TOKEN).and_then(Value::as_str);
        if current != Some(posted) {
            return Ok(Rotated::SkippedStale);
        }

        // Every other field is carried across untouched, which is what makes
        // this a rotation rather than a rewrite: `accounts_base`,
        // `connected_at_ms` and anything a connect callback stored stay exactly
        // as they were, so the broker's cache identity is unchanged and an
        // ordinary rotation remains a cache hit.
        //
        handle.insert(
            FIELD_REFRESH_TOKEN.to_owned(),
            Value::String(replacement.to_owned()),
        );
        let plaintext = serde_json::to_vec(&handle).map_err(|_shape| vault_data_invalid())?;
        let sealed = Sealer::new()
            .seal(
                self.kek(),
                &Aad::new(key.workspace_id.as_str(), key.name),
                &plaintext,
            )
            .map_err(crate::error::vault_open)?;

        let updated = EnvelopeRow {
            key,
            sealed: &sealed,
            now,
        }
        .bind(sqlx::query(sql::UPDATE_SECRET_ENVELOPE))
        .execute(&mut *transaction)
        .await
        .map_err(query(CONTEXT_ROTATE))?;
        if updated.rows_affected() == 0 {
            return Ok(Rotated::SkippedStale);
        }
        transaction.commit().await.map_err(query(CONTEXT_ROTATE))?;
        Ok(Rotated::Persisted)
    }
}
