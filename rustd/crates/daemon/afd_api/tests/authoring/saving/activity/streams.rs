//! Probe subscription readiness and use ordered markers to detect extra notices.

use super::fixture::Saved;
use afd_core::{
    env::{EnvSource as _, ProcessEnv},
    id::Uuid7,
};
use afd_dragonfly::{
    Subscription, SubscriptionHub,
    config::{DragonflyConfig, DragonflyRole},
    hub::Received,
    streams::{FleetStreams, fleet_activity_channel},
};
use serde_json::{Value, json};
use std::time::Duration;

const DELIVERY_BUDGET: Duration = Duration::from_secs(5);
const PROBE: &str = "receipt-subscription-ready";
const FINISHED: &str = "receipt-writes-finished";

pub(super) struct Reader {
    hub: SubscriptionHub,
    subscription: Subscription,
    publisher: FleetStreams,
    fleet: String,
}

impl Reader {
    pub(super) async fn open(saved: &Saved, fleet: &Uuid7) -> Self {
        let config = DragonflyConfig::from_url(
            DragonflyRole::Default,
            ProcessEnv
                .get("TEST_DRAGONFLY_URL")
                .expect("live cluster URL"),
        );
        let hub = SubscriptionHub::start(config).await.expect("live hub");
        let reader = hub.subscribe(&fleet_activity_channel(fleet.as_str()));
        let mut stream = Self {
            hub,
            subscription: reader,
            publisher: FleetStreams::new(saved.case.queue.clone()),
            fleet: fleet.to_string(),
        };
        tokio::time::timeout(DELIVERY_BUDGET, async {
            while stream
                .publisher
                .publish_tail(&stream.fleet, PROBE)
                .await
                .expect("subscription probe")
                == 0
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("subscription confirmed before mutation");
        assert_eq!(stream.receive().await, PROBE);
        stream
    }

    pub(super) async fn changed(&mut self) {
        let notice: Value = serde_json::from_str(&self.receive().await).expect("safe notice");
        assert_eq!(notice, json!({"kind":"catching_up", "dropped":0}));
    }

    pub(super) async fn quiet(&mut self) {
        self.publisher
            .publish_tail(&self.fleet, FINISHED)
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
        message.payload.clone()
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.hub.shutdown();
    }
}
