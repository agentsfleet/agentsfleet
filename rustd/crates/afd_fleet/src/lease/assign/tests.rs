use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};

/// A runner identifier for the two reporting helpers.
///
/// Minted rather than parsed from a literal: `Uuid7::encode` is the only
/// constructor that yields one without a fixture string to keep in step
/// with the type's own shape rules.
fn runner_id() -> Result<Uuid7, &'static str> {
    Uuid7::encode(
        UnixMillis::from_millis(1_767_225_600_000),
        [7u8; ENTROPY_LEN],
    )
    .map_err(|_unencodable| "a fixed timestamp and entropy encode to a Uuid7")
}

/// Both queue reporters render every Dragonfly failure kind without panicking.
///
/// Thin, and deliberately so. These are `tracing::warn!` calls with no
/// return value, so what there is to prove is that each field expression
/// EVALUATES for every kind the transport can produce — `error.to_string()`
/// most of all, which is the one that runs arbitrary `Display` code while
/// something is already going wrong.
///
/// That is not a hypothetical concern in these two functions: the fields
/// are hoisted into locals precisely because the `log` bridge compiles a
/// second copy of every field expression, and a panic in the copy that does
/// run would take down a path whose entire job is to report calmly.
#[test]
fn both_queue_reporters_render_every_redis_failure() -> Result<(), &'static str> {
    let runner = runner_id()?;

    for (label, error) in afd_dragonfly::error::one_of_each_kind() {
        assert!(
            !error.to_string().is_empty(),
            "{label} renders to something a reader can act on"
        );
        super::warn_queue("lease_poll_queue_failed", &runner, &error);
        super::warn_queue_fleet("lease_claim_queue_failed", "fleet-fixture", &error);
    }
    Ok(())
}

/// Every kind a grant can carry has a start label of its own.
///
/// The mapping is total by construction — a kind without an arm does not
/// compile — so what is asserted is the PAIRING, which a swapped arm would
/// get wrong while still compiling.
#[test]
fn a_fresh_grant_counts_as_fresh_and_a_reclaim_as_reclaimed() {
    use afd_observability::metrics::label::fleet::RunStart;

    use super::started;
    use crate::lease::envelope::Kind;

    assert_eq!(started(Kind::Fresh), RunStart::Fresh);
    assert_eq!(started(Kind::Reclaim), RunStart::Reclaimed);
}

/// A held fleet whose readiness cannot be read costs the poll that fleet
/// alone: a fleet named twice is read once, its failure is logged and
/// skipped, and the partition pass still runs and turns the cursor. The
/// partition's own read fails here too, and that is what fails this poll.
#[tokio::test]
async fn a_held_fleets_unreadable_readiness_leaves_the_poll_its_partition()
-> Result<(), &'static str> {
    use afd_core::test_util::trace::Capture;
    use afd_dragonfly::ReadyCursor;

    use super::diagnostics::EVENT_HELD_READ_FAILED;
    use crate::lease::test_dead::{AT, id, leases};

    let log = Capture::install();
    let store = leases();

    let polled = store.select(&id(9), &[id(1), id(1)], AT).await;

    polled
        .err()
        .ok_or("the partition's read failed, and so does the poll")?;
    let skipped = log.only(EVENT_HELD_READ_FAILED);
    assert_eq!(skipped.field("fleet_id"), Some(id(1).as_str()));
    let reason = skipped.field("reason").unwrap_or_default();
    assert!(
        !reason.is_empty() && !reason.contains("[UZ-"),
        "the code is the line's `error_code`, never the reason's prefix: {reason}"
    );
    assert_eq!(
        skipped.field("cause_error_code"),
        Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str()),
        "the refused command's own code rides `cause_error_code`"
    );
    assert_ne!(
        store.cursor().advance(),
        ReadyCursor::new().advance(),
        "the partition pass ran"
    );
    Ok(())
}

/// Each queue reporter's reason is the failure's sentence; its constant code
/// rides `error_code` alone.
#[test]
fn a_poll_queue_failure_reason_carries_no_code_prefix() -> Result<(), &'static str> {
    use afd_core::test_util::trace::Capture;

    let runner = runner_id()?;
    let (_label, error) = afd_dragonfly::error::one_of_each_kind()
        .into_iter()
        .next()
        .ok_or("the queue test utility exposes an error")?;
    let told = error.told();

    let log = Capture::install();
    super::warn_queue("lease_poll_queue_failed", &runner, &error);
    super::warn_queue_fleet("lease_claim_queue_failed", "fleet-fixture", &error);

    for event in ["lease_poll_queue_failed", "lease_claim_queue_failed"] {
        let line = log.only(event);
        assert_eq!(
            line.field("error_code"),
            Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
        );
        assert_eq!(
            line.field("cause_error_code"),
            Some(error.code().as_str()),
            "{event}"
        );
        assert_eq!(line.field("reason"), Some(told.as_str()), "{event}");
    }
    Ok(())
}

/// A stream entry no reader can decode is dropped on a dead queue too, and the
/// line that records it carries the decode failure's sentence alone.
#[tokio::test]
async fn an_undecodable_entry_reason_carries_no_code_prefix() {
    use afd_core::test_util::trace::Capture;

    use super::diagnostics::{EVENT_ENTRY_UNDECODABLE_DROP_FAILED, drop_undecodable};
    use crate::lease::test_dead;

    let error = crate::error::lease_lost();
    let told = error.told();
    let leases = test_dead::leases();

    let log = Capture::install();
    drop_undecodable(
        &leases.streams(),
        "fleet-fixture",
        &afd_dragonfly::EventId::of("1767225600000-0"),
        &error,
    )
    .await;

    let line = log.only(EVENT_ENTRY_UNDECODABLE_DROP_FAILED);
    assert_eq!(line.field("reason"), Some(told.as_str()));
    assert_eq!(line.field("cause_error_code"), Some(error.code().as_str()));
}
