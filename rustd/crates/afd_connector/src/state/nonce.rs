//! The half of a state that needs a store: remembering a nonce, and spending it.
//!
//! # Single-use is `DEL`'s return value, not a read followed by a write
//!
//! A `GET` that finds the slot and a `DEL` that removes it are two commands
//! with a gap, and two callbacks racing through that gap both see the slot and
//! both complete — which is the whole failure single-use exists to prevent.
//! `DEL` answers how many keys it removed, so exactly one caller can ever see
//! `1`. `state.zig` reaches the same conclusion and says so in the same words.
//!
//! # Nothing here decides anything
//!
//! A nonce is remembered when a connect starts and spent when one finishes.
//! Whether the state was genuine, unexpired, and presented by the person who
//! started it is [`super::verify`]'s and the caller's, decided before this
//! module is reached.

use afd_crypto::entropy::Entropy;
use afd_redis::Redis;

use crate::error::Result;
use crate::registry::{STATE_TTL_SECONDS, StateBinding};

/// Bytes of entropy behind a nonce, rendered as twice as many hex characters.
///
/// `state.zig`'s `NONCE_BYTES`. Sixteen is the width a value that must not be
/// guessed inside a ten-minute window needs, with room to spare.
const NONCE_BYTES: usize = 16;

/// What is stored at the slot, which is never read — only its presence counts.
const SLOT_MARK: &str = "1";

/// A fresh nonce, as the lower-case hex a state carries.
///
/// # Errors
/// Reports a host short of the entropy a nonce is drawn from.
pub fn mint(entropy: &Entropy) -> Result<String> {
    let mut bytes = [0_u8; NONCE_BYTES];
    entropy.fill(&mut bytes)?;
    let mut rendered = String::with_capacity(NONCE_BYTES * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        // A two-digit lower-case hex write into a `String` cannot fail.
        let _ = write!(rendered, "{byte:02x}");
    }
    Ok(rendered)
}

/// Remembers `nonce` for as long as a connect round-trip may take.
///
/// # Errors
/// Reports a store that would not answer. A connect whose nonce was not
/// remembered must not proceed: its state would verify and then fail to
/// consume, which reads to the person as a forged callback.
pub async fn remember(queue: &Redis, binding: StateBinding, nonce: &str) -> Result<()> {
    Ok(queue
        .set_for(
            &key(binding, nonce),
            SLOT_MARK,
            i64::from(STATE_TTL_SECONDS),
        )
        .await?)
}

/// Spends `nonce`, answering whether this caller is the one that got it.
///
/// `false` for a slot already spent and for one that expired — the two are
/// indistinguishable here and are answered identically by the caller, because
/// both mean the same thing to the person: start the connect again.
///
/// # Errors
/// Reports a store that would not answer. Deliberately NOT collapsed into
/// `false`: a store that is down would otherwise read as a replayed callback,
/// and an operator would go looking for an attacker.
pub async fn consume(queue: &Redis, binding: StateBinding, nonce: &str) -> Result<bool> {
    Ok(queue.spend_key(&key(binding, nonce)).await?)
}

/// Where one connect's slot lives — one spelling for the write and the spend.
fn key(binding: StateBinding, nonce: &str) -> String {
    format!("{}{nonce}", binding.nonce_prefix)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{NONCE_BYTES, key};
    use crate::provider::Provider;

    /// The slot key is the provider's namespace and the nonce, in that order.
    ///
    /// Pinned because the two sites that build it — remembering and spending —
    /// would fail apart silently: a connect would start, and every completion
    /// would answer "already used" for a slot nothing had ever spent.
    #[test]
    fn the_slot_key_is_the_providers_namespace_and_the_nonce() {
        let binding = Provider::Slack.state_binding();

        assert_eq!(key(binding, "abc"), "connect:slack:nonce:abc");
    }

    /// Two providers' slots for one nonce are different keys.
    ///
    /// The namespace half of the cross-connector guarantee `crate::registry`'s
    /// suite proves for the domain half: a nonce spent under one connector must
    /// leave the other's slot untouched.
    #[test]
    fn one_nonce_under_two_providers_is_two_slots() {
        let nonce = "0123456789abcdef0123456789abcdef";

        assert_ne!(
            key(Provider::Slack.state_binding(), nonce),
            key(Provider::Jira.state_binding(), nonce),
        );
    }

    /// The rendered nonce is twice the byte width, which is what hex means.
    #[test]
    fn the_rendered_nonce_is_two_hex_characters_per_byte() {
        let entropy = afd_crypto::entropy::Entropy::new();

        let nonce = super::mint(&entropy).expect("the host has entropy");

        assert_eq!(nonce.len(), NONCE_BYTES * 2);
        assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}
