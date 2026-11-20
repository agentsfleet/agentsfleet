//! Native progress and a Bus subscriber retain the same durable run across reconnects.

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use afd_bus::{Handle, Received, Subject, Subscription};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use std::time::Duration;

use crate::e2e::{Scenario, scenario};
use crate::integration_runner_stream::raw::Native;
use crate::tail::{FRAME_DEADLINE, SILENCE_WINDOW, run, settle};

pub(crate) struct Tailed {
    pub(crate) scene: Scenario,
    pub(crate) run: String,
    pub(crate) fence: u64,
    native: Native,
    bus: Handle,
    tail: Subscription,
}

impl Tailed {
    pub(crate) async fn open(supervisor: &mut Supervisor) -> Self {
        let mut scene = scenario(supervisor).await;
        let (run, fence) = run(&scene).await;
        scene
            .native
            .get_mut()
            .expect("run arrived through the native runner")
            .close()
            .await
            .expect("the issuing session stops before reconnect");
        let native = Native::connect(&scene, Transport::H2).await;
        let bus = afd_bus::test_util::lane()
            .await
            .expect("the lane's NATS: scene through make test-integration-rustd");
        let tail = bus
            .subscribe(&Subject::ThreadItems(scene.fleet.clone()))
            .await
            .expect("subscribe before forwarding activity");
        settle().await;
        Self {
            scene,
            run,
            fence,
            native,
            bus,
            tail,
        }
    }

    /// Activity has no reply; publication and refusal are observed independently.
    pub(crate) async fn forward(&mut self, request: &Value) {
        let value = json!({"type":"progress", "run_id":self.run, "request":request});
        self.native.send(StreamKind::Progress, &value).await;
    }

    pub(crate) async fn forward_for(&mut self, run: &str, request: &Value) {
        self.native
            .send(
                StreamKind::Progress,
                &json!({"type":"progress", "run_id":run, "request":request}),
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
            scene,
            run,
            fence,
            native,
            bus,
            tail,
        } = self;
        native.close().await;
        let native = Native::connect(&scene, Transport::H2).await;
        let mut fixture = Self {
            scene,
            run,
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
                &json!({"type":"renew_batch", "request_id":4, "runs":[{
                    "run_id":self.run, "request":{"requests":[]}
                }]}),
            )
            .await;
        let response = self.native.expect(StreamKind::Control, "renewed").await;
        assert_eq!((*crate::wire::pointer(&response, "/request_id")), 4);
        assert_eq!(
            (*crate::wire::pointer(&response, "/runs/0/run_id")),
            self.run
        );
        assert_eq!(
            (*crate::wire::pointer(&response, "/runs/0/answer/status")),
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
        self.scene.cleanup().await;
    }
}
