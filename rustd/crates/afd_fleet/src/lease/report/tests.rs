//! What a settled report says about itself, and what a finalize step that
//! did not land says: the posture fallback, and the warning that names the
//! step. Split from [`super`] the way `verdict/tests.rs` is, so the verb's
//! own file stays readable.

use afd_billing::rates::Posture;
use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};

use super::posture_of;
use super::steps::step;
use crate::lease::affinity::Fence;
use crate::lease::settle::Reported;

/// A settled lease, with three distinct identifiers.
///
/// Distinct because the warnings under test name the fleet, the event and
/// the lease separately, and a fixture that reused one id would let a
/// swapped field pass.
fn reported(posture: &str) -> Result<Reported, &'static str> {
    let at = UnixMillis::from_millis(1_767_225_600_000);
    let mint = |seed: u8| {
        Uuid7::encode(at, [seed; ENTROPY_LEN])
            .map_err(|_unencodable| "a fixed timestamp and entropy encode to a Uuid7")
    };
    Ok(Reported {
        fleet_id: mint(1)?,
        workspace_id: mint(2)?,
        tenant_id: mint(3)?,
        event_id: "event-fixture".to_owned(),
        actor: "api".to_owned(),
        posture: posture.to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-opus-5".to_owned(),
        fence: Fence::from_i64(1),
    })
}

/// Every stored spelling round-trips, and an edited one meters as platform.
///
/// The fallback is the interesting half and the reason it is not silent:
/// the column is written only by the issue path from `Posture::as_str`, so
/// a value that will not parse means the row was edited out of band. The
/// near-misses are deliberate — a case change, a hyphen for an underscore,
/// a leading space, and a posture spelling borrowed from a DIFFERENT column
/// — because each is what an out-of-band edit actually looks like, and a
/// parser that trimmed or lowercased would silently accept them.
#[test]
fn an_unparseable_stored_posture_meters_as_platform() -> Result<(), &'static str> {
    for known in [Posture::Platform, Posture::SelfManaged] {
        assert_eq!(
            posture_of(&reported(known.as_str())?),
            known,
            "a spelling this codebase wrote recovers as itself"
        );
    }

    for edited in ["", "Platform", "self-managed", "sandboxed", " platform"] {
        assert_eq!(
            posture_of(&reported(edited)?),
            Posture::Platform,
            "an unrecognised stored posture falls back rather than guessing"
        );
    }
    Ok(())
}

/// A finalize step renders every refusal it can be handed, and skips none.
///
/// The five call sites hand this whatever their step returned, at the one
/// moment the report has ALREADY settled — so a panic in the reporting
/// itself would lose a charge that is committed. Both `to_string` and
/// `code` run arbitrary per-kind code, which is what makes the loop over
/// kinds the point rather than one representative error.
#[test]
fn a_failed_finalize_step_renders_every_refusal() -> Result<(), &'static str> {
    let lease = reported(Posture::Platform.as_str())?;

    step("settled", &lease, "lease-fixture", Ok(()));

    for failure in [
        crate::error::stale_fence(),
        crate::error::lease_not_found(),
        crate::error::lease_lost(),
        crate::error::lease_max_runtime(),
        crate::error::renewal_no_credits(),
        crate::error::budget_exhausted(),
    ] {
        assert!(
            !failure.to_string().is_empty(),
            "the refusal renders to something an operator can act on"
        );
        assert!(!failure.code().as_str().is_empty());
        step("released", &lease, "lease-fixture", Err(failure));
    }
    Ok(())
}
