//! Each task refusal answers one fixed public code and one sentence that names
//! no task content.

use super::DurableTaskRefusal;

#[test]
fn every_refusal_answers_its_fixed_code_and_sentence() {
    for (reason, code, sentence) in [
        (
            DurableTaskRefusal::NotFound,
            "UZ-DT-001",
            "the task does not belong to this workspace and fleet",
        ),
        (
            DurableTaskRefusal::StaleState,
            "UZ-DT-002",
            "the task generation or lifecycle state changed",
        ),
        (
            DurableTaskRefusal::ReplayConflict,
            "UZ-DT-003",
            "the task operation conflicts with its saved interpretation",
        ),
        (
            DurableTaskRefusal::InvalidTarget,
            "UZ-DT-004",
            "the task has no accepted native target binding",
        ),
        (
            DurableTaskRefusal::LimitExhausted,
            "UZ-DT-005",
            "the task allowance or unfinished-task capacity is exhausted",
        ),
        (
            DurableTaskRefusal::DecisionOverage,
            "UZ-DT-005",
            "a decision cost exceeded its reservation; related paid work is blocked",
        ),
        (
            DurableTaskRefusal::DecisionUnavailable,
            "UZ-DT-006",
            "the fleet cannot currently obtain a funded reviewed decision",
        ),
        (
            DurableTaskRefusal::DecisionInputTooLarge,
            "UZ-REQ-002",
            "the decision request exceeds the decision model's reviewed input limit",
        ),
        (
            DurableTaskRefusal::InputAmbiguous,
            "UZ-DT-007",
            "the input needs clarification before it can change work",
        ),
        (
            DurableTaskRefusal::CheckpointIncompatible,
            "UZ-DT-008",
            "the checkpoint cannot safely restore this task generation",
        ),
        (
            DurableTaskRefusal::StorageUnavailable,
            "UZ-DT-009",
            "checkpoint storage cannot currently establish object custody",
        ),
        (
            DurableTaskRefusal::EffectUncertain,
            "UZ-DT-010",
            "the saved publication requires authoritative reconciliation",
        ),
    ] {
        assert_eq!(reason.code().as_str(), code, "{reason:?}");
        assert_eq!(reason.detail(), sentence, "{reason:?}");
        assert_eq!(reason.to_string(), sentence, "{reason:?}");
    }
}
