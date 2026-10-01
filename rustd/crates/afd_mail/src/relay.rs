//! The relay the `smtp-relay` platform bag names, and how this daemon reaches it.
//!
//! The bag is five string fields written by `playbooks/lib/platform_secret_sync.sh`.
//! A bag missing any of them, or carrying one this build cannot use, reads as
//! no relay at all: the invite then records `unconfigured`, which is the truth
//! an operator can act on, rather than a send attempted with half a credential.
//! The [`BagFault`] it reads as names the field to fix, never what it holds.

use std::net::IpAddr;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use afd_crypto::secret::{SecretBytes, SecretString};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use serde::Deserialize;

use crate::Result;

/// The platform bag holding the relay, in the admin workspace's vault.
pub const SMTP_RELAY_BAG: &str = "smtp-relay";

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

/// Why the admin workspace's vault yields no relay.
///
/// Fieldless but for the NAME of the field at fault, so the record an operator
/// reads says what to fix and never what the bag holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BagFault {
    /// No bag, or no admin workspace to hold one.
    Absent,
    /// Not a JSON object, or a field that is not a string.
    Malformed,
    /// A field absent or empty.
    Missing(&'static str),
    /// A field present but not a port, or not a mailbox.
    Unparsed(&'static str),
}

impl BagFault {
    /// What the `unconfigured` record calls this fault.
    #[must_use]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Malformed => "malformed",
            Self::Missing(_) => "missing",
            Self::Unparsed(_) => "unparsed",
        }
    }

    /// The field at fault, when one is.
    #[must_use]
    pub(crate) const fn field(self) -> Option<&'static str> {
        match self {
            Self::Missing(field) | Self::Unparsed(field) => Some(field),
            Self::Absent | Self::Malformed => None,
        }
    }
}

/// The bag as it is stored. Every field may be absent here, so an absent one
/// is refused by its name, as an empty one is, rather than as a bag that does
/// not parse.
#[derive(Deserialize)]
struct RelayBag {
    host: Option<String>,
    port: Option<String>,
    username: Option<String>,
    password: Option<SecretString>,
    from_address: Option<String>,
}

/// A usable relay, parsed once from its bag: the server and the sender.
#[derive(Debug)]
pub(crate) struct Relay {
    pub(crate) server: Server,
    pub(crate) from: Mailbox,
}

/// Where the relay listens, and the credential it takes.
#[derive(Debug)]
pub(crate) struct Server {
    host: String,
    port: u16,
    username: String,
    password: SecretString,
}

impl Relay {
    /// The relay a stored bag names, or the fault that makes it unusable.
    pub(crate) fn parse(stored: &SecretBytes) -> Result<Self, BagFault> {
        // serde's own message is dropped: it can quote a value, the password
        // among them, and the fault is logged.
        let bag: RelayBag =
            serde_json::from_slice(stored.expose()).map_err(|_quoted| BagFault::Malformed)?;
        Ok(Self {
            server: Server {
                host: filled(bag.host, "host")?,
                port: parsed(bag.port, "port")?,
                username: filled(bag.username, "username")?,
                password: bag
                    .password
                    .filter(|password| !password.is_empty())
                    .ok_or(BagFault::Missing("password"))?,
            },
            from: parsed(bag.from_address, "from_address")?,
        })
    }
}

impl Server {
    /// A transport to this server, each SMTP command bounded by `timeout`.
    ///
    /// Built per send from the bag just read, so a rotated password takes
    /// effect on the next invite; only the TLS parameters come from `tls`. The
    /// password leaves [`SecretString`] here because lettre's credentials hold
    /// a `String`; it lives as long as this one transport.
    pub(crate) fn transport(
        self,
        timeout: Duration,
        tls: &TlsCache,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, lettre::transport::smtp::Error> {
        let mode = match Security::of(&self.host, self.port) {
            Security::Implicit => Tls::Wrapper(tls.parameters(&self.host)?),
            Security::StartTls => Tls::Required(tls.parameters(&self.host)?),
            Security::Plain => Tls::None,
        };
        let credentials = Credentials::new(self.username, self.password.expose().to_owned());
        Ok(
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(self.host)
                .port(self.port)
                .tls(mode)
                .credentials(credentials)
                .timeout(Some(timeout))
                .build(),
        )
    }
}

/// The TLS parameters for the relay's host, built once and kept.
///
/// Building them loads and parses the system trust store, which is blocking
/// file work on a runtime worker; per send, every invite paid it. The first
/// host built is kept without a lock; a bag later naming another host builds
/// per send until the daemon restarts, since a relay that moves is rare.
#[derive(Clone, Default)]
pub(crate) struct TlsCache(Arc<OnceLock<(String, TlsParameters)>>);

impl TlsCache {
    /// The parameters for `host`, built on the first ask for it.
    pub(crate) fn parameters(
        &self,
        host: &str,
    ) -> Result<TlsParameters, lettre::transport::smtp::Error> {
        if let Some((built_for, parameters)) = self.0.get()
            && built_for == host
        {
            return Ok(parameters.clone());
        }
        let parameters = TlsParameters::new(host.to_owned())?;
        self.0.get_or_init(|| (host.to_owned(), parameters.clone()));
        Ok(parameters)
    }
}

/// lettre's parameters print nothing; the host they were built for is enough.
impl std::fmt::Debug for TlsCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let host = self.0.get().map(|(host, _)| host.as_str());
        f.debug_tuple("TlsCache").field(&host).finish()
    }
}

/// One field of the bag, refused by name when absent or empty.
fn filled(value: Option<String>, field: &'static str) -> Result<String, BagFault> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(BagFault::Missing(field))
}

/// One field of the bag, parsed, refused by name when it does not parse.
fn parsed<T: FromStr>(value: Option<String>, field: &'static str) -> Result<T, BagFault> {
    filled(value, field)?
        .parse()
        .map_err(|_unparsed| BagFault::Unparsed(field))
}

#[cfg(test)]
mod tests;
