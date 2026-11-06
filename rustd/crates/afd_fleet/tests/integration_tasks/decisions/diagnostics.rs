//! Native reservation and replay diagnostics retain identity without private bodies.

use std::fmt::Debug;

use afd_wire::{
    decision::{DecisionOutcome, DecisionQuestion, DecisionRequest},
    task::DecisionReservationResult,
};
use serde_json::Value;

// Value: protects=the complete wire survives while nested dispatch and replay diagnostics stay private;
// fails_when=keys or private request/results enter Debug, or serialization removes native fields;
// seam=real native lifecycle scenarios supply these bodies; no_claim=provider transport.
pub(super) fn reservation(body: &str) -> Value {
    let value: Value = serde_json::from_str(body).expect("native reservation document");
    let typed: DecisionReservationResult<'_> =
        serde_json::from_str(body).expect("public wire shape");
    assert_eq!(
        serde_json::to_value(&typed).expect("wire serialization"),
        value
    );
    match &typed {
        DecisionReservationResult::Dispatch { call_id, dispatch } => {
            for rendered in safe_diagnostics(&typed, call_id) {
                private_request(&rendered, &dispatch.request);
                assert!(!dispatch.api_key.is_empty());
                assert!(!rendered.contains(dispatch.api_key.as_ref()));
                assert!(!rendered.contains(&dispatch.assignment.profile.endpoint));
                assert!(rendered.contains("<redacted>"));
                assert!(rendered.contains(&dispatch.assignment.profile.model_id));
                assert!(rendered.contains(&dispatch.assignment.revision_id));
            }
        }
        DecisionReservationResult::Existing {
            call_id, recorded, ..
        } => {
            let enclosing = safe_diagnostics(&typed, call_id);
            if let Some(recorded) = recorded {
                for rendered in enclosing
                    .into_iter()
                    .chain(safe_diagnostics(recorded, &recorded.request.call_id))
                {
                    private_request(&rendered, &recorded.request);
                    if let DecisionOutcome::Answered { answers } = &recorded.result.result {
                        for name in answers.keys() {
                            assert_ne!(name, "");
                            assert!(
                                !rendered.contains(name),
                                "recorded answer identities reached diagnostics"
                            );
                        }
                    }
                }
            }
        }
    }
    value
}

fn safe_diagnostics(value: &impl Debug, call: &str) -> [String; 2] {
    let rendered = [format!("{value:?}"), format!("{value:#?}")];
    for diagnostic in &rendered {
        assert!(
            diagnostic.contains(call),
            "native call identity remains readable"
        );
        assert!(
            !diagnostic.contains("native-decision-fixture-"),
            "decision credential reached diagnostics"
        );
    }
    rendered
}

fn private_request(rendered: &str, request: &DecisionRequest) {
    let first = request
        .questions
        .first()
        .expect("native request has accepted questions");
    let instructions = match &first.question {
        DecisionQuestion::Noul { instructions, .. }
        | DecisionQuestion::Choice { instructions, .. }
        | DecisionQuestion::Score { instructions, .. } => instructions,
    };
    for private in std::iter::once(instructions.as_str()).chain(
        request
            .state
            .pointer("/evidence/observation")
            .and_then(Value::as_str),
    ) {
        assert!(!private.is_empty(), "privacy evidence must be nonempty");
        assert!(
            !rendered.contains(private),
            "private question/evidence reached diagnostics"
        );
    }
}
