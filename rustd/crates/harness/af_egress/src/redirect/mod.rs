//! One allowed GET chain; the policy check and transport remain owned by the caller.

use std::collections::BTreeSet;
use std::time::Instant;

use reqwest::Url;

use crate::{Draft, Inbound, Placement, Result};

mod privacy;
mod target;
pub use target::Target;

/// The Fetch standard permits twenty continuations. This is the default when the
/// runner's `[egress] max_redirects` names nothing else.
pub const MAX_REDIRECTS: u8 = 20;

/// The memory the automaton masking a redirect's secrets in its response may
/// build when the runner's `[egress] max_secret_mask_mib` names nothing else.
pub const MASK_MATCHER_BYTES: usize = 10 * 1024 * 1024;

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
    max_hops: u8,
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

    /// The hostname of the current hop, for host-only access evidence.
    #[must_use]
    pub fn host(&self) -> &str {
        self.current.host_str().unwrap_or_default()
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

    /// Begin after the first request's policy check, retaining only modeled
    /// headers, following at most `limits.max_redirects` before giving up and
    /// masking within `limits.mask_matcher_bytes`.
    #[must_use]
    pub fn new(current: &Url, headers: Vec<(String, String)>, limits: crate::Limits) -> Self {
        Self {
            current: current.clone(),
            headers,
            seen: BTreeSet::from([identity(current)]),
            privacy: privacy::Privacy::new(limits.mask_matcher_bytes),
            hops: 0,
            max_hops: limits.max_redirects.min(MAX_REDIRECTS),
            stripped: false,
            started: false,
            ending: "interrupted",
            began: Instant::now(),
            status: 0,
        }
    }

    /// Draft a continuation, which must pass the same run's egress policy before send.
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
        if self.hops >= self.max_hops {
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
            let hop_count = self.hops;
            let status = self.status;
            let stripped_headers = self.stripped;
            let reason = self.ending;
            let elapsed_ms = self.began.elapsed().as_millis();
            if self.ending == "completed" {
                {
                    let event = &"http_redirect_completed";
                    tracing::info!(
                        event,
                        hop_count,
                        status,
                        stripped_headers,
                        reason,
                        elapsed_ms
                    );
                };
            } else {
                let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                {
                    let event = &"http_redirect_failed";
                    tracing::warn!(
                        event,
                        hop_count,
                        status,
                        stripped_headers,
                        reason,
                        elapsed_ms,
                        error_code
                    );
                };
            }
        }
    }
}

fn identity(url: &Url) -> String {
    let mut normalized = url.clone();
    normalized.set_fragment(None);
    normalized.to_string()
}

#[cfg(test)]
#[path = "privacy/tests.rs"]
mod privacy_tests;
#[cfg(test)]
mod tests;
