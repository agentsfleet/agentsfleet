//! Public-boundary parity for the instance-local stream overview.
#![expect(clippy::expect_used, reason = "test fixtures are canonical UUIDv7s")]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::streams::LiveStreams;

const WORKSPACE_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb001";
const FIRST_FLEET_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
const SECOND_FLEET_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011";

#[test]
fn test_streams_overview_parity() {
    let streams = LiveStreams::new(100);
    let workspace = Uuid7::parse(WORKSPACE_ID).expect("workspace fixture is canonical");
    let first = Uuid7::parse(FIRST_FLEET_ID).expect("fleet fixture is canonical");
    let second = Uuid7::parse(SECOND_FLEET_ID).expect("fleet fixture is canonical");
    let _first = streams
        .try_register(
            &workspace,
            &first,
            UnixMillis::from_millis(1_725_000_000_001),
        )
        .expect("first seeded fleet claims a stream slot");
    let _second = streams
        .try_register(
            &workspace,
            &second,
            UnixMillis::from_millis(1_725_000_000_002),
        )
        .expect("second seeded fleet claims a stream slot");

    let actual = serde_json::to_value(streams.overview()).expect("overview serializes");
    assert_eq!(
        actual,
        serde_json::json!({
            "items": [
                {"workspace_id": WORKSPACE_ID, "fleet_id": FIRST_FLEET_ID, "started_ms": 1_725_000_000_001_i64},
                {"workspace_id": WORKSPACE_ID, "fleet_id": SECOND_FLEET_ID, "started_ms": 1_725_000_000_002_i64}
            ],
            "total": 2,
            "max_streams": 100
        })
    );
}
