//! Only the selected runner's commands are discarded; every other signal uses the real Bus.

use af_core::id::Uuid7;
use afd_bus::{Bus, Handle, Subject, Subscription};
use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
struct LostCommands {
    bus: Handle,
    runner: Uuid7,
    lost: Arc<AtomicUsize>,
}

pub(super) fn dropping(bus: Handle, runner: Uuid7) -> (Handle, Arc<AtomicUsize>) {
    let lost = Arc::new(AtomicUsize::new(0));
    (
        Handle::new(LostCommands {
            bus,
            runner,
            lost: Arc::clone(&lost),
        }),
        lost,
    )
}

#[async_trait]
impl Bus for LostCommands {
    async fn publish(&self, subject: &Subject, payload: &str) -> afd_bus::Result<()> {
        if matches!(subject, Subject::RunnerCommand(runner) if runner == &self.runner) {
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

    async fn shutdown(&self) -> afd_bus::Result<()> {
        self.bus.shutdown().await
    }
}
