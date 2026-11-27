//! A host grant names one DNS hostname, in the ASCII form used by the network policy.

use garde::Error;
use serde::{Deserialize, Serialize};
use url::Host;

/// A DNS hostname for a web-host grant, canonicalized before any lookup or recording.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Hostname(String);

impl Hostname {
    /// Parse a bare, fully qualified hostname. Address literals, URL syntax, and
    /// local-only names cannot be requested, including alternate IPv4 spellings.
    ///
    /// # Errors
    /// Returns a bounded validation message without echoing the submitted text.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let invalid = || Error::new("host must be a bare DNS hostname with at least two labels");
        if text.contains('%') {
            return Err(invalid());
        }
        let Host::Domain(domain) = Host::parse(text).map_err(|_invalid_host| invalid())? else {
            return Err(invalid());
        };
        let domain = domain.strip_suffix('.').unwrap_or(&domain);
        // RFC 1035 limits each label to 63 octets and the wire name to 255
        // including the length prefixes and root label (253 in presentation).
        if domain.len() > 253
            || !domain.contains('.')
            || domain.ends_with(".localhost")
            || !domain.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err(invalid());
        }
        Ok(Self(domain.to_owned()))
    }

    /// The canonical host used as the grant's service and allowed origin host.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Hostname {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

/// The tool whose actual network refusal supplied the redirect evidence.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusedTool {
    /// A page read encountered a redirect outside the fleet's network policy.
    WebFetch,
}

/// The adjacent hosts observed on a refused redirect during this run.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedirectEvidence {
    /// The tool executing when the redirect was refused.
    pub tool: RefusedTool,
    /// The host that returned the redirect.
    pub from_host: Hostname,
    /// The destination refused before connection.
    pub to_host: Hostname,
}
