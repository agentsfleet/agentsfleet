//! Bounded redirect metadata; formatting never reveals its download authority.

use std::fmt;

use reqwest::Url;
use reqwest::header::{HeaderMap, LOCATION};

use super::Stop;

/// The largest raw or resolved redirect URL admitted for one hop.
const TARGET_MAX_BYTES: usize = 8192;

#[derive(garde::Validate)]
struct Written {
    #[garde(length(bytes, min = 1, max = TARGET_MAX_BYTES))]
    value: String,
}

/// A validated, opaque Location and its absolute destination.
#[derive(Clone, PartialEq, Eq)]
pub struct Target {
    pub(super) raw: String,
    pub(super) resolved: Url,
}

impl Target {
    /// One bounded Location, resolved using the current request URL.
    pub(crate) fn new(base: &Url, raw: &str) -> Result<Self, Stop> {
        let written = garde::Unvalidated::new(Written {
            value: raw.to_owned(),
        })
        .validate()
        .map_err(|_invalid| Stop::InvalidTarget)?;
        Self::resolve(base, &written)
    }

    fn resolve(base: &Url, raw: &garde::Valid<Written>) -> Result<Self, Stop> {
        let resolved = base
            .join(&raw.value)
            .map_err(|_invalid| Stop::InvalidTarget)?;
        let bounded = garde::Unvalidated::new(Written {
            value: resolved.to_string(),
        })
        .validate()
        .map_err(|_invalid| Stop::InvalidTarget)?;
        let resolved = Url::parse(&bounded.value).map_err(|_invalid| Stop::InvalidTarget)?;
        Ok(Self {
            raw: raw.value.clone(),
            resolved,
        })
    }

    /// Only a single valid header can authorize a continuation.
    pub(crate) fn from_headers(base: &Url, headers: &HeaderMap) -> Option<Self> {
        let mut values = headers.get_all(LOCATION).iter();
        let raw = values.next()?.to_str().ok()?;
        if values.next().is_some() {
            return None;
        }
        Self::new(base, raw).ok()
    }

    /// The non-secret projection exposed by single-hop tools.
    #[must_use]
    pub fn origin(&self) -> String {
        self.resolved.origin().ascii_serialization()
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Target")
            .field("origin", &self.origin())
            .finish_non_exhaustive()
    }
}
