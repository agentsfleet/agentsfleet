//! The lane's NATS cluster, one dev namespace per test, and patient reads.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use afd_bus::{Handle, Received, Subject, Subscription, nats::Tuning};

pub(crate) use afd_bus::test_util::{Proxy, port_of, via};

pub(crate) type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
pub(crate) type Fallible<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// How long a read waits before the test fails.
pub(crate) const PATIENCE: Duration = Duration::from_secs(15);

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// The three node seeds `make` exports, one per node.
pub(crate) fn seeds() -> Fallible<Vec<String>> {
    Ok(afd_bus::test_util::lane_seeds()?)
}

/// A namespace no other test shares, so parallel tests never cross subjects.
pub(crate) fn namespace() -> String {
    format!(
        "t{}_{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// Connect to the given seeds under this test's `namespace`.
pub(crate) async fn bus(seeds: &str, namespace: &str, tuning: Tuning) -> Fallible<Handle> {
    Ok(afd_bus::test_util::dial(seeds, Some(namespace), tuning).await?)
}

/// The next delivery, or a failed test.
pub(crate) async fn next(subscription: &mut Subscription) -> Fallible<Received> {
    Ok(tokio::time::timeout(PATIENCE, subscription.recv()).await??)
}

/// Publish probes until every subscription has seen one, so the server's
/// interest has reached the publisher's node. Probes are drained.
pub(crate) async fn settle(
    publisher: &Handle,
    subject: &Subject,
    subscriptions: &mut [&mut Subscription],
) -> TestResult {
    for subscription in subscriptions.iter_mut() {
        let deadline = tokio::time::Instant::now() + PATIENCE;
        loop {
            publisher.publish(subject, "probe").await?;
            let got = tokio::time::timeout(Duration::from_millis(100), subscription.recv()).await;
            if matches!(got, Ok(Ok(Received::Message(_)))) {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                return Err("interest never reached the publisher".into());
            }
        }
    }
    for subscription in subscriptions.iter_mut() {
        while let Ok(Ok(_)) =
            tokio::time::timeout(Duration::from_millis(200), subscription.recv()).await
        {}
    }
    Ok(())
}

/// Read until `payload` arrives, returning what came before it.
pub(crate) async fn until(
    publisher: &Handle,
    subject: &Subject,
    subscription: &mut Subscription,
    payload: &str,
) -> Fallible<Vec<Received>> {
    let deadline = tokio::time::Instant::now() + PATIENCE;
    let mut before = Vec::new();
    loop {
        // A publish during an outage may fail; the loop retries until it lands.
        let _ = publisher.publish(subject, payload).await;
        while let Ok(Ok(received)) =
            tokio::time::timeout(Duration::from_millis(100), subscription.recv()).await
        {
            if let Received::Message(message) = &received
                && message.payload.as_str() == payload
            {
                return Ok(before);
            }
            before.push(received);
        }
        if tokio::time::Instant::now() > deadline {
            return Err(format!("{payload} never arrived; saw {before:?}").into());
        }
    }
}
