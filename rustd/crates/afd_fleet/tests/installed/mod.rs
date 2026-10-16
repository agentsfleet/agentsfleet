//! Claims use immutable native setup and refuse missing or stale policy.

#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

mod claim;
mod refusals;

use afd_core::id::Uuid7;
use afd_core::money::NANOS_PER_USD;
use afd_fleet_runtime::Definition;
use serde_json::json;

fn id(fleet: &str) -> Uuid7 {
    Uuid7::parse(fleet).expect("fixture fleet UUIDv7")
}

fn definition() -> Definition {
    Definition::parse(
        &json!({"schema_version":1, "description":"Runtime native read.",
        "instructions_md":"---\nKeep this entire text.\n---\nAnswer only in haiku.\n",
        "config":{"name":"probe","x-agentsfleet":{"triggers":[],
            "tools":[],"budget":{"daily_nanos":NANOS_PER_USD.to_string()}}}})
        .to_string(),
    )
    .expect("valid fixture")
}
