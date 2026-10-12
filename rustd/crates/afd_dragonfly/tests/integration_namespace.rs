//! Equal logical names must remain independent on one real cluster.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions must fail loudly"
)]

use crate::support::DragonflyHarness;
use afd_dragonfly::hub::{Received, Subscription};
use afd_dragonfly::session::{AbortReason, SessionState};
use afd_dragonfly::{
    Dedicated, Dragonfly, DragonflyConfig, FleetStreams, OutboundJob, OutboundQueue,
    OutboundReader, ReadyIndex, SessionStore, SubscriptionHub,
};
use std::time::Duration;

const VALUE_TTL_SECONDS: u32 = 60;

async fn pair() -> (DragonflyConfig, Dragonfly, DragonflyConfig, Dragonfly) {
    let harness = DragonflyHarness::connect().await;
    let a = DragonflyHarness::config()
        .with_namespace(&harness.name("a"))
        .expect("namespace a");
    let b = DragonflyHarness::config()
        .with_namespace(&harness.name("ab"))
        .expect("namespace b");
    let ra = Dragonfly::connect(&a).await.expect("connect a");
    let rb = Dragonfly::connect(&b).await.expect("connect b");
    (a, ra, b, rb)
}

fn session() -> SessionState {
    serde_json::from_value(serde_json::json!({
        "session_id": "equal-id", "status": "verification_pending",
        "cli_public_key": "cli", "token_name": "local", "dashboard_public_key": "dashboard",
        "ciphertext": "ciphertext", "nonce": "nonce", "verification_code_hmac_hex": "digest",
        "verification_attempts": 0, "created_at_ms": 1_700_000_000_000_i64,
        "expires_at_ms": 1_700_000_300_000_i64, "approved_at_ms": 1_700_000_010_000_i64,
        "clerk_user_id": "equal-owner"
    }))
    .expect("complete session fixture")
}

#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn namespaces_isolate_values_sessions_and_scans() {
    let (_, a, _, b) = pair().await;
    a.set_for("same-key", "a", i64::from(VALUE_TTL_SECONDS))
        .await
        .expect("write a");
    b.set_for("same-key", "b", i64::from(VALUE_TTL_SECONDS))
        .await
        .expect("write b");
    assert_eq!(
        a.get_string("same-key").await.expect("read a").as_deref(),
        Some("a")
    );
    assert_eq!(
        b.get_string("same-key").await.expect("read b").as_deref(),
        Some("b")
    );
    assert!(a.spend_key("same-key").await.expect("spend a"));
    assert_eq!(
        b.get_string("same-key")
            .await
            .expect("b survives")
            .as_deref(),
        Some("b")
    );
    assert_eq!(
        a.increment_in_window("counter", VALUE_TTL_SECONDS)
            .await
            .expect("counter a"),
        1
    );
    assert_eq!(
        b.increment_in_window("counter", VALUE_TTL_SECONDS)
            .await
            .expect("counter b"),
        1
    );
    b.set_for("foreign-only", "b", i64::from(VALUE_TTL_SECONDS))
        .await
        .expect("foreign write");
    assert_eq!(
        a.scan_keys("*", 1).await.expect("scoped scan"),
        vec!["counter"]
    );
    assert_session_isolation(a, b).await;
}

async fn assert_session_isolation(a: Dragonfly, b: Dragonfly) {
    let sa = SessionStore::new(a);
    let sb = SessionStore::new(b);
    sa.put(&session()).await.expect("session a");
    sb.put(&session()).await.expect("session b");
    assert_eq!(
        sa.abort_all_for_owner("equal-owner", AbortReason::ExplicitCancel)
            .await
            .expect("scoped abort"),
        vec!["equal-id"]
    );
    assert_eq!(
        sb.get("equal-id")
            .await
            .expect("foreign session")
            .expect("exists"),
        session()
    );
}

#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn namespaces_isolate_streams_readiness_and_outbound() {
    let (ca, a, cb, b) = pair().await;
    let fa = FleetStreams::new(a.clone());
    let fb = FleetStreams::new(b.clone());
    fa.ensure_group("same-fleet").await.expect("group a");
    fb.ensure_group("same-fleet").await.expect("group b");
    let id = fa
        .append("same-fleet", &[("type", "a")])
        .await
        .expect("append a");
    assert!(
        fb.read_new("same-fleet", "same-consumer")
            .await
            .expect("read b")
            .is_none()
    );
    assert_eq!(
        fa.read_new("same-fleet", "same-consumer")
            .await
            .expect("read a")
            .expect("event")
            .receipt,
        id
    );
    assert!(!fb.ack("same-fleet", &id).await.expect("foreign ack"));
    assert!(fa.ack("same-fleet", &id).await.expect("own ack"));
    assert_eq!(
        afd_dragonfly::test_util::fleet_entries(&a, "same-fleet")
            .await
            .expect("scoped history a")
            .len(),
        1
    );
    assert_eq!(
        afd_dragonfly::test_util::fleet_entries(&b, "same-fleet")
            .await
            .expect("scoped history b"),
        [] as [afd_dragonfly::FleetEvent; 0]
    );
    let ia = ReadyIndex::new(a.clone());
    let ib = ReadyIndex::new(b.clone());
    let token = ia.mark("same-fleet").await.expect("mark a");
    assert!(
        ib.token_for("same-fleet")
            .await
            .expect("b unmarked")
            .is_none()
    );
    ib.force_clear("same-fleet").await.expect("clear b");
    assert_eq!(
        ia.token_for("same-fleet").await.expect("a survives"),
        Some(token)
    );
    outbound_isolation(ca, a, cb, b).await;
}

#[path = "integration_namespace/sweep.rs"]
mod sweep;

async fn outbound_isolation(ca: DragonflyConfig, a: Dragonfly, cb: DragonflyConfig, b: Dragonfly) {
    let qa = OutboundQueue::new(a);
    let qb = OutboundQueue::new(b);
    qa.ensure_group().await.expect("outbound group a");
    qb.ensure_group().await.expect("outbound group b");
    let id = qa
        .enqueue(OutboundJob {
            provider: "test",
            destination: "test",
            workspace_id: "same",
            fleet_id: "same-fleet",
            event_id: "same-event",
            answer: "only-a",
        })
        .await
        .expect("enqueue a");
    let mut ra = OutboundReader::new(
        Dedicated::connect(&ca, Duration::from_millis(10))
            .await
            .expect("dedicated a"),
        "same-consumer".into(),
    );
    let mut rb = OutboundReader::new(
        Dedicated::connect(&cb, Duration::from_millis(10))
            .await
            .expect("dedicated b"),
        "same-consumer".into(),
    );
    assert!(rb.read_blocking(10).await.expect("b empty").is_none());
    assert_eq!(
        ra.read_blocking(10)
            .await
            .expect("a read")
            .expect("delivery")
            .id,
        id
    );
    assert_eq!(
        ra.read_pending()
            .await
            .expect("resume a")
            .expect("pending")
            .id,
        id
    );
    assert!(rb.read_pending().await.expect("resume b").is_none());
    assert!(!qb.ack(&id).await.expect("b cannot ack a"));
    assert!(qa.ack(&id).await.expect("a ack"));
}

async fn deliver(publisher: &FleetStreams, subscription: &mut Subscription, payload: &str) {
    let channel = subscription.channel().to_owned();
    tokio::time::timeout(Duration::from_secs(5), async {
        while publisher.publish(&channel, payload).await.expect("publish") == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let Received::Message(message) = subscription.recv().await.expect("receive") else {
            panic!("expected a message");
        };
        assert_eq!(message.channel, channel);
        assert_eq!(message.payload, payload);
    })
    .await
    .expect("subscription confirms and receives within budget");
}

#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn namespaces_isolate_subscriptions() {
    let (ca, a, cb, b) = pair().await;
    let ha = SubscriptionHub::start(ca.clone()).await.expect("hub a");
    let hb = SubscriptionHub::start(cb).await.expect("hub b");
    let mut sa = ha.subscribe("equal-channel");
    let mut sb = hb.subscribe("equal-channel");
    let pa = FleetStreams::new(a);
    let pb = FleetStreams::new(b);
    deliver(&pa, &mut sa, "a").await;
    tokio::time::timeout(Duration::from_millis(100), sb.recv())
        .await
        .expect_err("namespace b must not receive a");
    deliver(&pb, &mut sb, "b").await;
    tokio::time::timeout(Duration::from_millis(100), sa.recv())
        .await
        .expect_err("namespace a must not receive b");
    assert_eq!(ha.readers("equal-channel"), 1);
    drop(sa);
    assert_eq!(ha.readers("equal-channel"), 0);
    let restarted = SubscriptionHub::start(ca).await.expect("restart a");
    let mut sa = restarted.subscribe("equal-channel");
    deliver(&pa, &mut sa, "after restart").await;
    tokio::time::timeout(Duration::from_millis(100), sb.recv())
        .await
        .expect_err("namespace b must not receive restarted a");
    restarted.shutdown();
    hb.shutdown();
}

// The existing exclusive filter gives socket identification the cluster alone.
mod integration_hub_exclusive {
    use super::{Duration, FleetStreams, SubscriptionHub, deliver, pair};
    use crate::hub_exclusive::{
        clients_across, deliver as deliver_recovered, difference, gap_on, kill_each, nodes_of,
    };
    use crate::support::DragonflyHarness;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "needs live Dragonfly: make test-integration-rustd"]
    async fn namespaced_subscriptions_survive_a_server_disconnect() {
        let harness = DragonflyHarness::connect().await;
        let (ca, a, cb, b) = pair().await;
        let pa = FleetStreams::new(a);
        let pb = FleetStreams::new(b);
        let nodes = nodes_of(&harness).await;
        let hb = SubscriptionHub::start(cb).await.expect("hub b");
        let mut sb = hb.subscribe("equal-channel");
        deliver(&pb, &mut sb, "b before disconnect").await;
        // Dial the publisher before the snapshot so only hub sockets are killed.
        pa.publish("equal-channel", "warm")
            .await
            .expect("publisher a is warm");
        let before = clients_across(&nodes).await;
        let ha = SubscriptionHub::start(ca).await.expect("hub a");
        let mut sa = ha.subscribe("equal-channel");
        deliver(&pa, &mut sa, "a before disconnect").await;
        let victims = difference(&clients_across(&nodes).await, &before);
        assert!(!victims.is_empty(), "a opened identifiable sockets");
        kill_each(&nodes, &victims).await;
        gap_on(&mut sa).await;
        // Multiple lost sockets can produce more than one recovery notice.
        deliver_recovered(&pa, "equal-channel", "a after disconnect", &mut sa).await;
        tokio::time::timeout(Duration::from_millis(100), sb.recv())
            .await
            .expect_err("b cannot receive a after recovery");
        deliver(&pb, &mut sb, "b after sibling recovery").await;
        ha.shutdown();
        hb.shutdown();
    }
}
