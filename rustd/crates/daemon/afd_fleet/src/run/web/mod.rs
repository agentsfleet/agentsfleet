//! Platform-keyed search and extract. Each dispatched request has a committed
//! reservation under the calling runner's current fence and public policy.

use std::{sync::Arc, time::Duration};

use af_core::{
    clock::{self, Clock, SystemClock},
    id::Uuid7,
};
use af_providers::web::{Exa, Parallel, Vendor, WebFailure};
use moka::future::Cache;
use tokio::sync::OnceCell;
use tokio::time::Instant;

use self::reserve::Claim;
use crate::{
    Result,
    run::{RunService, Runs},
};

mod credential;
mod extract;
mod failure;
mod reserve;
mod search;
pub use self::failure::Failure;

const SEARCH: &str = "search";
const EXTRACT: &str = "extract";

/// The platforms whose fixed search priority and key names the daemon owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VendorName {
    /// Fast search and full page extraction.
    Parallel,
    /// Search fallback.
    Exa,
}

impl VendorName {
    /// The platform vault name, ledger key, and safe telemetry label.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Parallel => "parallel",
            Self::Exa => "exa",
        }
    }
}

#[derive(Debug)]
struct Clients {
    parallel: Parallel,
    exa: Exa,
}

impl Clients {
    fn vendor(&self, name: VendorName) -> &dyn Vendor {
        match name {
            VendorName::Parallel => &self.parallel,
            VendorName::Exa => &self.exa,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Web {
    admin: Option<Uuid7>,
    clock: Arc<dyn Clock>,
    // Two possible keys, one for each vendor, shared across all runs on this daemon.
    cooling: Cache<VendorName, jiff::Timestamp>,
    clients: Arc<OnceCell<Clients>>,
    #[cfg(any(test, feature = "test-util"))]
    post_commit: Option<Arc<tokio::sync::Notify>>,
}

impl Default for Web {
    fn default() -> Self {
        Self {
            admin: None,
            clock: Arc::new(SystemClock),
            cooling: Cache::new(2),
            clients: Arc::default(),
            #[cfg(any(test, feature = "test-util"))]
            post_commit: None,
        }
    }
}

impl Web {
    async fn clients(&self) -> Result<&Clients> {
        self.clients
            .get_or_try_init(async || {
                Ok(Clients {
                    parallel: Parallel::new().map_err(failure::client)?,
                    exa: Exa::new().map_err(failure::client)?,
                })
            })
            .await
    }

    async fn wait(&self, vendor: VendorName) -> Option<Duration> {
        let until = self.cooling.get(&vendor).await?;
        let remaining = clock::elapsed(self.clock.now(), until);
        (!remaining.is_zero()).then_some(remaining)
    }

    async fn cool(&self, vendor: VendorName, wait: Duration) {
        let until = clock::after(self.clock.now(), wait);
        self.cooling
            .entry(vendor)
            .and_upsert_with(|prior| {
                std::future::ready(prior.map_or(until, |entry| entry.into_value().max(until)))
            })
            .await;
        tracing::info!(
            event = "web_vendor_cooling",
            vendor = vendor.name(),
            wait_ms = clock::saturating_millis(wait)
        );
    }
}

impl Runs {
    /// Resolve web-vendor keys only in this deployment's administrator workspace.
    #[must_use]
    pub fn with_web_workspace(mut self, admin: Option<Uuid7>) -> Self {
        self.web.admin = admin;
        self
    }

    /// Interrupt a test invocation at the commit/send boundary. After the
    /// notification its caller drops the tool future, as on process exit.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn stop_after_web_reservation(mut self, committed: Arc<tokio::sync::Notify>) -> Self {
        self.web.post_commit = Some(committed);
        self
    }

    /// Point vendor calls at a fake server and control the named cooldown clock.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn with_web_clients(mut self, parallel: Parallel, exa: Exa, clock: Arc<dyn Clock>) -> Self {
        self.web.clients = Arc::new(OnceCell::new_with(Some(Clients { parallel, exa })));
        self.web.clock = clock;
        self
    }
}

impl RunService {
    async fn web_observed<T>(
        &self,
        claim: &Claim<'_>,
        fleet: &Uuid7,
        vendor: VendorName,
        operation: &'static str,
        start: Instant,
        result: &core::result::Result<T, WebFailure>,
    ) {
        let failed = result.as_ref().err();
        let status = failed.and_then(status);
        let outcome = if failed.is_some() {
            "failure"
        } else {
            "success"
        };
        tracing::info!(event = "web_call", run_id = %claim.run, fleet_id = %fleet,
            vendor = vendor.name(), operation, outcome, ms = clock::saturating_millis(start.elapsed()), vendor_status = status);
        af_observability::producers::fleet::web::dispatched(vendor.name(), operation, outcome);
        if let Some(WebFailure::Status {
            retry_after: Some(wait),
            ..
        }) = failed
        {
            self.runs.web.cool(vendor, *wait).await;
        }
    }
}

fn status(failure: &WebFailure) -> Option<u16> {
    match failure {
        WebFailure::Status { status, .. } | WebFailure::Page { status } => Some(*status),
        _ => None,
    }
}

fn remaining(deadline: Instant) -> Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(Failure::Deadline.into())
    } else {
        Ok(left)
    }
}
