//! Ephemeral signals; Postgres owns all durable facts and authority.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;

mod command;
pub mod dragonfly;
pub mod error;
pub mod nats;
mod subject;
mod subscription;
#[cfg(any(test, feature = "test-util"))]
pub mod test_util;
pub mod wake;

pub use command::Command;
pub use error::{Error, Result};
pub use subject::Subject;
pub use subscription::{Message, Received, Subscription};

/// Boot selects one adapter; callers never inspect or downcast it.
#[async_trait]
pub trait Bus: Debug + Send + Sync {
    /// Publish one ephemeral signal.
    ///
    /// # Errors
    /// Returns a broker refusal or unavailable connection.
    async fn publish(&self, subject: &Subject, payload: &str) -> Result<()>;
    /// Publish a batch in order on one subject.
    ///
    /// # Errors
    /// Returns a broker failure; a prefix may already have reached subscribers.
    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> Result<()>;
    /// Subscribe before reporting the consumer ready.
    ///
    /// # Errors
    /// Returns an unavailable or closed adapter.
    async fn subscribe(&self, subject: &Subject) -> Result<Subscription>;
    /// Idempotently close subscriptions and finish the adapter's connection.
    ///
    /// # Errors
    /// Returns a native shutdown failure; the caller supplies the drain deadline.
    async fn shutdown(&self) -> Result<()>;
    /// Whether the adapter holds a live connection right now.
    ///
    /// `/readyz` reports it and never gates on it. The default answers `true`
    /// for an adapter with no connection to lose, such as a test double.
    async fn connected(&self) -> bool {
        true
    }
}

/// Shared process-wide adapter, with no broker type at a domain call site.
#[derive(Debug, Clone)]
pub struct Handle(Arc<dyn Bus>);

impl Handle {
    /// Own one adapter behind the boot-selected object boundary.
    #[must_use]
    pub fn new(bus: impl Bus + 'static) -> Self {
        Self(Arc::new(bus))
    }

    /// Publish one signal.
    ///
    /// # Errors
    /// Preserves the adapter's failure.
    pub async fn publish(&self, subject: &Subject, payload: &str) -> Result<()> {
        self.0.publish(subject, payload).await
    }

    /// Publish ordered signals on one subject.
    ///
    /// # Errors
    /// Preserves the adapter's failure, including partial delivery.
    pub async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> Result<()> {
        self.0.publish_batch(subject, payloads).await
    }

    /// Open a subscription with backend-neutral gap and lag notifications.
    ///
    /// # Errors
    /// Preserves the adapter's failure.
    pub async fn subscribe(&self, subject: &Subject) -> Result<Subscription> {
        self.0.subscribe(subject).await
    }

    /// Stop this process's signal surface during drain.
    ///
    /// # Errors
    /// Preserves an adapter shutdown failure, including an interrupted drain.
    pub async fn shutdown(&self) -> Result<()> {
        self.0.shutdown().await
    }

    /// Whether the adapter holds a live connection right now; see [`Bus::connected`].
    pub async fn connected(&self) -> bool {
        self.0.connected().await
    }

    /// Serialize and publish a frame, retaining either failure for the caller's policy.
    ///
    /// # Errors
    /// Returns the original serialization or broker failure; no boolean represents failure.
    pub async fn publish_frame<F: Serialize + Sync>(
        &self,
        subject: &Subject,
        frame: &F,
    ) -> Result<()> {
        let payload = serde_json::to_string(frame)?;
        self.publish(subject, &payload).await
    }
}

/// Consume a notification outcome after a durable commit without changing that commit's answer.
/// This observer records failure; publishers themselves always return Result to their caller.
pub fn observe_notification(subject: &Subject, outcome: Result<()>) {
    if let Err(error) = outcome {
        dropped(subject, &error);
    }
}

fn dropped(subject: &Subject, error: &Error) {
    let fleet_id = match subject {
        Subject::FleetTail(fleet) => Some(fleet.as_str()),
        _other => None,
    };
    let subject_kind = subject.kind();
    let error_code = error.code().as_str();
    let reason = error.reason();
    tracing::warn!(
        event = "tail_frame_dropped",
        fleet_id,
        subject_kind,
        error_code,
        reason
    );
}

#[cfg(test)]
mod tests;
