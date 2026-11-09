//! Native progress and a Bus subscriber retain the same durable lease across reconnects.

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use afd_bus::{Handle, Received, Subject, Subscription};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use std::time::Duration;

use crate::e2e::{Scenario, dragonfly_config, scenario};
use crate::integration_runner_stream::raw::Native;
use crate::tail::{FRAME_DEADLINE, SILENCE_WINDOW, lease, settle};

pub(crate) struct Tailed {
    pub(crate) run: Scenario,
    lease: String,
    pub(crate) fence: u64,
    native: Native,
    bus: Handle,
    tail: Subscription,
}

impl Tailed {
    pub(crate) async fn open(supervisor: &mut Supervisor) -> Self {
        let mut run = scenario(supervisor).await;
        let (lease, fence) = lease(&run).await;
        run.native
            .get_mut()
            .expect("lease arrived through the native runner")
            .close()
            .await
            .expect("the issuing session stops before reconnect");
        let native = Native::connect(&run, Transport::H2).await;
        let bus = afd_bus::dragonfly::connect(run.booted.queue.clone(), dragonfly_config())
            .await
            .expect("the production Bus subscriber opens");
        let tail = bus
            .subscribe(&Subject::FleetTail(run.fleet.clone()))
            .await
            .expect("subscribe before forwarding activity");
        settle().await;
        Self {
            run,
            lease,
            fence,
            native,
            bus,
            tail,
        }
    }

    /// Activity has no reply; publication and refusal are observed independently.
    pub(crate) async fn forward(&mut self, request: &Value) {
        let value = json!({"type":"activity", "lease_id":self.lease, "request":request});
        self.native.send(StreamKind::Activity, &value).await;
    }

    pub(crate) async fn forward_for(&mut self, lease: &str, request: &Value) {
        self.native
            .send(
                StreamKind::Activity,
                &json!({"type":"activity", "lease_id":lease, "request":request}),
            )
            .await;
    }

    /// Invalid native input closes its owner before any frame reaches the subscriber.
    pub(crate) async fn refused(mut self, request: &Value) -> Self {
        self.forward(request).await;
        self.native.ended(STREAM_ANSWER_TIMEOUT).await;
        assert_eq!(
            self.silence().await,
            None,
            "a refused batch published nothing"
        );
        self.reconnect().await
    }

    async fn reconnect(self) -> Self {
        let Self {
            run,
            lease,
            fence,
            native,
            bus,
            tail,
        } = self;
        native.close().await;
        let native = Native::connect(&run, Transport::H2).await;
        let mut fixture = Self {
            run,
            lease,
            fence,
            native,
            bus,
            tail,
        };
        fixture.renew().await;
        fixture
    }

    async fn renew(&mut self) {
        self.native
            .send(
                StreamKind::Control,
                &json!({"type":"renew_batch", "request_id":4, "leases":[{
                    "lease_id":self.lease, "request":{"requests":[]}
                }]}),
            )
            .await;
        let response = self.native.expect(StreamKind::Control, "renewed").await;
        assert_eq!((*crate::wire::pointer(&response, "/request_id")), 4);
        assert_eq!(
            (*crate::wire::pointer(&response, "/leases/0/lease_id")),
            self.lease
        );
        assert_eq!(
            (*crate::wire::pointer(&response, "/leases/0/answer/status")),
            "ok"
        );
    }

    pub(crate) async fn next_frame(&mut self) -> Option<Value> {
        self.received(FRAME_DEADLINE).await
    }

    pub(crate) async fn silence(&mut self) -> Option<Value> {
        self.received(SILENCE_WINDOW).await
    }

    async fn received(&mut self, budget: Duration) -> Option<Value> {
        let received = match tokio::time::timeout(budget, self.tail.recv()).await {
            Ok(received) => received.expect("the Bus subscription remains live"),
            Err(_elapsed) => return None,
        };
        let Received::Message(message) = received else {
            panic!("a Bus gap or lag cannot prove publication or absence: {received:?}")
        };
        Some(serde_json::from_str(&message.payload).expect("a published tail frame is JSON"))
    }

    pub(crate) async fn finish(self, supervisor: Supervisor) {
        self.native.close().await;
        drop(self.tail);
        self.bus
            .shutdown()
            .await
            .expect("the subscriber owner joins");
        supervisor.shutdown().await;
        self.run.cleanup().await;
    }
}
