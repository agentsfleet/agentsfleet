//! Inspect physical state to catch a store that forgets namespace mapping.

use super::{
    Dragonfly, DragonflyHarness, Duration, FleetStreams, OutboundJob, OutboundQueue, SessionStore,
    SubscriptionHub, deliver, session,
};
use afd_dragonfly::session::{AbortOutcome, AbortReason, Approval, ApproveOutcome, SessionStatus};
use afd_dragonfly::{Dedicated, OUTBOUND_STREAM_KEY, OutboundReader};
use std::collections::BTreeSet;

const KEY_TTL_SECONDS: u32 = 60;

#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn namespaced_surface_writes_nothing_outside_its_prefix() {
    let harness = DragonflyHarness::connect().await;
    let marker = harness.name("sweep");
    let config = DragonflyHarness::config()
        .with_namespace(&marker)
        .expect("namespace");
    let redis = Dragonfly::connect(&config)
        .await
        .expect("scoped connection");
    let payload = "dev:foreign:STREAMS {tag} \0 opaque";
    write_values_and_session(&redis, &marker, payload).await;
    let streams = FleetStreams::new(redis.clone());
    write_outbound(&config, &redis, &harness.redis, &marker, payload).await;
    let hub = SubscriptionHub::start(config).await.expect("hub");
    let mut subscription = hub.subscribe(&marker);
    deliver(&streams, &mut subscription, payload).await;
    let expected = [
        marker.clone(),
        format!("{marker}_counter"),
        afd_dragonfly::session::session_key(&marker),
        OUTBOUND_STREAM_KEY.to_owned(),
    ]
    .into_iter()
    .map(|key| redis.physical_key(&key))
    .collect::<BTreeSet<_>>();
    let pattern = format!("*{marker}*");
    let actual = harness
        .redis
        .scan_keys(&pattern, 1)
        .await
        .expect("every primary");
    assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), expected);
    let channels = afd_dragonfly::test_util::shard_channels(&harness.redis, &pattern)
        .await
        .expect("inspect every primary");
    assert!(!channels.is_empty(), "confirmed subscription is visible");
    assert!(
        channels
            .iter()
            .all(|channel| channel == &redis.physical_key(&marker))
    );
    drop(subscription);
    hub.shutdown();
    for key in expected {
        harness
            .redis
            .spend_key(&key)
            .await
            .expect("remove owned state");
    }
}

async fn write_values_and_session(redis: &Dragonfly, marker: &str, payload: &str) {
    redis
        .set_for(marker, payload, i64::from(KEY_TTL_SECONDS))
        .await
        .expect("value");
    assert_eq!(
        redis
            .get_string(marker)
            .await
            .expect("read value")
            .as_deref(),
        Some(payload)
    );
    redis
        .increment_in_window(&format!("{marker}_counter"), KEY_TTL_SECONDS)
        .await
        .expect("counter");
    let store = SessionStore::new(redis.clone());
    let mut state = session();
    state.session_id = marker.to_owned();
    state.status = SessionStatus::Pending;
    store.put(&state).await.expect("session");
    let approval = Approval {
        session_id: marker,
        dashboard_public_key: payload,
        ciphertext: payload,
        nonce: payload,
        code_hmac_hex: "digest",
        approver: marker,
    };
    assert_eq!(
        store
            .approve(&approval, state.created_at)
            .await
            .expect("approve"),
        ApproveOutcome::Approved
    );
    assert_eq!(
        store
            .abort(marker, marker, AbortReason::ExplicitCancel)
            .await
            .expect("abort"),
        AbortOutcome::Aborted
    );
}

async fn write_outbound(
    config: &afd_dragonfly::DragonflyConfig,
    redis: &Dragonfly,
    default: &Dragonfly,
    marker: &str,
    payload: &str,
) {
    let queue = OutboundQueue::new(redis.clone());
    queue.ensure_group().await.expect("outbound group");
    let id = queue
        .enqueue(OutboundJob {
            provider: marker,
            destination: payload,
            workspace_id: marker,
            fleet_id: marker,
            event_id: marker,
            answer: payload,
        })
        .await
        .expect("enqueue");
    // The fixed logical key cannot carry the marker, so inspect its entries too.
    let mut cmd = redis::cmd("XRANGE");
    cmd.arg(OUTBOUND_STREAM_KEY).arg("-").arg("+");
    let unscoped: redis::streams::StreamRangeReply = default
        .command("XRANGE", marker, &cmd)
        .await
        .expect("inspect unscoped outbound");
    assert!(
        unscoped
            .ids
            .iter()
            .all(|entry| entry.get::<String>("fleet_id").as_deref() != Some(marker))
    );
    let connection = Dedicated::connect(config, Duration::from_millis(10))
        .await
        .expect("reader");
    assert_eq!(
        connection.physical_key(OUTBOUND_STREAM_KEY),
        redis.physical_key(OUTBOUND_STREAM_KEY)
    );
    let mut reader = OutboundReader::new(connection, marker.to_owned());
    let delivery = reader
        .read_blocking(std::time::Duration::from_millis(10))
        .await
        .expect("read outbound")
        .expect("delivery");
    assert_eq!(delivery.id, id);
    assert_eq!(delivery.answer, payload);
    assert_eq!(delivery.destination, payload);
    assert!(queue.ack(&id).await.expect("ack outbound"));
    queue.trim().await.expect("trim outbound");
}

#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn raw_commands_accept_publicly_mapped_physical_keys() {
    let harness = DragonflyHarness::connect().await;
    let logical = harness.name("raw");
    let config = DragonflyHarness::config()
        .with_namespace(&logical)
        .expect("namespace");
    let redis = Dragonfly::connect(&config).await.expect("connection");
    let physical = redis.physical_key(&logical);
    let mut cmd = redis::cmd("SET");
    cmd.arg(&physical)
        .arg("dev:opaque:STREAMS")
        .arg("EX")
        .arg(KEY_TTL_SECONDS);
    let _: String = redis
        .command("SET", &logical, &cmd)
        .await
        .expect("raw write");
    assert_eq!(
        redis
            .get_string(&logical)
            .await
            .expect("typed read")
            .as_deref(),
        Some("dev:opaque:STREAMS")
    );
    assert_eq!(
        harness
            .redis
            .get_string(&physical)
            .await
            .expect("physical read")
            .as_deref(),
        Some("dev:opaque:STREAMS")
    );
    assert!(
        harness
            .redis
            .get_string(&logical)
            .await
            .expect("unmapped name")
            .is_none()
    );
    let mut pipeline = redis::pipe();
    pipeline.cmd("GET").arg(&physical);
    let (value,): (String,) = redis
        .pipeline("GET", &logical, &pipeline)
        .await
        .expect("physical pipeline");
    assert_eq!(value, "dev:opaque:STREAMS");
    let script = redis::Script::new("return redis.call('GET', KEYS[1])");
    let mut invocation = script.prepare_invoke();
    invocation.key(&physical);
    let value: String = redis
        .script("EVAL", &logical, &invocation)
        .await
        .expect("physical script");
    assert_eq!(value, "dev:opaque:STREAMS");
    assert!(redis.spend_key(&logical).await.expect("cleanup"));
}
