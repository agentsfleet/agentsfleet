//! The relay the `smtp-relay` platform bag names, and how this daemon reaches it.
//!
//! The bag is five string fields written by `playbooks/lib/platform_secret_sync.sh`.
//! A bag missing any of them, or carrying one this build cannot use, reads as
//! no relay at all: the invite then records `unconfigured`, which is the truth
//! an operator can act on, rather than a send attempted with half a credential.

use std::net::IpAddr;
use std::time::Duration;

use afd_crypto::secret::SecretBytes;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, Tokio1Executor};

/// The platform bag holding the relay, in the admin workspace's vault.
pub const SMTP_RELAY_BAG: &str = "smtp-relay";

const FIELD_HOST: &str = "host";
const FIELD_PORT: &str = "port";
const FIELD_USERNAME: &str = "username";
const FIELD_PASSWORD: &str = "password";
const FIELD_FROM_ADDRESS: &str = "from_address";

/// The port SMTP over implicit Transport Layer Security (TLS) answers on.
pub(crate) const SMTPS_PORT: u16 = 465;

/// The one host name that means loopback without being an address.
const LOCALHOST: &str = "localhost";

/// How a connection to the relay is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Security {
    /// TLS from the first byte: port 465.
    Implicit,
    /// A plain connection upgraded with STARTTLS before any credential is
    /// sent; a relay that will not upgrade is refused.
    StartTls,
    /// No TLS. Only to a loopback host, where the message never leaves the
    /// machine: the local Docker daemon and the integration lane reaching
    /// Mailpit.
    Plain,
}

impl Security {
    /// The protection a relay at `host:port` gets.
    ///
    /// Derived rather than configured, so no bag can ask for plaintext to a
    /// host on the network.
    #[must_use]
    pub(crate) fn of(host: &str, port: u16) -> Self {
        if is_loopback(host) {
            Self::Plain
        } else if port == SMTPS_PORT {
            Self::Implicit
        } else {
            Self::StartTls
        }
    }
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case(LOCALHOST)
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// A usable relay, parsed once from its bag.
#[derive(Debug)]
pub(crate) struct Relay {
    host: String,
    port: u16,
    username: String,
    password: SecretBytes,
    pub(crate) from: Mailbox,
}

impl Relay {
    /// The relay a stored bag names, or `None` when the bag cannot be used.
    pub(crate) fn parse(stored: &SecretBytes) -> Option<Self> {
        let bag: serde_json::Value = serde_json::from_slice(stored.expose()).ok()?;
        let password = field(&bag, FIELD_PASSWORD)?;
        Some(Self {
            host: field(&bag, FIELD_HOST)?.to_owned(),
            port: field(&bag, FIELD_PORT)?.parse().ok()?,
            username: field(&bag, FIELD_USERNAME)?.to_owned(),
            password: SecretBytes::new(password.as_bytes().to_vec()),
            from: field(&bag, FIELD_FROM_ADDRESS)?.parse().ok()?,
        })
    }

    /// A transport to this relay, each SMTP command bounded by `timeout`.
    ///
    /// Built per send so a rotated password takes effect on the next invite.
    /// The password leaves [`SecretBytes`] here because lettre's credentials
    /// hold a `String`; it lives as long as this one transport.
    pub(crate) fn transport(
        &self,
        timeout: Duration,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, lettre::transport::smtp::Error> {
        let builder = match Security::of(&self.host, self.port) {
            Security::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host)?,
            Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)?,
            Security::Plain => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.host),
        };
        let password = String::from_utf8_lossy(self.password.expose()).into_owned();
        Ok(builder
            .port(self.port)
            .credentials(Credentials::new(self.username.clone(), password))
            .timeout(Some(timeout))
            .build())
    }
}

/// One non-empty string field of the bag.
fn field<'b>(bag: &'b serde_json::Value, name: &str) -> Option<&'b str> {
    bag.get(name)?.as_str().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;
