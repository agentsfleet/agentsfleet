//! Shared setup validation, the native corpus, typed round trips, content
//! identity and atomic edits.

#![expect(
    clippy::expect_used,
    reason = "valid fixture shape is a test precondition"
)]

mod corpus;
mod digest;
mod edits;
mod fields;
mod mention;
mod private_text;
mod roundtrip;
#[cfg(feature = "openapi")]
mod schema_parity;
mod trigger_kinds;
mod validation;
mod webhook_source;

use afd_fleet_runtime::Definition;
use serde_json::{Value, json};

/// The anomaly window the shared fixture declares; any positive span would do.
const ANOMALY_WINDOW_S: u32 = 60;

fn value() -> Value {
    json!({
        "schema_version":1,
        "description":"Explain failures.",
        "instructions_md":"Investigate failures.\nLink evidence before changes.\n",
        "config":{"name":"checkout-watch", "x-agentsfleet":{
            "triggers":[{"type":"cron", "schedule":"0 * * * *", "timezone":"UTC", "message":"Review evidence"}],
            "tools":["http_request","shell"], "credentials":["GITHUB_TOKEN"],
            "budget":{"daily_nanos":"1000000001", "monthly_nanos":"10000000001"},
            "network":{"allow":["api.github.com"], "read_only":true, "read_post_paths":["/graphql"]},
            "gates":{"timeout_ms":60000,"rules":[{"tool":"shell","action":"execute","behavior":"approve"}],
                "anomaly_rules":[{"pattern":"same_action","threshold_count":3,"threshold_window_s":ANOMALY_WINDOW_S}]},
            "repositories":["acme/checkout"], "repository_access":"write", "repository_base":"main",
            "context":{"context_cap_tokens":32000, "tool_window":4000, "memory_checkpoint_every":8, "stage_chunk_threshold":0.75},
            "model":"runtime-model"
        }}
    })
}

/// Replaces the field at `pointer`, which the shared fixture must already carry.
fn set(document: &mut Value, pointer: &str, to: Value) {
    *document
        .pointer_mut(pointer)
        .expect("the shared fixture carries the field") = to;
}

fn parse(value: &Value) -> Definition {
    Definition::parse(&value.to_string()).expect("the shared complete fixture is valid")
}
