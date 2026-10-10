//! What a renewal says when the catalogue cannot price its tokens.

use afd_billing::Cumulative;
use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_core::test_util::trace::Capture;
use tracing::Level;

use super::{EVENT_TOKENS_HELD, Renewing, hold_tokens};

/// The counts the fixture renewal could not price, one per token class.
const HELD: Cumulative = Cumulative {
    input: 25_000,
    cached: 2_500,
    output: 4_000,
};

/// A lease with three distinct identifiers, so a swapped field cannot pass.
fn renewing() -> Result<Renewing, &'static str> {
    let at = UnixMillis::from_millis(1_767_225_600_000);
    let mint = |seed: u8| {
        Uuid7::encode(at, [seed; ENTROPY_LEN])
            .map_err(|_unencodable| "a fixed timestamp and entropy encode to a Uuid7")
    };
    Ok(Renewing {
        tenant_id: mint(1)?,
        fleet_id: mint(2)?,
        workspace_id: mint(3)?,
        posture: "platform".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-opus-5-5".to_owned(),
        status: "active".to_owned(),
    })
}

#[test]
fn a_catalogue_fault_logs_the_held_tokens() -> Result<(), &'static str> {
    let log = Capture::install();
    let lease = renewing()?;
    let fault = crate::error::query("rate read")(sqlx::Error::PoolTimedOut);
    hold_tokens(&lease, "lease-fixture", HELD, &fault);

    let event = log.only(EVENT_TOKENS_HELD);
    assert_eq!(event.level, Level::WARN);
    assert_eq!(event.field("fleet_id"), Some(lease.fleet_id.as_str()));
    assert_eq!(event.field("lease_id"), Some("lease-fixture"));
    assert_eq!(
        (
            event.field("input_tokens"),
            event.field("cached_input_tokens"),
            event.field("output_tokens"),
        ),
        (
            Some(HELD.input.to_string().as_str()),
            Some(HELD.cached.to_string().as_str()),
            Some(HELD.output.to_string().as_str()),
        ),
        "the warning names every count the renewal left for a priced slice"
    );
    assert_eq!(event.field("error_code"), Some(fault.code().as_str()));
    Ok(())
}
