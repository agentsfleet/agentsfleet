//! Claims use immutable native setup and refuse missing or stale policy.

#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

mod claim;
mod refusals;

use af_core::money::NANOS_PER_USD;
use af_fleet_runtime::Definition;
use serde_json::json;

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
