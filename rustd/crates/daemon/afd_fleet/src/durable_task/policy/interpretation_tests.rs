//! An interpretation's spans cut the message into ordered, non-empty pieces.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use af_core::error_code;
use af_fleet_runtime::config::FleetConfig;
use af_wire::durable_task::{
    DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskInputAction,
    DurableTaskProposedItem,
};

use crate::durable_task::policy::DurableTaskPolicy;

const MESSAGE: &str = "Repair titles; reject empty tags";

fn config() -> FleetConfig {
    let document = format!(
        r#"{{"name":"pull-request-repair","x-agentsfleet":{{"tools":[],"triggers":[],"budget":{{"daily_nanos":"1000000000"}},"decision_policy":{}}}}}"#,
        af_fleet_runtime::test_util::DECISION_POLICY
    );
    FleetConfig::stored(&document).expect("a fleet with a reviewed decision policy")
}

fn input(spans: &[(i32, i32)]) -> DurableTaskBindingsInput {
    DurableTaskBindingsInput {
        fencing_token: 1,
        source_turn_id: "0190a8f0-0000-7000-8000-000000000001".to_owned(),
        accepted_revision_id: "0190a8f0-0000-7000-8000-000000000002".to_owned(),
        plan_digest: "0".repeat(64),
        message_decision_call_id: "0190a8f0-0000-7000-8000-000000000003".to_owned(),
        items: spans
            .iter()
            .map(|&(source_start, source_end)| DurableTaskBindingItem {
                decision_call_id: None,
                proposed: DurableTaskProposedItem {
                    source_start,
                    source_end,
                    action: DurableTaskInputAction::Steer,
                    durable_task_id: None,
                    target: None,
                    title: "Steer".to_owned(),
                    interpretation: "Steer the running repair".to_owned(),
                },
            })
            .collect(),
    }
}

// Value: protects=two items never claim the same words and no item claims
// none; fails_when=an overlapping, empty or reversed span is interpreted;
// why_new=the span-order refusal had no case; seam=the policy's own
// interpretation check.
#[test]
fn overlapping_empty_or_reversed_spans_are_ambiguous() {
    let config = config();
    let decision = config.decision_policy().expect("reviewed decision policy");
    let policy = DurableTaskPolicy::default();

    let ordered = policy
        .interpretation(&input(&[(0, 14), (15, 32)]), MESSAGE, decision)
        .expect("ordered spans are interpreted");
    assert_eq!(ordered, ["Repair titles;", "reject empty tags"]);

    for spans in [
        [(0, 14), (10, 32)],
        [(0, 14), (20, 20)],
        [(0, 14), (30, 20)],
    ] {
        let refused = policy
            .interpretation(&input(&spans), MESSAGE, decision)
            .expect_err("the spans do not cut the message cleanly");
        assert_eq!(
            refused.code(),
            error_code::DURABLE_TASK_INPUT_AMBIGUOUS,
            "{spans:?}"
        );
    }
}
