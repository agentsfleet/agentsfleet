//! Text anchors cannot guess positions or modify a different candidate revision.

use af_core::money::NANOS_PER_USD;
use serde_json::{Value, json};

use super::{Head, Native, Proposal, Suggestion, native};
use crate::definition::{Definition, EditBatch};
use crate::setup::Rejection;

/// The head candidate every edit here names.
const ID: &str = "01900000-0000-7000-8000-000000000001";
/// Its revision, as a proposal spells it.
const REVISION: &str = "1";
/// Its revision, as the head carries it.
const HEAD_REVISION: u64 = 1;

/// A setup whose instructions are `text`.
fn definition(text: &str) -> Result<Definition, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(json!({
        "schema_version":1,"description":"Answer questions.","instructions_md":text,
        "config":{"name":"text-edits","x-agentsfleet":{
            "tools":[],"credentials":[],"network":{"allow":[]},
            "gates":{"timeout_ms":60000,"rules":[]},"budget":{"daily_nanos":NANOS_PER_USD.to_string()}
        }}
    }))?)
}

/// `definition` as the head candidate at `revision`.
const fn head(definition: &Definition, revision: u64) -> Head<'_> {
    Head {
        id: ID,
        revision,
        definition,
    }
}

fn suggestion(edits: &Value) -> Result<Suggestion, Box<dyn std::error::Error>> {
    suggestion_at(edits, REVISION)
}

/// [`suggestion`] naming the head's revision spelled as `revision`.
fn suggestion_at(edits: &Value, revision: &str) -> Result<Suggestion, Box<dyn std::error::Error>> {
    Ok(serde_json::from_value(json!({
        "kind":"edit","base_candidate_id":ID,"base_candidate_revision":revision,
        "config_ops":[],"instruction_edits":edits
    }))?)
}

/// The edit `native` resolved, applied to `base`.
fn applied(base: &Definition, resolved: Native) -> Result<Definition, Box<dyn std::error::Error>> {
    let Proposal::Edit {
        config_ops,
        instruction_edits,
        ..
    } = resolved
    else {
        return Err("expected an edit".into());
    };
    Ok(base.edit(EditBatch {
        config_ops,
        instruction_edits,
    })?)
}

// Value: protects=intact multi-byte instructions around a small model edit;
// fails_when=character counts are used as bytes or unrelated text is dropped.
#[test]
fn unicode_anchors_become_exact_native_ranges_and_preserve_the_surrounding_text()
-> Result<(), Box<dyn std::error::Error>> {
    let base = definition("語🙂 before café after\n")?;
    let proposed = suggestion(&json!([{"kind":"replace","old_text":"café","text":"evidence"}]))?;
    let resolved = native(proposed, Some(&head(&base, HEAD_REVISION)), None)
        .map_err(|reason| format!("unexpected rejection: {reason:?}"))?;
    let Proposal::Edit {
        instruction_edits, ..
    } = &resolved
    else {
        return Err("expected an edit".into());
    };
    let range = instruction_edits.first().ok_or("missing native edit")?;
    assert_eq!((range.start_byte, range.end_byte), (15, 20));
    let changed = applied(&base, resolved)?;
    assert_eq!(changed.instructions_md(), "語🙂 before evidence after\n");
    Ok(())
}

// Value: protects=ambiguous, overlapping, absent and empty anchors never choose a guessed match;
// fails_when=the adapter picks the first occurrence or drops unknown offsets into the native editor.
#[test]
fn ambiguous_missing_empty_and_invalid_anchors_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    for (text, anchor, reason) in [
        ("again again", "again", Rejection::InstructionsAmbiguous),
        ("aaa", "aa", Rejection::InstructionsAmbiguous),
        ("🙂🙂🙂", "🙂🙂", Rejection::InstructionsAmbiguous),
        ("text", "absent", Rejection::InstructionsMissing),
        ("text", "", Rejection::InstructionsAmbiguous),
        ("text", "\0", Rejection::InvalidArguments),
    ] {
        let base = definition(text)?;
        let proposed =
            suggestion(&json!([{"kind":"replace","old_text":anchor,"text":"replacement"}]))?;
        assert_eq!(
            native(proposed, Some(&head(&base, HEAD_REVISION)), None).err(),
            Some(reason)
        );
        assert_eq!(base.instructions_md(), text);
    }
    Ok(())
}

// Value: protects=private model text cannot bind edits to a stale or nonexistent base;
// fails_when=the edit resolves against a candidate other than the head it names.
#[test]
fn stale_and_missing_candidate_snapshots_cannot_resolve_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let base = definition("current")?;
    let edit = json!([{"kind":"replace","old_text":"current","text":"changed"}]);
    assert_eq!(
        native(suggestion(&edit)?, None, None).err(),
        Some(Rejection::StaleCandidate)
    );
    assert_eq!(
        native(suggestion(&edit)?, Some(&head(&base, 2)), None).err(),
        Some(Rejection::StaleCandidate)
    );
    Ok(())
}

// Value: protects=a revision is compared as the number it is;
// fails_when=a spelling with leading zeros reads as a stale candidate.
#[test]
fn a_revision_with_leading_zeros_names_the_head() -> Result<(), Box<dyn std::error::Error>> {
    let base = definition("current")?;
    let edit = json!([{"kind":"append","text":" More."}]);

    let resolved = native(
        suggestion_at(&edit, "01")?,
        Some(&head(&base, HEAD_REVISION)),
        None,
    );

    assert!(resolved.is_ok(), "{:?}", resolved.err());
    Ok(())
}

// Value: protects=a revision that is not a number is malformed arguments, never compared;
// fails_when=a non-number reaches the comparison and reads as a stale candidate.
#[test]
fn a_revision_that_is_not_a_number_is_refused_as_malformed()
-> Result<(), Box<dyn std::error::Error>> {
    let base = definition("current")?;
    let edit = json!([{"kind":"append","text":" More."}]);

    for spelled in ["one", "-1", "+1", "1.0", ""] {
        let refused = native(
            suggestion_at(&edit, spelled)?,
            Some(&head(&base, HEAD_REVISION)),
            None,
        );
        assert_eq!(
            refused.err(),
            Some(Rejection::InvalidArguments),
            "{spelled}"
        );
    }
    Ok(())
}

// Value: protects=an edit with no setup to apply to is refused as stale, never applied to nothing;
// fails_when=apply edits an absent head or reports the refusal as malformed arguments.
#[test]
fn an_edit_with_no_head_to_apply_to_is_refused_as_stale() -> Result<(), Box<dyn std::error::Error>>
{
    let base = definition("current")?;
    let resolved = native(
        suggestion(&json!([{"kind":"append","text":" More."}]))?,
        Some(&head(&base, HEAD_REVISION)),
        None,
    )
    .map_err(|reason| format!("unexpected rejection: {reason:?}"))?;

    let refused = resolved
        .apply(None)
        .err()
        .ok_or("an edit applied to nothing")?;

    assert_eq!(refused.reason, Rejection::StaleCandidate);
    assert!(refused.detail.is_none(), "{:?}", refused.detail);
    Ok(())
}

// Value: protects=a first setup never replaces a draft's head wholesale; fails_when=the
// head check on a first setup is dropped and the proposal is accepted on any draft.
#[test]
fn a_first_setup_on_a_draft_with_a_head_is_refused_as_stale()
-> Result<(), Box<dyn std::error::Error>> {
    let base = definition("current")?;
    let first = || -> Result<Suggestion, Box<dyn std::error::Error>> {
        Ok(serde_json::from_value(
            json!({"kind":"initialize","definition":definition("replaced")?}),
        )?)
    };

    let refused = native(first()?, Some(&head(&base, HEAD_REVISION)), None);
    assert_eq!(refused.err(), Some(Rejection::StaleCandidate));

    let accepted = native(first()?, None, None)
        .map_err(|reason| format!("a first setup with no head was refused: {reason:?}"))?;
    let Proposal::Initialize { definition } = accepted else {
        return Err("a first setup read as an edit".into());
    };
    assert_eq!(definition.instructions_md(), "replaced");
    Ok(())
}

// Value: protects=an empty editable instruction field can still receive its first text;
// fails_when=empty anchors are rejected even when exactly one insertion position exists.
#[test]
fn empty_instructions_have_one_unambiguous_insertion_position()
-> Result<(), Box<dyn std::error::Error>> {
    let base = definition("")?;
    let proposed =
        suggestion(&json!([{"kind":"replace","old_text":"","text":"Answer directly."}]))?;
    let resolved = native(proposed, Some(&head(&base, HEAD_REVISION)), None)
        .map_err(|reason| format!("unexpected rejection: {reason:?}"))?;
    assert_eq!(
        applied(&base, resolved)?.instructions_md(),
        "Answer directly."
    );
    Ok(())
}

// Value: protects=adding several rules preserves every prior byte and the declared rule order;
// fails_when=the model must repeat old text to append or additions share conflicting native ranges.
#[test]
fn multiple_additions_append_once_after_unicode_instructions()
-> Result<(), Box<dyn std::error::Error>> {
    let base = definition("語🙂 Keep this rule.")?;
    let proposed = suggestion(&json!([
        {"kind":"append","text":"\n\nAccept corrections."},
        {"kind":"append","text":"\nAcknowledge uncertainty."}
    ]))?;
    let resolved = native(proposed, Some(&head(&base, HEAD_REVISION)), None)
        .map_err(|reason| format!("unexpected rejection: {reason:?}"))?;
    let Proposal::Edit {
        instruction_edits, ..
    } = &resolved
    else {
        return Err("expected an edit".into());
    };
    assert_eq!(instruction_edits.len(), 1);
    assert_eq!(
        applied(&base, resolved)?.instructions_md(),
        "語🙂 Keep this rule.\n\nAccept corrections.\nAcknowledge uncertainty."
    );
    Ok(())
}

// Value: protects=an instruction added "exactly as written" stores the person's bytes, not the model's retyping;
// fails_when=the edit keeps straight quotes the model substituted for the person's curly ones.
#[test]
fn an_appended_rule_keeps_the_persons_quotes_and_every_prior_byte()
-> Result<(), Box<dyn std::error::Error>> {
    let base = definition("Answer directly.")?;
    let said = "Add this instruction exactly as written: Write units as “ms”, “µs” and “°C” in every answer.";
    let proposed = suggestion(
        &json!([{"kind":"append","text":"\n\n- Write units as \"ms\", \"µs\" and \"°C\" in every answer."}]),
    )?;
    let resolved = native(proposed, Some(&head(&base, HEAD_REVISION)), Some(said))
        .map_err(|reason| format!("unexpected rejection: {reason:?}"))?;
    assert_eq!(
        applied(&base, resolved)?.instructions_md(),
        "Answer directly.\n\n- Write units as “ms”, “µs” and “°C” in every answer."
    );
    Ok(())
}
