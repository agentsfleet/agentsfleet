//! Only one subject's publishes are discarded; every other signal uses the real Bus.

use afd_bus::{Bus, Handle, Subject, Subscription};
use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
struct Dropping {
    bus: Handle,
    subject: Subject,
    lost: Arc<AtomicUsize>,
}

/// `bus`, with every publish on `subject` counted and discarded.
pub(in crate::integration_runner_stream) fn dropping(
    bus: Handle,
    subject: Subject,
) -> (Handle, Arc<AtomicUsize>) {
    let lost = Arc::new(AtomicUsize::new(0));
    (
        Handle::new(Dropping {
            bus,
            subject,
            lost: Arc::clone(&lost),
        }),
        lost,
    )
}

#[async_trait]
impl Bus for Dropping {
    async fn publish(&self, subject: &Subject, payload: &str) -> afd_bus::Result<()> {
        if subject == &self.subject {
            self.lost.fetch_add(1, Ordering::SeqCst);
            return Ok(());
        }
        self.bus.publish(subject, payload).await
    }

    async fn publish_batch(&self, subject: &Subject, payloads: &[String]) -> afd_bus::Result<()> {
        for payload in payloads {
            self.publish(subject, payload).await?;
        }
        Ok(())
    }

    async fn subscribe(&self, subject: &Subject) -> afd_bus::Result<Subscription> {
        self.bus.subscribe(subject).await
    }

    async fn subscribe_thread_wakes(&self) -> afd_bus::Result<Subscription> {
        self.bus.subscribe_thread_wakes().await
    }

    async fn shutdown(&self) -> afd_bus::Result<()> {
        self.bus.shutdown().await
    }
}
