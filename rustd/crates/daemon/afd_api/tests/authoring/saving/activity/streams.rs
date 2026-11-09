//! Use an ordered marker to detect extra notices.

use super::fixture::Saved;
use af_core::id::Uuid7;
use afd_bus::{Received, Subject, Subscription};
use serde_json::{Value, json};
use std::time::Duration;

const DELIVERY_BUDGET: Duration = Duration::from_secs(5);
const FINISHED: &str = "receipt-writes-finished";

pub(super) struct Reader {
    subscription: Subscription,
    bus: afd_bus::Handle,
    subject: Subject,
}

impl Reader {
    pub(super) async fn open(saved: &Saved, fleet: &Uuid7) -> Self {
        let subject = Subject::FleetTail(fleet.clone());
        let subscription = saved
            .case
            .bus
            .subscribe(&subject)
            .await
            .expect("the in-process bus subscribes");
        Self {
            subscription,
            bus: saved.case.bus.clone(),
            subject,
        }
    }

    pub(super) async fn changed(&mut self) {
        let notice: Value = serde_json::from_str(&self.receive().await).expect("safe notice");
        assert_eq!(notice, json!({"kind":"catching_up", "dropped":0}));
    }

    pub(super) async fn quiet(&mut self) {
        self.bus
            .publish(&self.subject, FINISHED)
            .await
            .expect("ordered end marker");
        assert_eq!(
            self.receive().await,
            FINISHED,
            "no unexpected notice before marker"
        );
    }

    async fn receive(&mut self) -> String {
        let received = tokio::time::timeout(DELIVERY_BUDGET, self.subscription.recv())
            .await
            .expect("notice within delivery budget")
            .expect("subscription remains open");
        let Received::Message(message) = received else {
            panic!("a published message, not a gap");
        };
        message.payload.as_ref().clone()
    }
}
