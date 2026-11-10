//! Dragonfly's sharded channel spelling and process-wide subscription owner.

pub(crate) mod subscribe;

use afd_dragonfly::{Dragonfly, DragonflyConfig, FleetStreams, SubscriptionHub};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{Bus, Handle, Result, Subject, Subscription, error::ErrorKind};

/// Start the process's one sharded subscription hub and its publishing adapter.
///
/// # Errors
/// Returns the native startup failure; boot can retain publication with [`publisher`].
pub async fn connect(client: Dragonfly, config: DragonflyConfig) -> Result<Handle> {
    let hub = SubscriptionHub::start(config).await?;
    Ok(attached(client, Some(hub)))
}

/// Keep publication available when initial subscription startup failed.
/// Subscription attempts fail; the existing silent live surface is selected by boot.
#[must_use]
pub fn publisher(client: Dragonfly) -> Handle {
    attached(client, None)
}

pub(crate) fn attached(client: Dragonfly, hub: Option<SubscriptionHub>) -> Handle {
    Handle::new(Adapter {
        live: RwLock::new(Some(Resources {
            streams: FleetStreams::new(client),
            hub,
        })),
    })
}

#[derive(Debug)]
struct Adapter {
    // Shared reads allow concurrent operations. Shutdown takes the write lock,
    // waits for in-flight publication, then prevents subscriptions reopening.
    live: RwLock<Option<Resources>>,
}

#[derive(Debug)]
struct Resources {
    streams: FleetStreams,
    hub: Option<SubscriptionHub>,
}

#[async_trait]
impl Bus for Adapter {
    async fn publish(&self, subject: &Subject, payload: &str) -> Result<()> {
        let live = self.live.read().await;
        let resources = live.as_ref().ok_or(ErrorKind::Closed)?;
        resources
            .streams
            .publish(&channel(subject), payload)
            .await?;
        Ok(())
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> Result<()> {
        let live = self.live.read().await;
        let resources = live.as_ref().ok_or(ErrorKind::Closed)?;
        Ok(resources
            .streams
            .publish_batch(&channel(subject), payloads)
            .await?)
    }

    async fn subscribe(&self, subject: &Subject) -> Result<Subscription> {
        let live = self.live.read().await;
        let resources = live.as_ref().ok_or(ErrorKind::Closed)?;
        let hub = resources.hub.as_ref().ok_or(ErrorKind::Closed)?;
        Ok(subscribe::open(
            hub.subscribe(&channel(subject)),
            subject.clone(),
        ))
    }

    async fn shutdown(&self) -> Result<()> {
        let mut live = self.live.write().await;
        if let Some(resources) = live.take()
            && let Some(hub) = resources.hub
        {
            hub.stopped().await;
        }
        Ok(())
    }
}

pub(crate) fn channel(subject: &Subject) -> String {
    match subject {
        Subject::FleetTail(fleet) => format!("fleet:{fleet}:activity"),
        Subject::DraftEvents(draft) => format!("draft:{draft}:events"),
        Subject::Wake(shard) => format!("fleet:wake:{{{shard}}}"),
        Subject::RunnerCommand(runner) => format!("runner:{{{runner}}}:commands"),
    }
}
