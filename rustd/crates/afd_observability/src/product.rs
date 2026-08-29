//! What this daemon reports to the product analytics it is measured by.
//!
//! # This is a PORT, not a new event set
//!
//! Every event here already fires in the daemon this replaces, under the same
//! name, carrying the same property keys. Nothing is added and nothing is
//! renamed, because the funnels, dashboards and alerts on the other end match
//! on those bytes — a rename is an observability migration, and it is not this
//! milestone's.
//!
//! # A deployment with no key is a value, not an `Option` at every call site
//!
//! `afd_fleet::bundle::Bundles::unconfigured` is the shape this follows. Most
//! deployments — every developer's, every test — configure no `PostHog` project,
//! and a caller that had to ask before reporting would be a caller that can
//! forget. [`Analytics::silent`] reports nothing and says so once at boot.
//!
//! # Reporting never blocks the request that caused it
//!
//! [`Analytics::report`] hands the event to the client's background transport
//! and returns. A product event is a thing we would LIKE to know; a request
//! waiting on an analytics endpoint is a request the user is waiting on.

mod properties;
mod telemetry;

use std::sync::Arc;

use posthog_rs::{Client, ClientOptions};

pub use self::telemetry::Telemetry;

/// Where this daemon's product events go.
///
/// `Arc` rather than the client itself: the client owns a background transport
/// and is neither `Clone` nor `Debug`, and every plane that reports holds a
/// handle to the same one — a second client would be a second batch queue and a
/// second flush to remember at shutdown.
#[derive(Clone)]
pub struct Analytics(Option<Arc<Client>>);

impl std::fmt::Debug for Analytics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Analytics")
            .field(&self.is_reporting())
            .finish()
    }
}

impl Analytics {
    /// The reporter for a deployment holding a project key.
    ///
    /// `host` is the ingestion host, when this deployment names one — a
    /// self-hosted `PostHog`, or the EU region. `None` is `PostHog`'s own default.
    pub async fn resolve(project_key: &str, host: Option<&str>) -> Self {
        let mut options = ClientOptions::from(project_key);
        if let Some(host) = host {
            options = ClientOptions::from((project_key, host));
        }
        Self(Some(Arc::new(posthog_rs::client(options).await)))
    }

    /// The reporter for a deployment holding none.
    #[must_use]
    pub const fn silent() -> Self {
        Self(None)
    }

    /// Whether anything is actually being reported.
    #[must_use]
    pub const fn is_reporting(&self) -> bool {
        self.0.is_some()
    }

    /// Queues one event. Returns as soon as it is queued, never on delivery.
    pub fn report(&self, telemetry: &Telemetry) {
        let Some(client) = self.0.as_ref() else {
            return;
        };
        client.capture(telemetry.event());
    }

    /// Delivers what is queued, for a process that is going away.
    ///
    /// Called in shutdown order BEFORE the pools close: an event queued by the
    /// last request served is one this daemon still owes, and dropping the
    /// client without this would discard it.
    pub async fn flush(&self) {
        if let Some(client) = self.0.as_ref() {
            client.shutdown().await;
        }
    }
}

#[cfg(test)]
mod tests;
