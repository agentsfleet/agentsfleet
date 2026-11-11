//! Whose key this run dials with, resolved from the saved setup for every lease.
//!
//! A saved reference names either the platform default or one of the tenant's
//! own registry entries. Resolving it ([`Providers::resolve_reference`], and
//! [`Providers::resolve_observed`] for the lease path) takes three steps, and
//! only the middle one touches the vault:
//!
//! ```text
//!   row      = read(reference)     ← the priced model, the credential's name,
//!                                    the saved settings; no ciphertext
//!   body     = open(row.credential)  ← the vault, one statement
//!   resolved = vet(body) + key rule  ← pure: the provider, the key, the endpoint
//! ```
//!
//! The platform default is the one reference a replica may answer from memory:
//! when the lease claim's witnesses name a held entry ([`cache`]) the vault is
//! not read at all. A tenant's own entry is read every time.
//!
//! # Where this crate keeps it, and where it may have to move
//!
//! A read of shared tenant state like this one would normally sit in `afd_state`.
//! It is in this crate instead because its only caller today is the lease
//! verb, and because the rich refusal classification it produces
//! ([`crate::Error::is_config_permanent`]) is this crate's error type. The
//! tenant plane resolves the same credentials for `PUT /v1/tenants/me/provider`
//! and must NOT reach them by depending on the runner-plane crate: when that
//! lands, this module moves down to `afd_state` and both callers import it.
//! Recorded here rather than pre-built: a seam with one caller is a guess.

mod activate;
pub mod cache;
mod cap;
mod decision;
mod endpoint;
pub mod entries;
mod managed;
mod registered;
mod reset;
mod resolved;
mod runtime;
mod secret_kind;
mod selection;
pub mod sql;
mod ssrf;
mod store;
mod vetted;
mod wire;

use serde::de::DeserializeOwned;

use crate::error::{Result, provider_malformed};

pub use self::activate::Activation;
pub use self::endpoint::{OPENAI_COMPATIBLE, Rejection};
pub use self::entries::{
    Added, Boundary, CatalogueRate, Entry, PricedDefault, RegistryPage, RegistryRow, Removed,
    Retargeted,
};
pub use self::registered::{PLATFORM_MODEL_REF, RegisteredModel, registered_model};
pub use self::resolved::{Dialled, Resolved};
pub use self::runtime::{RuntimeModel, runtime_model};
pub use self::secret_kind::SecretKind;
pub use self::selection::{OwnModel, PlatformDefault, Selection};
pub use self::store::Providers;
pub use self::wire::Wire;

/// Validates the provider/base-url pairing without exposing endpoint internals.
///
/// Platform administration and runner resolution must make the same SSRF and
/// provider-pairing decision. This façade shares that decision across crates
/// while keeping the parsed endpoint and host private to the runner plane.
///
/// # Errors
/// Refuses a missing or unsafe compatible endpoint and a named provider that
/// carries any endpoint.
pub fn validate_endpoint_pair(provider: &str, base_url: Option<&str>) -> Result<(), Rejection> {
    endpoint::resolve(provider, base_url).map(|_endpoint| ())
}

/// One credential body, typed — refusing anything that is not a JSON OBJECT.
///
/// The gate lives in [`af_core::json`], which carries the reasoning: a derived
/// struct fills from a JSON ARRAY positionally, so `["anthropic","sk-live"]`
/// parses as a credential with a provider and a key and passes every shape
/// check after it. `loadJson` refuses a non-object at the top and so does this.
///
/// What is local is only the ERROR: every way a credential body can be
/// unreadable answers `field`, because a caller is told none of them.
///
/// # Errors
/// Reports a body that is not an object, and one the credential's own shape
/// cannot read.
fn credential<T: DeserializeOwned>(body: &[u8], field: &'static str) -> Result<T> {
    af_core::json::object_from_slice(body).map_err(|_shape| provider_malformed(field))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{OPENAI_COMPATIBLE, Rejection, credential, validate_endpoint_pair};
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

    #[test]
    fn the_pairing_facade_refuses_what_resolution_refuses() {
        // A named provider carries no endpoint, and the compatible one carries a
        // safe one: the operator route asks this before a key is stored.
        assert_eq!(validate_endpoint_pair("anthropic", None), Ok(()));
        assert_eq!(
            validate_endpoint_pair(OPENAI_COMPATIBLE, Some("https://api.example.com/v1")),
            Ok(())
        );
        for (provider, base_url) in [
            ("anthropic", Some("https://api.example.com/v1")),
            (OPENAI_COMPATIBLE, None),
            (OPENAI_COMPATIBLE, Some("https://169.254.169.254")),
        ] {
            assert!(
                validate_endpoint_pair(provider, base_url).is_err(),
                "{provider} {base_url:?}"
            );
        }
        assert_eq!(
            validate_endpoint_pair(OPENAI_COMPATIBLE, None),
            Err(Rejection::Required)
        );
    }
}
