//! Corruption cannot extend retention, invent an answer or change report identity.

#![expect(
    clippy::indexing_slicing,
    reason = "a test corrupts a field of the metadata it just read"
)]

use super::{ATTEMPT, Fixture, LEASE, report};
use crate::report_spool::authoring::files;
use afd_core::id::Uuid7;
use afd_wire::authoring::Report;

#[tokio::test]
async fn changed_authenticated_metadata_is_removed_without_opening_an_answer() {
    let fixture = Fixture::new();
    fixture.hold().await;
    let path = fixture.directory.join(files::META);
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    metadata["metadata"]["generation"] = "2".into();
    std::fs::write(path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    fixture.spool.sweep().await.unwrap();
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}

#[tokio::test]
async fn corrupt_ciphertext_keeps_the_independent_accounting_envelope() {
    let fixture = Fixture::new();
    fixture.hold().await;
    std::fs::write(fixture.directory.join(files::CONTENT), b"broken ciphertext").unwrap();
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}

#[tokio::test]
async fn a_missing_or_corrupt_clock_fails_closed_at_startup() {
    let mut fixture = Fixture::new();
    fixture.hold().await;
    std::fs::write(
        fixture.directory.parent().unwrap().join(files::CLOCK),
        b"broken clock",
    )
    .unwrap();
    fixture.restart();
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}

#[tokio::test]
async fn changed_accounting_cannot_replace_the_original_report() {
    let fixture = Fixture::new();
    fixture.hold().await;
    let mut changed = report();
    let Report::Terminal { accounting, .. } = &mut changed else {
        unreachable!()
    };
    accounting.usage.runtime_ms = 101;
    let error = fixture
        .spool
        .hold(&Uuid7::parse(LEASE).unwrap(), ATTEMPT, changed)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("reused with changed accounting"));
    assert_eq!(
        serde_json::to_value(fixture.read().await).unwrap(),
        serde_json::to_value(report()).unwrap()
    );
}

#[tokio::test]
async fn missing_content_key_and_orphaned_temporary_files_are_cleaned() {
    let fixture = Fixture::new();
    fixture.hold().await;
    std::fs::remove_file(fixture.directory.join(files::KEY)).unwrap();
    std::fs::write(
        fixture.directory.join("unfinished"),
        b"encrypted placeholder",
    )
    .unwrap();
    fixture.spool.sweep().await.unwrap();
    let loaded = fixture.read().await;
    fixture.assert_accounting_only(&loaded);
}

#[tokio::test]
async fn a_removed_settlement_does_not_leave_an_expired_timer_in_the_drainer() {
    let fixture = Fixture::new();
    fixture.hold().await;
    assert_eq!(fixture.spool.0.state.lock().unwrap().deadlines.len(), 1);
    std::fs::remove_dir_all(&fixture.directory).unwrap();
    fixture.spool.sweep().await.unwrap();
    assert!(fixture.spool.0.state.lock().unwrap().deadlines.is_empty());
}

#[tokio::test]
async fn an_oversized_report_is_refused_before_any_private_or_accounting_write() {
    let fixture = Fixture::new();
    let mut request = report();
    let Report::Terminal { answer, .. } = &mut request else {
        unreachable!()
    };
    *answer = Some("x".repeat(afd_wire::authoring::ENVELOPE_BYTES_MAX + 1));
    assert!(
        fixture
            .spool
            .hold(&Uuid7::parse(LEASE).unwrap(), ATTEMPT, request)
            .await
            .is_err()
    );
    assert!(!fixture.directory.exists());
}
