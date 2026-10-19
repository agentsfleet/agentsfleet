//! The probe a walked candidate is answered with, when the queue will not say.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::test_util::trace::Capture;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Dragonfly, EventId};

use crate::Admissions;
use crate::reconcile::{EVENT_PROBE_FAILED, Unfinished};

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// The fleet the probe asks about; nothing answers, so any will do.
const FLEET: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeee3";

/// A probe that could not be made answers "still there", and its line says
/// why without repeating the code the line already carries.
#[tokio::test]
async fn a_failed_probe_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    let admissions = Admissions::new(
        afd_db::test_util::unreachable_db(),
        queue,
        afd_crypto::entropy::Entropy::new(),
    );
    let unfinished = Unfinished {
        fleet_id: FLEET.to_owned(),
        receipt: EventId::of("1790000000000-0"),
    };

    assert!(
        admissions.stream_holds(&unfinished).await,
        "an unanswered probe changes nothing"
    );

    let logged = capture.only(EVENT_PROBE_FAILED);
    logged.assert_uncoded_reason();
    assert!(logged.field("error_code").is_some(), "{logged:?}");
}
