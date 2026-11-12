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
//! Reading the handle, comparing its refresh token against the one the exchange
//! posted, and writing — with no lock across the three — leaves a window. An
//! administrator reconnecting the integration in it would have their new
//! handle silently overwritten with a refresh token belonging to the grant they
//! just replaced, which kills the connection they were repairing.
//!
//! [`afd_vault::Vault::rewrite`] takes `FOR UPDATE` inside the same
//! transaction as the write, so the compare in the edit below and the write
//! see one another. The comparison itself is kept
//! for what it is actually good for: detecting that the row changed BEFORE this
//! transaction started, which no lock can tell you.
//!
//! # A failed write-back is not a failed mint
//!
//! The exchange already succeeded and the caller holds a working access token.
//! So every outcome here is reported, none of them fails the request, and the
//! cost of the worst one is bounded: one forced reconnect when the access token
//! expires (RULE ECL).

use af_core::credential::FIELD_REFRESH_TOKEN;
use af_core::id::Uuid7;
use af_crypto::secret::SecretObject;
use afd_vault::{Rewritten, SecretBody, SecretName, Vault};
use serde_json::Value;

use crate::error::{Result, vault_data_invalid};

/// What one write-back did.
///
/// Two outcomes rather than a `bool`, because an operator reading the log
/// needs to tell a write from "somebody else got there first" — the second is
/// a reconnect racing a mint, and it is worth seeing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotated {
    /// The handle now holds the replacement.
    Persisted,
    /// The stored handle is no longer the one this exchange posted from, or is
    /// gone, so the replacement belongs to a grant that has been superseded.
    /// Dropping it is correct: writing it would undo the reconnect that
    /// replaced it.
    SkippedStale,
}

/// Replaces the handle's refresh token, if it is still the one that was
/// posted.
///
/// Every other field is carried across untouched, which is what makes this a
/// rotation rather than a rewrite: `accounts_base`, `connected_at_ms` and
/// anything a connect callback stored stay exactly as they were, so the
/// broker's cache identity is unchanged.
///
/// # Errors
/// Reports a datastore that would not answer, an envelope that will not open or
/// re-seal, and a stored body that is not a JSON object. None of them fails the
/// mint that called this — see the module note.
pub async fn rotate_refresh_token(
    vault: &Vault,
    workspace: &Uuid7,
    name: &str,
    posted: &str,
    replacement: &str,
) -> Result<Rotated> {
    // A name the vault would refuse holds no row to write into.
    let Ok(name) = SecretName::parse(name) else {
        return Ok(Rotated::SkippedStale);
    };
    let rewritten = vault
        .rewrite(workspace, &name, |stored| {
            let mut handle =
                SecretObject::parse(stored.expose()).map_err(|_shape| vault_data_invalid())?;
            // Compares against what this exchange actually posted, which is the
            // one thing the lock cannot tell us: a reconnect that landed BEFORE
            // the transaction opened leaves a handle whose refresh token was
            // never the one we redeemed.
            let current = handle
                .fields()
                .get(FIELD_REFRESH_TOKEN)
                .and_then(Value::as_str);
            if current != Some(posted) {
                return Ok(None);
            }
            handle.replace_string(FIELD_REFRESH_TOKEN, replacement);
            SecretBody::from_object(&handle)
                .map(Some)
                .map_err(|_shape| vault_data_invalid())
        })
        .await??;
    Ok(match rewritten {
        Rewritten::Replaced => Rotated::Persisted,
        Rewritten::Kept | Rewritten::Absent => Rotated::SkippedStale,
    })
}
