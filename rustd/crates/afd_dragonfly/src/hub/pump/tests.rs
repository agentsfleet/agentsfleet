//! What a redial says while the cluster stays away.

use std::time::Duration;

use afd_core::test_util::trace::Capture;
use backon::ExponentialBuilder;

use super::redial;
use crate::config::{DragonflyConfig, DragonflyRole};

/// A loopback port nobody listens on, so every dial is refused at once.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// Each failed redial logs the failure's sentence and causes; its code rides
/// the `error_code` field, so a `[UZ-…]` prefix in the reason would repeat it.
#[tokio::test]
async fn a_failed_redial_reason_carries_no_code_prefix() {
    let recorder = Capture::install();
    let config = DragonflyConfig::from_url(DragonflyRole::Default, NOWHERE.to_owned())
        .with_connect_timeout(Duration::from_millis(200));
    let schedule = ExponentialBuilder::new()
        .with_min_delay(Duration::from_millis(1))
        .with_max_delay(Duration::from_millis(5))
        .with_max_times(2);

    let redialled = tokio::time::timeout(Duration::from_secs(2), redial(&config, schedule)).await;
    assert!(
        redialled.ok().is_none(),
        "nothing listens, so the redial never ends"
    );

    let events = recorder.events();
    let failed: Vec<_> = events
        .iter()
        .filter(|record| {
            record.fields.get("event").map(String::as_str) == Some("hub_reconnect_failed")
        })
        .collect();
    assert!(!failed.is_empty(), "a failed redial is logged");
    for record in failed {
        let reason = record.fields.get("reason").map_or("", String::as_str);
        assert!(!reason.is_empty() && !reason.contains("[UZ-"), "{reason}");
        assert_eq!(
            record.fields.get("cause_error_code").map(String::as_str),
            Some(afd_core::error_code::STARTUP_DRAGONFLY_CONNECT.as_str()),
            "the refused dial's own code rides `cause_error_code`"
        );
    }
}
