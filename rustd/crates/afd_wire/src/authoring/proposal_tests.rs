//! Pin the successful wire and refuse ambiguous executable/refused objects.

use super::{ProposalCall, ProposalRejection};
use crate::authoring::Proposal;
use serde_json::{Value, json};

// Exact pre-existing successful wire: runners and durable receipts read this shape.
const PROPOSED_WIRE: &str =
    r#"{"id":"provider-call","proposal":{"kind":"initialize","definition":{}}}"#;

#[test]
fn successful_calls_keep_the_existing_wire_in_both_directions() {
    let expected: Value = serde_json::from_str(PROPOSED_WIRE).expect("reviewed wire fixture");
    let call: ProposalCall = ProposalCall::Proposed {
        id: "provider-call".into(),
        proposal: Proposal::Initialize {
            definition: Box::new(json!({})),
        },
    };
    assert_eq!(
        serde_json::to_value(&call).expect("serializable call"),
        expected
    );
    let decoded: ProposalCall = serde_json::from_value(expected).expect("existing successful wire");
    assert!(matches!(decoded, ProposalCall::Proposed { .. }));
    assert_eq!(decoded.id(), call.id());
}

#[test]
fn each_refusal_round_trips_without_an_executable_proposal() {
    for (reason, spelling) in [
        (ProposalRejection::InvalidArguments, "invalid_arguments"),
        (
            ProposalRejection::InstructionsMissing,
            "instructions_missing",
        ),
        (
            ProposalRejection::InstructionsAmbiguous,
            "instructions_ambiguous",
        ),
        (ProposalRejection::StaleCandidate, "stale_candidate"),
    ] {
        let call: ProposalCall = ProposalCall::Refused {
            id: "provider-call".into(),
            reason,
        };
        let expected = json!({"id":"provider-call", "reason":spelling});
        assert_eq!(
            serde_json::to_value(&call).expect("serializable refusal"),
            expected
        );
        let decoded: ProposalCall = serde_json::from_value(expected).expect("closed refusal shape");
        assert!(
            matches!(decoded, ProposalCall::Refused { reason: actual, .. } if actual == reason)
        );
        assert_eq!(decoded.id(), call.id());
    }
}

#[test]
fn a_call_with_both_proposal_and_refusal_is_rejected() {
    let mut proposed: Value = serde_json::from_str(PROPOSED_WIRE).expect("reviewed wire fixture");
    proposed["reason"] = json!("invalid_arguments");
    assert!(serde_json::from_value::<ProposalCall>(proposed).is_err());
    // Reverse key order must not let the first matching variant swallow authority.
    let refused_first = json!({
        "id":"provider-call", "reason":"invalid_arguments",
        "proposal":{"kind":"initialize", "definition":{}}
    });
    assert!(serde_json::from_value::<ProposalCall>(refused_first).is_err());
}

#[test]
fn unknown_fields_are_rejected_on_both_shapes() {
    let proposed: Value = serde_json::from_str(PROPOSED_WIRE).expect("reviewed wire fixture");
    for mut call in [
        proposed,
        json!({"id":"provider-call", "reason":"invalid_arguments"}),
    ] {
        call["unexpected"] = json!(true);
        assert!(serde_json::from_value::<ProposalCall>(call).is_err());
    }
}

#[test]
fn missing_outcomes_unknown_reasons_and_discriminators_are_rejected() {
    for malformed in [
        json!({"id":"provider-call"}),
        json!({"id":"provider-call", "reason":"unrecognized"}),
        json!({"id":"provider-call", "proposal":null}),
        json!({"id":"provider-call", "reason":null}),
        json!({"id":"provider-call", "reason":"invalid_arguments", "outcome":"refused"}),
    ] {
        assert!(serde_json::from_value::<ProposalCall>(malformed).is_err());
    }
}

#[cfg(feature = "openapi")]
#[test]
fn each_published_alternative_rejects_unknown_fields() {
    let schema = serde_json::to_value(<ProposalCall as utoipa::PartialSchema>::schema())
        .expect("the derived schema serializes");
    let alternatives = schema["oneOf"]
        .as_array()
        .expect("exclusive call alternatives");
    assert_eq!(alternatives.len(), 2);
    for payload in ["proposal", "reason"] {
        let alternative = alternatives
            .iter()
            .find(|alternative| alternative["properties"].get(payload).is_some())
            .expect("each outcome has its own object alternative");
        let properties = alternative["properties"]
            .as_object()
            .expect("object fields");
        let required = alternative["required"].as_array().expect("required fields");
        assert_eq!(properties.len(), 2);
        assert_eq!(required.len(), properties.len());
        assert!(required.contains(&json!("id")));
        assert!(required.contains(&json!(payload)));
        // With every declared field required, this cap rules out any extra key.
        assert_eq!(alternative["maxProperties"], json!(required.len()));
    }
}
