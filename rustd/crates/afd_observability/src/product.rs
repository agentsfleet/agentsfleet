//! What this daemon reports to the product analytics it is measured by.
//!
//! # Ported events keep their bytes; new ones only add names
//!
//! Eleven events fired in the daemon this replaces, and each keeps its name and
//! property keys, because the funnels, dashboards and alerts on the other end
//! match on those bytes — a rename is an observability migration. Events added
//! since, such as the invite email's, arrive under names nothing matched
//! before, so they rename nothing.
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
#[cfg(feature = "test-util")]
use std::sync::{Mutex, PoisonError};

use posthog_rs::{Client, ClientOptions};

pub use self::telemetry::{InviteEmailOutcome, Telemetry};

/// Where this daemon's product events go.
///
/// `Arc` rather than the client itself: the client owns a background transport
/// and is neither `Clone` nor `Debug`, and every plane that reports holds a
/// handle to the same one — a second client would be a second batch queue and a
/// second flush to remember at shutdown.
#[derive(Clone)]
pub struct Analytics(Sink);

/// The three places an event can go. A recording exists only for suites that
/// prove a route reported what it did; production cannot build one.
#[derive(Clone)]
enum Sink {
    Silent,
    PostHog(Arc<Client>),
    #[cfg(feature = "test-util")]
    Recording(Recorded),
}

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
        Self(Sink::PostHog(Arc::new(posthog_rs::client(options).await)))
    }

    /// The reporter for a deployment holding none.
    #[must_use]
    pub const fn silent() -> Self {
        Self(Sink::Silent)
    }

    /// A reporter that keeps every event, and the handle a suite reads them
    /// back through.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn recording() -> (Self, Recorded) {
        let recorded = Recorded::default();
        (Self(Sink::Recording(recorded.clone())), recorded)
    }

    /// Whether anything is actually being reported.
    #[must_use]
    pub const fn is_reporting(&self) -> bool {
        !matches!(self.0, Sink::Silent)
    }

    /// Queues one event. Returns as soon as it is queued, never on delivery.
    pub fn report(&self, telemetry: &Telemetry) {
        match &self.0 {
            Sink::Silent => {}
            Sink::PostHog(client) => client.capture(telemetry.event()),
            #[cfg(feature = "test-util")]
            Sink::Recording(recorded) => recorded.push(telemetry),
        }
    }

    /// Delivers what is queued, for a process that is going away.
    ///
    /// Called in shutdown order BEFORE the pools close: an event queued by the
    /// last request served is one this daemon still owes, and dropping the
    /// client without this would discard it.
    pub async fn flush(&self) {
        if let Sink::PostHog(client) = &self.0 {
            client.shutdown().await;
        }
    }
}

/// The events a recording reporter kept, in the order they were reported.
///
/// A suite whose thread panicked while holding the lock still reads every
/// event: the list is only ever appended to, so a poisoned guard holds a whole
/// list, and dropping events there would turn one failure into a second,
/// misleading one.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone, Default)]
pub struct Recorded(Arc<Mutex<Vec<Telemetry>>>);

#[cfg(feature = "test-util")]
impl Recorded {
    /// Every event so far.
    #[must_use]
    pub fn events(&self) -> Vec<Telemetry> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn push(&self, telemetry: &Telemetry) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(telemetry.clone());
    }
}

#[cfg(test)]
mod tests;
