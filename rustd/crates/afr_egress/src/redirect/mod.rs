//! One admitted GET chain; admission and transport remain owned by the caller.

use std::collections::BTreeSet;
use std::time::Instant;

use reqwest::Url;

use crate::{Draft, Inbound, Placement};

mod privacy;
mod target;
pub use target::Target;

/// The largest number of continuations for a single tool call.
pub const MAX_REDIRECTS: u8 = 3;

/// Closed stopping reasons; none holds private target data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Location was absent, ambiguous, malformed or beyond its bound.
    InvalidTarget,
    /// A normalized destination was already seen by this call.
    Loop,
    /// Another continuation would exceed the call's hop budget.
    HopLimit,
    /// The known-target masker could not be built.
    Privacy,
}

impl Stop {
    /// The safe terminal reason shown to the model and operator.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::InvalidTarget => "redirect has no single valid bounded Location",
            Self::Loop => "redirect loop refused",
            Self::HopLimit => "redirect limit exceeded",
            Self::Privacy => "redirect response could not be safely masked",
        }
    }
}

/// Whether a status can continue a bodyless GET.
#[must_use]
pub const fn follows(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Private state belongs to one call and is never formatted or serialized.
pub struct Chain {
    current: Url,
    headers: Vec<(String, String)>,
    seen: BTreeSet<String>,
    privacy: privacy::Privacy,
    hops: u8,
    stripped: bool,
    started: bool,
    ending: &'static str,
    began: Instant,
    status: u16,
}

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chain")
            .field("hops", &self.hops)
            .field("stripped_headers", &self.stripped)
            .finish_non_exhaustive()
    }
}

impl Chain {
    /// The safe destination projection for a continuation refusal.
    #[must_use]
    pub fn origin(&self) -> String {
        self.current.origin().ascii_serialization()
    }

    /// Whether private redirect metadata has entered this call.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }

    /// The last observed upstream status, without its private target.
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// The continuations already issued when inspecting an upstream response.
    #[must_use]
    pub const fn hops(&self) -> u8 {
        self.hops
    }

    /// Mark completion only after the caller has finished masking the response.
    pub fn finish(&mut self) {
        self.ending = if (200..300).contains(&self.status) {
            "completed"
        } else {
            "upstream_status"
        };
    }

    /// Begin after the first request's admission, retaining only modeled headers.
    #[must_use]
    pub fn new(current: &Url, headers: Vec<(String, String)>) -> Self {
        Self {
            current: current.clone(),
            headers,
            seen: BTreeSet::from([identity(current)]),
            privacy: privacy::Privacy::default(),
            hops: 0,
            stripped: false,
            started: false,
            ending: "interrupted",
            began: Instant::now(),
            status: 0,
        }
    }

    /// Draft a continuation, which must pass the same lease's admission before send.
    ///
    /// # Errors
    /// Returns a safe stopping reason for an invalid target, loop, exhausted
    /// hop budget or response that cannot be safely masked.
    pub fn next(&mut self, response: &mut Inbound) -> Result<Option<Draft>, Stop> {
        self.status = response.status;
        if !follows(response.status) {
            self.privacy.clean(response).inspect_err(|reason| {
                self.ending = reason.reason();
            })?;
            response.followed_redirects = self.hops;
            return Ok(None);
        }
        if !self.started {
            self.started = true;
            tracing::info!(event = "http_redirect_started", status = response.status);
        }
        let next = self.continuation(response);
        if let Err(reason) = next {
            self.ending = reason.reason();
        }
        next.map(Some)
    }

    fn continuation(&mut self, response: &mut Inbound) -> Result<Draft, Stop> {
        let target = response.redirect.take().ok_or(Stop::InvalidTarget)?;
        self.privacy.add(&target);
        if self.seen.contains(&identity(&target.resolved)) {
            return Err(Stop::Loop);
        }
        if self.hops == MAX_REDIRECTS {
            return Err(Stop::HopLimit);
        }
        if self.current.origin() != target.resolved.origin() {
            self.headers.clear();
            self.stripped = true;
        }
        self.current = target.resolved;
        self.seen.insert(identity(&self.current));
        self.hops += 1;
        Ok(Draft {
            method: "GET".to_owned(),
            url: self.current.to_string(),
            headers: self.headers.clone(),
            body: None,
            placement: Placement::Authorization,
        })
    }
}

impl Drop for Chain {
    fn drop(&mut self) {
        if self.started {
            let event = if self.ending == "completed" {
                "http_redirect_completed"
            } else {
                "http_redirect_failed"
            };
            tracing::info!(
                event,
                hop_count = self.hops,
                status = self.status,
                stripped_headers = self.stripped,
                reason = self.ending,
                elapsed_ms = self.began.elapsed().as_millis()
            );
        }
    }
}

fn identity(url: &Url) -> String {
    let mut normalized = url.clone();
    normalized.set_fragment(None);
    normalized.to_string()
}

#[cfg(test)]
mod privacy_tests;
#[cfg(test)]
mod tests;
