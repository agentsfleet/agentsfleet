//! The real protocol client retries a malformed pending entry after acknowledgement fails.
#![expect(clippy::expect_used, reason = "test fixture failures must fail loudly")]

use crate::fake_redis::{FakeRedis, Reply};
use afd_core::{error_code, test_util::trace::Capture};
use afd_dragonfly::{
    Dedicated, OutboundReader,
    config::{DragonflyConfig, DragonflyRole},
};
use std::time::Duration;

const MALFORMED: &str =
    "*1\r\n*2\r\n$6\r\noutbox\r\n*1\r\n*2\r\n$3\r\n1-0\r\n*2\r\n$8\r\nprovider\r\n$5\r\nslack\r\n";

#[tokio::test]
async fn a_failed_malformed_entry_acknowledgement_retains_the_actual_registry_code_and_retries() {
    let capture = Capture::install();
    let (server, connection) = fault_peer().await;
    let mut reader = OutboundReader::new(connection, "fixture-consumer".into());
    assert!(
        reader
            .read_pending()
            .await
            .expect("malformed input is withheld")
            .is_none()
    );
    let failed = capture.only("outbound_entry_undecodable_drop_failed");
    assert_eq!(failed.field("entry_id"), Some("1-0"));
    assert_eq!(
        failed.field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert_eq!(failed.level, tracing::Level::WARN);
    server.set_reply("XACK", Reply::Raw(":1\r\n"));
    assert!(
        reader
            .read_pending()
            .await
            .expect("same entry can be retried")
            .is_none()
    );
    assert_eq!(
        capture
            .only("outbound_entry_undecodable_dropped")
            .field("entry_id"),
        Some("1-0")
    );
    assert_eq!(
        server
            .seen()
            .iter()
            .filter(|command| *command == "XACK")
            .count(),
        2
    );
    server.set_reply("XREADGROUP", Reply::Raw("*0\r\n"));
    assert!(
        reader
            .read_pending()
            .await
            .expect("queue remains readable after recovery")
            .is_none()
    );
}

async fn fault_peer() -> (FakeRedis, Dedicated) {
    let server = FakeRedis::spawn(&[
        ("XREADGROUP", Reply::Raw(MALFORMED)),
        (
            "XACK",
            Reply::Raw("-OOM fixture acknowledgement refused\r\n"),
        ),
    ])
    .await;
    let config = DragonflyConfig::from_url(DragonflyRole::Default, server.url());
    let connection = Dedicated::connect(&config, Duration::from_millis(10))
        .await
        .expect("fault peer");
    (server, connection)
}
