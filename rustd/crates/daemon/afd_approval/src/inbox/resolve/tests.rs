//! What a resolved gate continues: the answer turn its waiting run is owed.

use std::borrow::Cow;

use afd_api_wire::approval::{GATE_TYPE_QUESTION, status};

use super::{Continues, KIND_INTEGRATION_GRANT};

#[test]
fn an_answered_question_continues_with_its_answer() {
    assert_eq!(
        Continues::of(status::ANSWERED, GATE_TYPE_QUESTION),
        Some(Continues::Answer)
    );
}

#[test]
fn an_access_card_continues_whichever_way_a_person_answered() {
    assert_eq!(
        Continues::of(status::APPROVED, KIND_INTEGRATION_GRANT),
        Some(Continues::Access { approved: true })
    );
    assert_eq!(
        Continues::of(status::DENIED, KIND_INTEGRATION_GRANT),
        Some(Continues::Access { approved: false })
    );
}

#[test]
fn a_denied_question_or_an_unanswered_card_continues_nothing() {
    assert_eq!(Continues::of(status::DENIED, GATE_TYPE_QUESTION), None);
    assert_eq!(Continues::of(status::APPROVED, GATE_TYPE_QUESTION), None);
    assert_eq!(Continues::of(status::PENDING, KIND_INTEGRATION_GRANT), None);
    assert_eq!(
        Continues::of(status::ARCHIVED, KIND_INTEGRATION_GRANT),
        None
    );
    assert_eq!(Continues::of(status::APPROVED, ""), None);
}

/// The waiting run reads the answer as a question's answer: the person's
/// words for a question, `approved` or `denied` for an access card.
#[test]
fn an_access_answer_names_what_the_person_decided() -> Result<(), Box<dyn std::error::Error>> {
    let turn = af_core::id::Uuid7::parse("01890a5d-ac96-774b-bcce-b302099a8057")?;
    let resolved = super::Resolved {
        gate_id: turn.clone(),
        action_id: turn.clone(),
        workspace_id: turn.clone(),
        fleet_id: turn.clone(),
        status: status::APPROVED.to_owned(),
        resolved_at: jiff::Timestamp::UNIX_EPOCH,
        resolved_by: "person".to_owned(),
        detail: "the person's note".to_owned(),
        turn_id: Some(turn.clone()),
        gate_type: KIND_INTEGRATION_GRANT.to_owned(),
        continuation_turn_id: None,
    };
    for (continues, message) in [
        (Continues::Access { approved: true }, "approved"),
        (Continues::Access { approved: false }, "denied"),
        (Continues::Answer, "the person's note"),
    ] {
        let expected = serde_json::json!(af_wire::turn::QuestionAnswer {
            message: Cow::Borrowed(message),
            asked_turn_id: Cow::Borrowed(turn.as_str()),
        });
        assert_eq!(continues.body(&resolved, &turn), expected);
    }
    Ok(())
}
