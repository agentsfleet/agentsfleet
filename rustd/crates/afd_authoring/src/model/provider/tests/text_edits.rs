//! Text anchors cannot guess positions or modify a different candidate revision.

use super::{Suggestion, native};
use crate::{Result, types::Candidate};
use afd_core::{id::Uuid7, money::NANOS_PER_USD};
use afd_fleet_runtime::definition::EditBatch;
use afd_wire::authoring::{Proposal, ProposalRejection};
use serde_json::{Value, json};

pub(in crate::model::provider) fn candidate(
    text: &str,
) -> Result<Candidate, Box<dyn std::error::Error>> {
    Ok(Candidate {
        id: Uuid7::parse("01900000-0000-7000-8000-000000000001")?,
        revision: "1".into(),
        definition: serde_json::from_value(json!({
            "schema_version":1,"description":"Answer questions.","instructions_md":text,
            "config":{"name":"text-edits","x-agentsfleet":{
                "tools":[],"credentials":[],"network":{"allow":[]},
                "gates":{"timeout_ms":60000,"rules":[]},"budget":{"daily_nanos":NANOS_PER_USD.to_string()}
            }}
        }))?,
        canonical_digest: String::new(),
        policy_semantics: afd_fleet_runtime::definition::POLICY_SEMANTICS.into(),
    })
}

fn suggestion(base: &Candidate, edits: &Value) -> Result<Suggestion, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(json!({
        "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":base.revision,
        "config_ops":[],"instruction_edits":edits
    }))?)
}

// Value: protects=intact multi-byte instructions around a small model edit;
// fails_when=character counts are used as bytes or unrelated text is dropped.
#[test]
fn unicode_anchors_become_exact_native_ranges_and_preserve_the_surrounding_text()
-> Result<(), Box<dyn std::error::Error>> {
    let base = candidate("語🙂 before café after\n")?;
    let proposed = suggestion(
        &base,
        &json!([{"kind":"replace","old_text":"café","text":"evidence"}]),
    )?;
    let Proposal::Edit {
        config_ops,
        instruction_edits,
        ..
    } = native(proposed, Some(&base))
        .map_err(|reason| format!("unexpected rejection: {reason:?}"))?
    else {
        return Err("expected an edit".into());
    };
    let range = instruction_edits.first().ok_or("missing native edit")?;
    assert_eq!((range.start_byte, range.end_byte), (15, 20));
    let changed = base.definition.edit(EditBatch {
        config_ops,
        instruction_edits,
    })?;
    assert_eq!(changed.instructions_md(), "語🙂 before evidence after\n");
    Ok(())
}

// Value: protects=ambiguous, overlapping, absent and empty anchors never choose a guessed match;
// fails_when=the adapter picks the first occurrence or drops unknown offsets into the native editor.
#[test]
fn ambiguous_missing_empty_and_invalid_anchors_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    for (text, anchor, reason) in [
        (
            "again again",
            "again",
            ProposalRejection::InstructionsAmbiguous,
        ),
        ("aaa", "aa", ProposalRejection::InstructionsAmbiguous),
        ("🙂🙂🙂", "🙂🙂", ProposalRejection::InstructionsAmbiguous),
        ("text", "absent", ProposalRejection::InstructionsMissing),
        ("text", "", ProposalRejection::InstructionsAmbiguous),
        ("text", "\0", ProposalRejection::InvalidArguments),
    ] {
        let base = candidate(text)?;
        let proposed = suggestion(
            &base,
            &json!([{"kind":"replace","old_text":anchor,"text":"replacement"}]),
        )?;
        assert_eq!(native(proposed, Some(&base)).err(), Some(reason));
        assert_eq!(base.definition.instructions_md(), text);
    }
    Ok(())
}

// Value: protects=private model text cannot bind edits to a stale or nonexistent base;
// fails_when=the server resolves against a candidate other than the reserved immutable revision.
#[test]
fn stale_and_missing_candidate_snapshots_cannot_resolve_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let mut base = candidate("current")?;
    let proposed = suggestion(
        &base,
        &json!([{"kind":"replace","old_text":"current","text":"changed"}]),
    )?;
    assert_eq!(
        native(proposed, None).err(),
        Some(ProposalRejection::StaleCandidate)
    );
    let proposed = suggestion(
        &base,
        &json!([{"kind":"replace","old_text":"current","text":"changed"}]),
    )?;
    base.revision = "2".into();
    assert_eq!(
        native(proposed, Some(&base)).err(),
        Some(ProposalRejection::StaleCandidate)
    );
    Ok(())
}

// Value: protects=an empty editable instruction field can still receive its first text;
// fails_when=empty anchors are rejected even when exactly one insertion position exists.
#[test]
fn empty_instructions_have_one_unambiguous_insertion_position()
-> Result<(), Box<dyn std::error::Error>> {
    let base = candidate("")?;
    let proposed = suggestion(
        &base,
        &json!([{"kind":"replace","old_text":"","text":"Answer directly."}]),
    )?;
    let Proposal::Edit {
        config_ops,
        instruction_edits,
        ..
    } = native(proposed, Some(&base))
        .map_err(|reason| format!("unexpected rejection: {reason:?}"))?
    else {
        return Err("expected an edit".into());
    };
    assert_eq!(
        base.definition
            .edit(EditBatch {
                config_ops,
                instruction_edits
            })?
            .instructions_md(),
        "Answer directly."
    );
    Ok(())
}

// Value: protects=adding several rules preserves every prior byte and the declared rule order;
// fails_when=the model must repeat old text to append or additions share conflicting native ranges.
#[test]
fn multiple_additions_append_once_after_unicode_instructions()
-> Result<(), Box<dyn std::error::Error>> {
    let base = candidate("語🙂 Keep this rule.")?;
    let proposed = suggestion(
        &base,
        &json!([
            {"kind":"append","text":"\n\nAccept corrections."},
            {"kind":"append","text":"\nAcknowledge uncertainty."}
        ]),
    )?;
    let Proposal::Edit {
        config_ops,
        instruction_edits,
        ..
    } = native(proposed, Some(&base))
        .map_err(|reason| format!("unexpected rejection: {reason:?}"))?
    else {
        return Err("expected an edit".into());
    };
    assert_eq!(instruction_edits.len(), 1);
    let changed = base.definition.edit(EditBatch {
        config_ops,
        instruction_edits,
    })?;
    assert_eq!(
        changed.instructions_md(),
        "語🙂 Keep this rule.\n\nAccept corrections.\nAcknowledge uncertainty."
    );
    Ok(())
}
