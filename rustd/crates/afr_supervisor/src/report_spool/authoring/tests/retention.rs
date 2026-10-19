//! First-write expiry survives retries, restart, wall-clock rollback and suspended time.

use super::{ANSWER, Fixture, report};
use crate::report_spool::authoring::{PRIVATE_TTL, files};
use afd_wire::authoring::Report;
use std::time::Duration;

/// How far the clock moves before the sweep records its high-water mark; any
/// forward step works, since the restart only has to land behind it.
const SWEPT_AFTER: Duration = Duration::from_secs(1);

#[tokio::test]
async fn private_bytes_are_encrypted_and_the_original_report_survives_restart() {
    let mut fixture = Fixture::new();
    fixture.hold().await;
    for entry in std::fs::read_dir(&fixture.directory).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(ANSWER.len())
                .any(|window| window == ANSWER.as_bytes())
        );
    }
    fixture.restart();
    let loaded = fixture.read().await;
    assert!(
        matches!(loaded, Report::Terminal { answer: Some(ref answer), .. } if answer == ANSWER)
    );
    assert_eq!(
        serde_json::to_value(loaded).unwrap(),
        serde_json::to_value(report()).unwrap()
    );
}

#[tokio::test]
async fn restart_after_expiry_removes_every_private_category_before_delivery() {
    let mut fixture = Fixture::new();
    fixture.hold().await;
    std::fs::copy(
        fixture.directory.join(files::CONTENT),
        fixture.directory.join("rejected"),
    )
    .unwrap();
    std::fs::copy(
        fixture.directory.join(files::CONTENT),
        fixture.directory.join("temporary"),
    )
    .unwrap();
    fixture.clock.advance(PRIVATE_TTL);
    fixture.restart();
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}

#[tokio::test]
async fn retry_does_not_rewrite_first_write_time_or_extend_private_retention() {
    let fixture = Fixture::new();
    fixture.hold().await;
    let before = std::fs::read(fixture.directory.join(files::META)).unwrap();
    fixture
        .clock
        .advance(PRIVATE_TTL.saturating_sub(Duration::from_millis(1)));
    fixture.hold().await;
    assert_eq!(
        before,
        std::fs::read(fixture.directory.join(files::META)).unwrap()
    );
    fixture.clock.advance(Duration::from_millis(1));
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}

#[tokio::test(start_paused = true)]
async fn monotonic_expiry_still_removes_content_when_wall_time_stands_still() {
    let fixture = Fixture::new();
    fixture.hold().await;
    tokio::time::advance(PRIVATE_TTL).await;
    fixture.spool.sweep().await.unwrap();
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}

#[tokio::test]
async fn a_restarted_clock_behind_the_saved_high_water_removes_private_content() {
    let mut fixture = Fixture::new();
    fixture.hold().await;
    fixture.clock.advance(SWEPT_AFTER);
    fixture.spool.sweep().await.unwrap();
    fixture.clock.rewind(Duration::from_millis(1));
    fixture.restart();
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}
