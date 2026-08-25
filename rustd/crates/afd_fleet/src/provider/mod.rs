//! Whose key this run dials with, resolved fresh for every lease.
//!
//! # Two strategies, one interface
//!
//! A tenant's provider comes from one of two places, and the Zig writes each as
//! its own function: `resolvePlatformDefault` reads the active platform row and
//! loads a key out of the ADMIN workspace, `resolveSelfManaged` reads the
//! tenant's selection and loads a key out of the TENANT's workspace. Both then
//! do the same three things — open a vault row, read its JSON, own the result —
//! and both write that part out again, with their own `errdefer` ladder and
//! their own `secureZero`.
//!
//! Here the difference is a [`Resolution`]: a value that knows WHICH row
//! carries its key and what that row's body means. The part they share — open
//! the row — is written once, in [`vault`], and runs between the two halves of
//! the trait:
//!
//! ```text
//!   strategy = platform | self-managed        ← the fork, decided once
//!   body     = open(strategy.key())           ← shared, one statement
//!   resolved = strategy.interpret(body)       ← the fork again, pure
//! ```
//!
//! # Why the trait is object-safe, synchronous and pure
//!
//! Because it can be. Neither half of a strategy touches a datastore: `key`
//! reports what it already holds, and `interpret` is a function from bytes to a
//! value. Putting the I/O in the trait instead — one `async fn resolve(&self,
//! …)` per strategy — would need boxed futures to stay object-safe and would
//! give up the property that matters more than either: EVERY branching decision
//! in this module is provable with no Postgres, no vault and no key. The two
//! strategy files are seventeen unit tests between them and not one of them
//! opens a connection.
//!
//! That is the same split [`crate::money`] draws between its pure arithmetic
//! and its reads, and the same one [`crate::lease::admit::posture`] draws
//! between a verdict and a fault.
//!
//! # Where this crate keeps it, and where it may have to move
//!
//! The Zig keeps provider resolution in `state/`, which is `afd_state` here.
//! It is in `afd_fleet` instead because its only caller this milestone is the
//! lease verb, and because the rich refusal classification it produces
//! ([`crate::Error::is_config_permanent`]) is this crate's error type. M178's
//! tenant plane resolves the same credentials for `PUT /v1/tenants/me/provider`
//! and must NOT reach them by depending on the runner-plane crate — when that
//! lands, this module moves down to `afd_state` and both callers import it.
//! Recorded here rather than pre-built: a seam with one caller is a guess.

mod endpoint;
mod managed;
mod platform;
mod resolved;
mod selection;
mod ssrf;
mod store;
mod vault;

use std::fmt::Debug;

use afd_core::id::Uuid7;
use serde::de::DeserializeOwned;

use crate::error::{Result, provider_malformed, provider_no_workspace, provider_secret_missing};
use crate::money::Posture;

pub use self::endpoint::{OPENAI_COMPATIBLE, Rejection};
pub use self::resolved::{Resolved, SecretString};
pub use self::selection::{PlatformDefault, Selection};
pub use self::store::Providers;
pub use self::vault::KeyRef;

/// A prepared way of resolving one tenant's provider.
///
/// Boxed rather than an enum, and that is the one place this design spends
/// something. An enum would dispatch statically and cost no allocation; what it
/// would also do is put both strategies' data in one type, so every match arm
/// in this file would carry the other strategy's fields. The trait keeps each
/// strategy's inputs inside the strategy — [`platform::Platform`] holds a
/// platform row and nothing else — and the allocation is one `Box` per lease,
/// beside four Postgres round trips and an AES-GCM open.
pub type Strategy = Box<dyn Resolution>;

/// What a strategy knows: where its key is, and what its body means.
///
/// `Debug` because the workspace denies `missing_debug_implementations` and a
/// strategy ends up inside values that derive it; `Send + Sync` because a
/// [`Strategy`] is held across an `await` on the vault read between the two
/// method calls.
pub trait Resolution: Debug + Send + Sync {
    /// The vault row carrying this strategy's key.
    fn key(&self) -> KeyRef<'_>;

    /// What that row's body resolves to.
    ///
    /// Pure: the same bytes always answer the same way, which is what lets
    /// every credential shape in this module be proven with no datastore.
    ///
    /// # Errors
    /// Reports a body that is not a credential this strategy can read — a
    /// missing or blank required field, a field of the wrong type, or an
    /// endpoint the guard refused.
    fn interpret(&self, body: &[u8]) -> Result<Resolved>;
}

/// One credential body, typed — refusing anything that is not a JSON OBJECT.
///
/// The gate is not decoration. `serde_json` deserializes a struct from a JSON
/// ARRAY by taking its elements POSITIONALLY, so `["anthropic","sk-live"]`
/// parses as a credential with a provider and a key, and every shape check
/// after it passes. `loadJson` refuses that at the top — `parsed.value !=
/// .object` — and dropping the check on the way to Rust would have made a
/// positional array a valid provider credential in a daemon where it is not
/// one anywhere else.
///
/// Structural rather than semantic: the first non-space byte of any JSON object
/// is `{`, so this costs one scan of the leading whitespace and no second
/// parse. Deserializing to a [`serde_json::Value`] to ask `is_object` would
/// answer the same question by building the whole document a second time — and
/// would hold the key in an intermediate that has no destructor, which is the
/// copy [`SecretString`] exists to prevent.
///
/// # Errors
/// Reports a body that is not an object, and one the strategy's own shape
/// cannot read. Both answer `field`, because a caller is told neither.
fn credential<T: DeserializeOwned>(body: &[u8], field: &'static str) -> Result<T> {
    let opens_an_object = body
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'{');
    if !opens_an_object {
        return Err(provider_malformed(field));
    }
    serde_json::from_slice(body).map_err(|_shape| provider_malformed(field))
}

impl Providers {
    /// The provider `tenant_id`'s next run dials with.
    ///
    /// Resolved fresh, with no cache. `model_rate_cache.zig`'s reasoning does
    /// not transfer here and neither does its machinery: a platform default is
    /// meant to change under a running fleet, and a cache is precisely what
    /// would stop the next lease from seeing it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and every way a stored
    /// configuration can fail to name a usable provider. The caller separates
    /// the two with [`crate::Error::is_config_permanent`] rather than by
    /// matching kinds.
    pub async fn resolve(&self, tenant_id: &Uuid7) -> Result<Resolved> {
        let strategy = self.strategy(tenant_id).await?;
        // Split across two statements, not folded into one expression: `key`
        // borrows from `strategy`, so the borrow has to outlive the `await`
        // that uses it and end before `interpret` is called on the same value.
        let body = self.open_secret(strategy.key()).await?;
        strategy.interpret(body.ok_or_else(provider_secret_missing)?.expose())
    }

    /// Which of the two ways `tenant_id` resolves.
    ///
    /// A tenant with NO selection row and a tenant with an explicit `platform`
    /// row take the same arm, and that collapse is the Zig's: `if (row == null
    /// or row.?.mode == .platform)`. The explicit row exists so the dashboard
    /// can tell "never configured" from "explicitly reset", which is a display
    /// distinction and not a resolution one.
    async fn strategy(&self, tenant_id: &Uuid7) -> Result<Strategy> {
        match self.selection(tenant_id).await? {
            Some(row) if row.posture == Posture::SelfManaged => {
                let workspace = self
                    .primary_workspace(tenant_id)
                    .await?
                    // A tenant with no workspace at all is a violated bootstrap
                    // invariant — signup creates the primary workspace — and it
                    // is permanent, because nothing later creates one.
                    .ok_or_else(provider_no_workspace)?;
                managed::SelfManaged::prepare(row, workspace)
            }
            _platform => platform::Platform::prepare(self.platform_default().await?),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Providers, Resolution, credential};
    use serde::Deserialize;

    /// A two-field credential, which is what makes the positional reading
    /// reachable at all: `serde_json` fills a struct from a JSON array in
    /// declaration order.
    #[derive(Debug, Deserialize)]
    struct Pair {
        provider: String,
        api_key: String,
    }

    /// The field a refusal in these tests is reported against.
    const FIELD: &str = "provider";

    #[test]
    fn a_positional_array_is_not_a_credential() {
        // Left unguarded, this parses: `provider` = "anthropic", `api_key` =
        // "sk-live", and every shape check downstream passes. `loadJson`
        // refuses it because a vault body must be an OBJECT, and so does this.
        credential::<Pair>(br#"["anthropic","sk-live"]"#, FIELD)
            .expect_err("an array is not a credential, however well it lines up");
        for refused in [
            br#""a string""#.as_slice(),
            b"42".as_slice(),
            b"null".as_slice(),
            b"".as_slice(),
        ] {
            credential::<Pair>(refused, FIELD).expect_err("only an object is a credential");
        }
    }

    #[test]
    fn an_object_still_parses_through_its_leading_whitespace() {
        // The gate reads the first NON-SPACE byte, because a stored body may
        // have been written by anything that produces valid JSON.
        let parsed: Pair = credential(br#"  {"provider":"anthropic","api_key":"sk-live"}"#, FIELD)
            .expect("an indented object is still an object");

        assert_eq!(parsed.provider, "anthropic");
        assert_eq!(parsed.api_key, "sk-live");
    }

    /// A strategy is held across the vault `await` between `key` and
    /// `interpret`, so it has to cross a thread boundary in a multi-threaded
    /// runtime. Stated as a compile-time assertion rather than discovered as a
    /// `!Send` future three call sites up.
    const fn assert_shareable<T: Send + Sync + ?Sized>() {}

    #[test]
    fn a_strategy_survives_the_await_between_its_two_halves() {
        assert_shareable::<dyn Resolution>();
        // And the store itself, which every request-path handle clones.
        assert_shareable::<Providers>();
    }
}
