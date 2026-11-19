//! Each task refusal answers one fixed public code and one sentence that names
//! no task content.

use super::TaskRefusal;

#[test]
fn every_refusal_answers_its_fixed_code_and_sentence() {
    for (reason, code, sentence) in [
        (
            TaskRefusal::NotFound,
            "UZ-DT-001",
            "the task does not belong to this workspace and fleet",
        ),
        (
            TaskRefusal::StaleState,
            "UZ-DT-002",
            "the task generation or lifecycle state changed",
        ),
        (
            TaskRefusal::ReplayConflict,
            "UZ-DT-003",
            "the task operation conflicts with its saved interpretation",
        ),
        (
            TaskRefusal::InvalidTarget,
            "UZ-DT-004",
            "the task has no accepted native target binding",
        ),
        (
            TaskRefusal::LimitExhausted,
            "UZ-DT-005",
            "the task allowance or unfinished-task capacity is exhausted",
        ),
        (
            TaskRefusal::DecisionOverage,
            "UZ-DT-005",
            "a decision cost exceeded its reservation; related paid work is blocked",
        ),
        (
            TaskRefusal::DecisionUnavailable,
            "UZ-DT-006",
            "the fleet cannot currently obtain a funded reviewed decision",
        ),
        (
            TaskRefusal::InputAmbiguous,
            "UZ-DT-007",
            "the input needs clarification before it can change work",
        ),
        (
            TaskRefusal::CheckpointIncompatible,
            "UZ-DT-008",
            "the checkpoint cannot safely restore this task generation",
        ),
        (
            TaskRefusal::StorageUnavailable,
            "UZ-DT-009",
            "checkpoint storage cannot currently establish object custody",
        ),
        (
            TaskRefusal::EffectUncertain,
            "UZ-DT-010",
            "the saved publication requires authoritative reconciliation",
        ),
    ] {
        assert_eq!(reason.code().as_str(), code, "{reason:?}");
        assert_eq!(reason.detail(), sentence, "{reason:?}");
        assert_eq!(reason.to_string(), sentence, "{reason:?}");
    }
}
