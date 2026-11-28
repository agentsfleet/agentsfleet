//! Caller-visible outcomes contain only bounded status codes and vendor names.

use std::{fmt, time::Duration};

use af_core::error_code::{self, ErrorCode};

use super::VendorName;
use crate::error::{Error, ErrorKind};

/// The typed reason attached to a refused web tool.
#[derive(Debug)]
pub enum Failure {
    /// Public egress must be explicitly enabled.
    Restricted,
    /// No usable platform credential exists.
    Unconfigured,
    /// Every configured vendor has named a wait.
    Cooling(Duration),
    /// Each physical search ended without a usable result.
    Vendors(Vec<(VendorName, Option<u16>)>),
    /// The extract target fails public HTTPS admission.
    InvalidUrl,
    /// Accounting or authority could not be proved.
    Reservation,
    /// A permanent upstream refusal for the requested page.
    Page(u16),
    /// Missing or ambiguous full contents.
    InvalidResponse,
    /// Temporary extraction outage after a fresh public-policy check.
    ExtractUnavailable(Option<Duration>),
    /// The complete tool budget elapsed.
    Deadline,
}

impl Failure {
    pub(super) fn caused_by(self, source: impl std::error::Error + Send + Sync + 'static) -> Error {
        ErrorKind::Web {
            reason: self,
            source: Some(Box::new(source)),
        }
        .into()
    }

    pub(crate) const fn code(&self) -> ErrorCode {
        match self {
            Self::Restricted => error_code::WEB_NETWORK_RESTRICTED,
            Self::Unconfigured => error_code::WEB_VENDOR_UNCONFIGURED,
            Self::Cooling(_) => error_code::WEB_VENDORS_COOLING,
            Self::Vendors(_) => error_code::WEB_VENDORS_FAILED,
            Self::InvalidUrl => error_code::WEB_INVALID_URL,
            Self::Reservation => error_code::WEB_RESERVATION_UNAVAILABLE,
            Self::Page(_) => error_code::WEB_PAGE_UNAVAILABLE,
            Self::InvalidResponse => error_code::WEB_EXTRACT_INVALID_RESPONSE,
            Self::ExtractUnavailable(_) => error_code::WEB_EXTRACT_UNAVAILABLE,
            Self::Deadline => error_code::WEB_DEADLINE_EXCEEDED,
        }
    }

    pub(crate) const fn sentence(&self) -> &'static str {
        match self {
            Self::Restricted => {
                "This fleet restricts web access. Turn on Allow all to search or extract a public page."
            }
            Self::Unconfigured => "No web vendor is configured.",
            Self::Cooling(_) => "The configured web vendors have requested a wait.",
            Self::Vendors(_) => "The configured web vendors could not answer the search.",
            Self::InvalidUrl => "A page read requires a public HTTPS URL without credentials.",
            Self::Reservation => "The web dispatch could not be reserved.",
            Self::Page(_) => "The requested page is unavailable.",
            Self::InvalidResponse => "The extract vendor returned an invalid page response.",
            Self::ExtractUnavailable(_) => "The extract vendor is temporarily unavailable.",
            Self::Deadline => "The web tool deadline expired.",
        }
    }

    /// Sanitized explanation, including per-vendor statuses when known.
    #[must_use]
    pub fn detail(&self) -> String {
        self.to_string()
    }

    /// A named wait from an upstream service.
    #[must_use]
    pub const fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Cooling(wait) => Some(*wait),
            Self::ExtractUnavailable(wait) => *wait,
            _ => None,
        }
    }

    /// Only a freshly authorized extract outage permits one direct read.
    #[must_use]
    pub const fn local_read_permitted(&self) -> bool {
        matches!(self, Self::ExtractUnavailable(_))
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.sentence())?;
        match self {
            Self::Page(status) => write!(f, " HTTP {status}."),
            Self::Vendors(attempts) => {
                for (vendor, status) in attempts {
                    write!(f, " {}", vendor.name())?;
                    match status {
                        Some(code) => write!(f, " (HTTP {code}).")?,
                        None => f.write_str(" (no usable response).")?,
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl From<Failure> for Error {
    fn from(reason: Failure) -> Self {
        ErrorKind::Web {
            reason,
            source: None,
        }
        .into()
    }
}

/// Preserve datastore failures for operators without exposing row contents.
pub(super) fn reservation(source: impl std::error::Error + Send + Sync + 'static) -> Error {
    Failure::Reservation.caused_by(source)
}

pub(super) fn deadline(source: tokio::time::error::Elapsed) -> Error {
    Failure::Deadline.caused_by(source)
}

pub(super) fn client(source: af_providers::Error) -> Error {
    Failure::Unconfigured.caused_by(source)
}
