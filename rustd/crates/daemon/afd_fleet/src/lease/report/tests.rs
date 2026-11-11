//! What a settled report says about itself: the posture fallback and the
//! run's token sums. Split from [`super`] the way `verdict/tests.rs` is, so the verb's
//! own file stays readable.

use af_core::id::{ENTROPY_LEN, Uuid7};
use afd_billing::rates::Posture;
use jiff::Timestamp;

use super::{RunTokens, posture_of};
use crate::lease::affinity::Fence;
use crate::lease::settle::Reported;

/// A settled lease, with three distinct identifiers.
///
/// Distinct because the warnings under test name the fleet, the event and
/// the lease separately, and a fixture that reused one id would let a
/// swapped field pass.
fn reported(posture: &str) -> Result<Reported, &'static str> {
    let at = Timestamp::constant(1_767_225_600, 0);
    let mint = |seed: u8| {
        Uuid7::encode(at, [seed; ENTROPY_LEN])
            .map_err(|_unencodable| "a fixed timestamp and entropy encode to a Uuid7")
    };
    Ok(Reported {
        fleet_id: mint(1)?,
        workspace_id: mint(2)?,
        tenant_id: mint(3)?,
        event_id: mint(4)?,
        actor: "api".to_owned(),
        posture: posture.to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-opus-5".to_owned(),
        fence: Fence::from_i64(1),
        lease_created_at: at,
        event_created_at: at,
        catalogue_version: None,
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

const SAMPLE_INPUT: i64 = 1_000;

/// Every prompt class counts toward the run's input exactly once, and a
/// sum that would overflow saturates instead of wrapping.
#[test]
fn test_prompt_counts_reads_and_writes_once() {
    let tokens = RunTokens {
        input_tokens: SAMPLE_INPUT,
        cached_input_tokens: 400,
        cache_write_tokens: 250,
        output_tokens: 90,
    };
    assert_eq!(tokens.prompt(), 1_650, "output is not prompt");

    let full = RunTokens {
        input_tokens: i64::MAX,
        cache_write_tokens: 1,
        ..RunTokens::default()
    };
    assert_eq!(full.prompt(), i64::MAX);
}
