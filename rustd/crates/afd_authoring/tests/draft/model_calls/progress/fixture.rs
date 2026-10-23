//! Real SDK bytes and the real private bus; only the remote model is simulated.

use afd_authoring::{
    ModelCallInput, ModelCallResult, ModelCalls, ProviderRequest, ProviderResponse, Transport,
};
use afd_core::env::{EnvSource as _, ProcessEnv};
use afd_core::id::Uuid7;
use afd_dragonfly::{
    FleetStreams, Subscription, SubscriptionHub,
    config::{DragonflyConfig, DragonflyRole},
    hub::Received,
};
use bytes::Bytes;
use futures_util::stream;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Semaphore, mpsc};

use crate::draft::{attempt::Attempt, fixture::Fixture};

pub(super) const WAIT: Duration = Duration::from_secs(10);
const HELLO: &str = "subscribed";
const BARRIER: &str = "drained";

#[derive(Debug)]
pub(super) struct Provider {
    receiver: Mutex<Option<mpsc::Receiver<Bytes>>>,
    entered: Semaphore,
    pub(super) calls: AtomicUsize,
}

#[async_trait::async_trait]
impl Transport for Provider {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let body: Value = serde_json::from_slice(request.body()).expect("native SDK request");
        assert_eq!(body.get("stream"), Some(&Value::Bool(true)));
        self.calls.fetch_add(1, Ordering::SeqCst);
        let receiver = self
            .receiver
            .lock()
            .expect("fixture receiver")
            .take()
            .expect("one physical send");
        self.entered.add_permits(1);
        let body = stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|bytes| (Ok(bytes), receiver))
        });
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );
        Ok(ProviderResponse {
            status: 200,
            headers,
            body: Box::pin(body),
        })
    }
}

pub(super) struct Live {
    pub(super) provider: Arc<Provider>,
    pub(super) sender: mpsc::Sender<Bytes>,
    pub(super) calls: ModelCalls,
    pub(super) hub: SubscriptionHub,
    pub(super) reader: Subscription,
    publisher: FleetStreams,
    channel: String,
}

impl Live {
    pub(super) fn call(
        &self,
        runner: &Uuid7,
        attempt: &Attempt,
        request: ModelCallInput,
    ) -> tokio::task::JoinHandle<afd_authoring::Result<ModelCallResult>> {
        let (calls, runner, lease) = (self.calls.clone(), runner.clone(), attempt.lease.clone());
        tokio::spawn(async move { calls.call(&runner, &lease, request).await })
    }

    pub(super) async fn start(f: &Fixture, attempt: &Attempt) -> Self {
        let url = ProcessEnv
            .get("TEST_DRAGONFLY_URL")
            .expect("Make integration lane");
        let config = DragonflyConfig::from_url(DragonflyRole::Default, url);
        let redis = afd_dragonfly::test_util::connect_live(&config)
            .await
            .expect("live bus");
        let publisher = FleetStreams::new(redis);
        let hub = SubscriptionHub::start(config)
            .await
            .expect("live subscriptions");
        let channel = afd_authoring::draft_channel(&attempt.draft.id);
        let reader = hub.subscribe(&channel);
        let (sender, receiver) = mpsc::channel(8);
        let provider = Arc::new(Provider {
            receiver: Mutex::new(Some(receiver)),
            entered: Semaphore::new(0),
            calls: AtomicUsize::new(0),
        });
        let calls = super::super::fixture::calls(f, attempt, Arc::clone(&provider))
            .await
            .with_progress(publisher.clone());
        let mut live = Self {
            provider,
            sender,
            calls,
            hub,
            reader,
            publisher,
            channel,
        };
        tokio::time::timeout(WAIT, live.ready())
            .await
            .expect("subscription handshake");
        live
    }

    async fn ready(&mut self) {
        loop {
            self.publisher
                .publish(&self.channel, HELLO)
                .await
                .expect("subscribe probe");
            if let Ok(Ok(Received::Message(message))) =
                tokio::time::timeout(Duration::from_millis(100), self.reader.recv()).await
            {
                assert_eq!(message.payload, HELLO);
                return;
            }
        }
    }

    pub(super) async fn arrived(&self) {
        tokio::time::timeout(WAIT, self.provider.entered.acquire())
            .await
            .expect("dispatch entered")
            .expect("fixture live")
            .forget();
        self.frame(json!({"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_1","role":"assistant","status":"in_progress","content":[]}})).await;
        self.frame(json!({"type":"response.content_part.added","item_id":"msg_1","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}})).await;
    }

    pub(super) async fn text(&self, text: &str) {
        self.frame(json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":text})).await;
    }

    pub(super) async fn frame(&self, document: Value) {
        self.sender
            .send(Bytes::from(format!("data: {document}\n\n")))
            .await
            .expect("reader is live");
    }

    pub(super) async fn finish(&self) {
        let response = super::super::fixture::Provider::known().response;
        self.frame(json!({"type":"response.completed","response":response}))
            .await;
    }

    pub(super) async fn next(&mut self) -> afd_authoring::Delta {
        tokio::time::timeout(WAIT, async {
            loop {
                let message = message(self.reader.recv().await.expect("bus stays live"));
                if message.payload == HELLO {
                    continue;
                }
                return serde_json::from_str(&message.payload).expect("private frame schema");
            }
        })
        .await
        .expect("incremental text before completion")
    }

    pub(super) async fn drained(&mut self) {
        self.publisher
            .publish(&self.channel, BARRIER)
            .await
            .expect("same-slot barrier");
        let received = tokio::time::timeout(WAIT, self.reader.recv())
            .await
            .expect("bus drain")
            .expect("live bus");
        let message = message(received);
        assert_eq!(
            message.payload, BARRIER,
            "no unauthorized text before the bus barrier"
        );
    }
}

fn message(received: Received) -> Arc<afd_dragonfly::hub::Message> {
    match received {
        Received::Message(message) => Some(message),
        Received::Gap | Received::Lagged(_) => None,
    }
    .expect("private bus must not lose a frame")
}
