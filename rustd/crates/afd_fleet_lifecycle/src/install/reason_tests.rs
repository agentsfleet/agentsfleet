//! What the install's best-effort lines say when a datastore will not answer:
//! each carries its code as `error_code` and the failure's sentence as
//! `reason`, never the `[CODE]` rendering.

#![expect(
    clippy::expect_used,
    reason = "a fixture whose own constants are malformed should stop the suite"
)]

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_dragonfly::{Dragonfly, DragonflyConfig, DragonflyRole};

use crate::Fleets;
use crate::error::ErrorKind;

/// A Dragonfly nobody listens on: port 1 is reserved and unbound.
const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

/// A Dragonfly address nothing routes to, so a connect waits out its timeout.
const SILENT_QUEUE: &str = "redis://192.0.2.1:6379";

/// Why building a Dragonfly handle that never connects cannot fail.
const LAZY_MANAGER: &str = "a lazy manager opens no socket";

/// The instant every fixture here is stamped at.
const AT: UnixMillis = UnixMillis::from_millis(1_767_225_600_000);

/// A fleet store over a database and a queue nobody answers on.
fn dead_fleets() -> Fleets {
    let config = DragonflyConfig::from_url(DragonflyRole::Default, NOWHERE_QUEUE.to_owned());
    Fleets::new(
        afd_db::test_util::unreachable_db(),
        Dragonfly::unreachable(&config).expect(LAZY_MANAGER),
        Arc::new(Kek::from_bytes([7; afd_crypto::KEY_LEN])),
        Entropy::new(),
    )
}

/// A fixed identifier, distinct per `seed`.
fn id(seed: u8) -> Uuid7 {
    Uuid7::encode(AT, [seed; ENTROPY_LEN]).expect("a fixed instant encodes")
}

/// Whether `reason` is a sentence with no `[CODE]` prefix in it.
fn told_only(reason: Option<&str>) -> bool {
    reason.is_some_and(|reason| !reason.is_empty() && !reason.contains("[UZ-"))
}

#[tokio::test]
async fn a_rolled_back_install_and_its_failed_rollback_carry_no_code_prefix() {
    let log = Capture::install();
    let cause = crate::Error::from(ErrorKind::SkillRejected);

    let answered = dead_fleets().roll_back(&id(2), &id(1), &cause).await;

    assert!(matches!(answered.kind(), ErrorKind::InstallRolledBack));
    let line = log.only("install_rollback_failed");
    assert_eq!(line.field("reason"), Some(cause.told().as_str()));
    assert!(told_only(line.field("rollback_error")), "{line:?}");
    assert_eq!(line.field("hint"), Some("row_orphaned_manual_recovery"));
}

#[tokio::test]
async fn an_unclassified_bundle_reason_carries_no_code_prefix() {
    let log = Capture::install();

    dead_fleets()
        .request_declared_grants(&id(2), &id(1), &["GITHUB_TOKEN".to_owned()], AT)
        .await;

    let line = log.only("install_grant_classify_failed");
    assert!(told_only(line.field("reason")), "{line:?}");
}

/// A queue whose connect times out, which the install retries.
///
/// The address is in TEST-NET-1 (RFC 5737), which nothing answers on, so the
/// connect ends in the timeout rather than in a refusal the retry skips.
fn timing_out_fleets() -> Fleets {
    let config = DragonflyConfig::from_url(DragonflyRole::Default, SILENT_QUEUE.to_owned())
        .with_connect_timeout(std::time::Duration::from_millis(25));
    Fleets::new(
        afd_db::test_util::unreachable_db(),
        Dragonfly::unreachable(&config).expect(LAZY_MANAGER),
        Arc::new(Kek::from_bytes([7; afd_crypto::KEY_LEN])),
        Entropy::new(),
    )
}

#[tokio::test]
async fn a_retried_stream_reason_carries_no_code_prefix() {
    let log = Capture::install();

    timing_out_fleets()
        .ensure_stream("fleet-fixture")
        .await
        .expect_err("a queue nobody answers on never creates the group");

    let retried: Vec<_> = log
        .events()
        .into_iter()
        .filter(|line| line.field("event") == Some("install_stream_retry"))
        .collect();
    assert!(!retried.is_empty(), "an unavailable queue is retried");
    for line in retried {
        assert_eq!(
            line.field("error_code"),
            Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
        );
        assert!(told_only(line.field("reason")), "{line:?}");
    }
}
