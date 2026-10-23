//! Pin the successful wire and refuse ambiguous executable/refused objects.

use super::{ProposalCall, ProposalRejection};
use crate::authoring::Proposal;
use serde_json::{Value, json};

// Exact pre-existing successful wire: runners and durable receipts read this shape.
const PROPOSED_WIRE: &str =
    r#"{"id":"provider-call","proposal":{"kind":"initialize","definition":{}}}"#;

#[test]
fn successful_calls_keep_the_existing_wire_in_both_directions()
-> Result<(), Box<dyn std::error::Error>> {
    let expected: Value = serde_json::from_str(PROPOSED_WIRE)?;
    let call: ProposalCall = ProposalCall::Proposed {
        id: "provider-call".into(),
        proposal: Proposal::Initialize {
            definition: Box::new(json!({})),
        },
    };
    assert_eq!(serde_json::to_value(&call)?, expected);
    let decoded: ProposalCall = serde_json::from_value(expected)?;
    assert!(matches!(decoded, ProposalCall::Proposed { .. }));
    assert_eq!(decoded.id(), call.id());
    Ok(())
}

#[test]
fn each_refusal_round_trips_without_an_executable_proposal()
-> Result<(), Box<dyn std::error::Error>> {
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
        assert_eq!(serde_json::to_value(&call)?, expected);
        let decoded: ProposalCall = serde_json::from_value(expected)?;
        assert!(
            matches!(decoded, ProposalCall::Refused { reason: actual, .. } if actual == reason)
        );
        assert_eq!(decoded.id(), call.id());
    }
    Ok(())
}

#[test]
fn a_call_with_both_proposal_and_refusal_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut proposed: Value = serde_json::from_str(PROPOSED_WIRE)?;
    proposed
        .as_object_mut()
        .ok_or("object call")?
        .insert("reason".into(), json!("invalid_arguments"));
    serde_json::from_value::<ProposalCall>(proposed)
        .err()
        .ok_or("mixed authority accepted")?;
    // Reverse key order must not let the first matching variant swallow authority.
    let refused_first = r#"{"id":"provider-call","reason":"invalid_arguments","proposal":{"kind":"initialize","definition":{}}}"#;
    serde_json::from_str::<ProposalCall>(refused_first)
        .err()
        .ok_or("mixed authority accepted")?;
    Ok(())
}

#[test]
fn unknown_fields_are_rejected_on_both_shapes() -> Result<(), Box<dyn std::error::Error>> {
    let proposed: Value = serde_json::from_str(PROPOSED_WIRE)?;
    for mut call in [
        proposed,
        json!({"id":"provider-call", "reason":"invalid_arguments"}),
    ] {
        call.as_object_mut()
            .ok_or("object call")?
            .insert("unexpected".into(), json!(true));
        serde_json::from_value::<ProposalCall>(call)
            .err()
            .ok_or("unknown field accepted")?;
    }
    Ok(())
}

#[test]
fn missing_outcomes_unknown_reasons_and_discriminators_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    for malformed in [
        json!({"id":"provider-call"}),
        json!({"id":"provider-call", "reason":"unrecognized"}),
        json!({"id":"provider-call", "proposal":null}),
        json!({"id":"provider-call", "reason":null}),
        json!({"id":"provider-call", "reason":"invalid_arguments", "outcome":"refused"}),
    ] {
        serde_json::from_value::<ProposalCall>(malformed)
            .err()
            .ok_or("malformed call accepted")?;
    }
    Ok(())
}

#[cfg(feature = "openapi")]
#[test]
fn each_published_alternative_rejects_unknown_fields() -> Result<(), Box<dyn std::error::Error>> {
    let schema = serde_json::to_value(<ProposalCall as utoipa::PartialSchema>::schema())?;
    let alternatives = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .ok_or("exclusive call alternatives")?;
    assert_eq!(alternatives.len(), 2);
    for payload in ["proposal", "reason"] {
        let alternative = alternatives
            .iter()
            .find(|alternative| {
                alternative
                    .get("properties")
                    .and_then(|properties| properties.get(payload))
                    .is_some()
            })
            .ok_or("each outcome has its own object alternative")?;
        let properties = alternative
            .get("properties")
            .and_then(Value::as_object)
            .ok_or("object fields")?;
        let required = alternative
            .get("required")
            .and_then(Value::as_array)
            .ok_or("required fields")?;
        assert_eq!(properties.len(), 2);
        assert_eq!(required.len(), properties.len());
        assert!(required.contains(&json!("id")));
        assert!(required.contains(&json!(payload)));
        // With every declared field required, this cap rules out any extra key.
        assert_eq!(
            alternative.get("maxProperties"),
            Some(&json!(required.len()))
        );
    }
    Ok(())
}
