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

/// Both queue reporters render every Redis failure kind without panicking.
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

    for (label, error) in afd_redis::error::one_of_each_kind() {
        assert!(
            !error.to_string().is_empty(),
            "{label} renders to something a reader can act on"
        );
        super::warn_queue("lease_poll_queue_failed", &runner, &error);
        super::warn_queue_fleet("lease_claim_queue_failed", "fleet-fixture", &error);
    }
    Ok(())
}
