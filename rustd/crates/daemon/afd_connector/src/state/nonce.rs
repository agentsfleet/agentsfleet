//! The half of a state that needs a store: remembering a nonce, and spending it.
//!
//! # Single-use is the delete's returned row, not a read followed by a write
//!
//! A read that finds the row and a delete that removes it are two statements
//! with a gap, and two callbacks racing through that gap would both complete,
//! which is the whole failure single-use exists to prevent. `DELETE …
//! RETURNING` answers the row to exactly one caller.
//!
//! # Only the digest is stored
//!
//! The nonce rides the signed state the browser carries. The table keeps its
//! SHA-256, so a reader of `connector.oauth_nonces` holds nothing a callback
//! could present.
//!
//! # Nothing here decides anything
//!
//! A nonce is remembered when a connect starts and spent when one finishes.
//! Whether the state was genuine, unexpired, and presented by the person who
//! started it is [`super::verify`]'s and the caller's, decided before this
//! module is reached.

use std::time::Duration;

use af_crypto::entropy::Entropy;
use afd_db::Db;
use jiff::Timestamp;
use sha2::{Digest as _, Sha256};

use crate::error::{Result, query};
use crate::provider::Provider;
use crate::registry::STATE_TTL_SECONDS;
use crate::sql;

/// Bytes of entropy behind a nonce, rendered as twice as many hex characters.
///
/// Sixteen is the width a value that must not be guessed inside a ten-minute
/// window needs, with room to spare.
const NONCE_BYTES: usize = 16;

const CONTEXT_REMEMBER: &str = "remember a connect nonce";
const CONTEXT_SPEND: &str = "spend a connect nonce";

/// A fresh nonce, as the lower-case hex a state carries.
///
/// # Errors
/// Reports a host short of the entropy a nonce is drawn from.
pub fn mint(entropy: &Entropy) -> Result<String> {
    let mut bytes = [0_u8; NONCE_BYTES];
    entropy.fill(&mut bytes)?;
    Ok(hex::encode(bytes))
}

/// The stored form of a nonce.
fn digest(nonce: &str) -> Vec<u8> {
    Sha256::digest(nonce.as_bytes()).to_vec()
}

/// Remembers `nonce` for as long as a connect round-trip may take.
///
/// # Errors
/// Reports a store that would not answer. A connect whose nonce was not
/// remembered must not proceed: its state would verify and then fail to
/// consume, which reads to the person as a forged callback.
pub async fn remember(
    database: &Db,
    entropy: &Entropy,
    provider: Provider,
    nonce: &str,
    now: Timestamp,
) -> Result<()> {
    let mut connection = database.acquire().await?;
    sqlx::query(sql::INSERT_OAUTH_NONCE)
        .bind(entropy.uuid7(now)?)
        .bind(provider.id())
        .bind(digest(nonce))
        .bind(Duration::from_secs(u64::from(STATE_TTL_SECONDS)))
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_REMEMBER))?;
    Ok(())
}

/// Spends `nonce`, answering whether this caller is the one that got it.
///
/// `false` for a row already spent and for one that expired — the two are
/// indistinguishable here and are answered identically by the caller, because
/// both mean the same thing to the person: start the connect again.
///
/// # Errors
/// Reports a store that would not answer. Deliberately NOT collapsed into
/// `false`: a store that is down would otherwise read as a replayed callback,
/// and an operator would go looking for an attacker.
pub async fn consume(database: &Db, provider: Provider, nonce: &str) -> Result<bool> {
    let mut connection = database.acquire().await?;
    let spent = sqlx::query(sql::SPEND_OAUTH_NONCE)
        .bind(provider.id())
        .bind(digest(nonce))
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT_SPEND))?;
    Ok(spent.is_some())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{NONCE_BYTES, digest};

    /// The stored form is the nonce's SHA-256, never the nonce.
    #[test]
    fn the_stored_form_is_a_digest_not_the_nonce() {
        let nonce = "0123456789abcdef0123456789abcdef";

        let stored = digest(nonce);

        assert_eq!(stored.len(), 32);
        assert_ne!(stored.as_slice(), nonce.as_bytes());
        assert_eq!(stored, digest(nonce), "one nonce, one digest");
        assert_ne!(stored, digest("0123456789abcdef0123456789abcdee"));
    }

    /// The rendered nonce is twice the byte width, which is what hex means.
    #[test]
    fn the_rendered_nonce_is_two_hex_characters_per_byte() {
        let entropy = af_crypto::entropy::Entropy::new();

        let nonce = super::mint(&entropy).expect("the host has entropy");

        assert_eq!(nonce.len(), NONCE_BYTES * 2);
        assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    /// Each byte renders as two lower-case digits, leading zero kept, so the
    /// digest a state names is the one the mint remembered.
    #[test]
    fn the_rendered_nonce_is_lower_case_with_every_leading_zero() {
        let (entropy, control) = af_crypto::entropy::Entropy::new_mocked();
        control.push_bytes(&[0x0a; NONCE_BYTES]);

        let nonce = super::mint(&entropy).expect("the mock answers");

        // pin test: literal is the contract
        assert_eq!(nonce, "0a".repeat(NONCE_BYTES));
    }
}
