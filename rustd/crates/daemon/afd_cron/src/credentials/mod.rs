//! The scheduler credentials this deployment keeps in the admin workspace's
//! vault, under the `qstash` bag, read when a call needs them.
//!
//! One [`afd_vault::bag::Bag`], like `afd_mail`'s `smtp-relay`: this module
//! declares only the schema; the load and the [`BagFault`] are the vault's.
//! Read on use rather than at boot, so a rotation is a bag update and not a
//! restart.

use af_crypto::secret::SecretString;
use afd_vault::bag::{Bag, BagFault, secret};
use serde::Deserialize;
use url::{Host, Url};

use crate::qstash::API_BASE;
use crate::verifier::SigningKeys;

/// The platform bag holding the scheduler credentials, in the admin
/// workspace's vault. `playbooks/lib/platform_secret_sync.sh` writes it.
pub(crate) const QSTASH_BAG: &str = "qstash";

/// The bag as it is stored. `Debug` prints no value: the secret fields are
/// [`SecretString`], whose `Debug` is redacted.
#[derive(Debug, Deserialize)]
pub(crate) struct QstashBag {
    token: Option<SecretString>,
    current_signing_key: Option<SecretString>,
    next_signing_key: Option<SecretString>,
    url: Option<String>,
}

/// Usable scheduler credentials, parsed once from their bag.
///
/// Both signing keys or none: one key is a rotation half done, and a verifier
/// holding only the current key refuses every delivery the vendor has moved to
/// the next one.
#[derive(Debug, Clone)]
pub(crate) struct Credentials {
    /// This deployment's bearer for the scheduler.
    pub(crate) token: SecretString,
    /// Where the management calls go: the bag's `url`, else [`API_BASE`].
    pub(crate) api_base: String,
    /// What a fire's signature is checked against.
    pub(crate) keys: SigningKeys,
}

impl Bag for Credentials {
    type Stored = QstashBag;

    fn from_stored(stored: QstashBag) -> Result<Self, BagFault> {
        Ok(Self {
            token: secret(stored.token, "token")?,
            api_base: api_base(stored.url)?,
            keys: SigningKeys {
                current: secret(stored.current_signing_key, "current_signing_key")?,
                next: secret(stored.next_signing_key, "next_signing_key")?,
            },
        })
    }
}

/// The bag's `url` as the management base: the vendor default when the bag names
/// none, else an `https` address, or `http` on a loopback host, with no query or
/// fragment.
///
/// Parsed here so a typo is a fault naming the field, where an address the HTTP
/// client cannot send to would read as a scheduler that is down and be retried.
fn api_base(url: Option<String>) -> Result<String, BagFault> {
    let Some(url) = url.filter(|url| !url.is_empty()) else {
        return Ok(API_BASE.to_owned());
    };
    let parsed = Url::parse(&url).map_err(|_unparsed| BagFault::Unparsed("url"))?;
    let loopback = match parsed.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    // The scheduler's bearer rides every call under this base, so cleartext is
    // for the local emulator only, as the boot checks do for a token's base.
    let carries_a_bearer_safely = match parsed.scheme() {
        "https" => parsed.host().is_some(),
        "http" => loopback,
        _other => false,
    };
    if !carries_a_bearer_safely || parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(BagFault::Unparsed("url"));
    }
    Ok(parsed.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests;
