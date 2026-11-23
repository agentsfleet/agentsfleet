//! Complete public requests reject ambiguous identities and unsafe recovery paths.
use af_wire::{
    activity::DurableTaskActivity,
    decision::{DecisionProfile, DecisionRequest},
    durable_task::{CheckpointPageInput, CheckpointPlaintext, DurableTaskEffectInput},
};
use garde::Validate;
use serde_json::{Value, json};

const ID: &str = "01900000-0000-7000-8000-000000000001";

fn errors(result: Result<(), garde::Report>) -> Vec<(String, String)> {
    result
        .err()
        .into_iter()
        .flat_map(|report| {
            report
                .iter()
                .map(|(path, error)| (path.to_string(), error.to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn required<T>(value: Option<T>) -> serde_json::Result<T> {
    value.ok_or_else(|| {
        <serde_json::Error as serde::de::Error>::custom("complete request fixture field")
    })
}

#[test]
fn duplicate_question_and_choice_identities_are_rejected_at_their_fields() -> serde_json::Result<()>
{
    let question = json!({"name":"route","question":{"type":"choice","instructions":"Choose route",
        "options":[{"value":"WAIT","description":"Wait"},{"value":"DONE","description":"Finish"}]}});
    let mut request: DecisionRequest = serde_json::from_value(json!({"call_id":ID,
        "purpose":"intent","state":{},"questions":[question]}))?;
    assert_eq!(errors(request.validate()), vec![]);
    request
        .questions
        .push(required(request.questions.first())?.clone());
    assert_eq!(
        errors(request.validate()),
        vec![(
            "questions".into(),
            "decision question names must be unique".into()
        )]
    );
    request.questions.pop();
    let question = &mut required(request.questions.first_mut())?.question;
    assert!(matches!(
        question,
        af_wire::decision::DecisionQuestion::Choice { .. }
    ));
    if let af_wire::decision::DecisionQuestion::Choice { options, .. } = question {
        required(options.get_mut(1))?.value = "WAIT".into();
    }
    assert_eq!(
        errors(request.validate()),
        vec![(
            "questions[0].question.options".into(),
            "decision choices must have unique identities".into()
        )]
    );
    Ok(())
}

fn page(entries: &[Value]) -> serde_json::Result<CheckpointPageInput> {
    serde_json::from_value(
        json!({"fencing_token":7,"durable_task":{"durable_task_id":ID,"generation":2},
        "pin_id":ID,"digest":"a".repeat(64),"contents":{"page_number":0,"entries":entries,"history_objects":[]}}),
    )
}

#[test]
fn checkpoint_validation_rejects_unsafe_paths_links_and_noncanonical_digests()
-> serde_json::Result<()> {
    let valid = page(&[json!({"kind":"symlink","path":"src/link","target":"../data"})])?;
    assert_eq!(errors(valid.validate()), vec![]);
    for (entry, field, message) in [
        (
            json!({"kind":"directory","path":"../outside"}),
            "contents.entries[0].path",
            "expected a safe relative path",
        ),
        (
            json!({"kind":"symlink","path":"src/link","target":"/outside"}),
            "contents.entries[0].target",
            "expected a relative symlink",
        ),
    ] {
        assert_eq!(
            errors(page(&[entry])?.validate()),
            vec![(field.into(), message.into())]
        );
    }
    let mut invalid = valid;
    invalid.digest = "A".repeat(64);
    assert_eq!(
        errors(invalid.validate()),
        vec![("digest".into(), "expected lowercase SHA-256".into())]
    );
    Ok(())
}

#[test]
fn manifest_byte_bound_applies_even_when_individual_entries_are_valid() -> serde_json::Result<()> {
    let entry = json!({"kind":"directory","path":"a".repeat(4096)});
    assert_eq!(errors(page(&vec![entry.clone(); 250])?.validate()), vec![]);
    assert_eq!(
        errors(page(&vec![entry; 256])?.validate()),
        vec![(
            "contents".into(),
            "manifest page exceeds one mebibyte".into()
        )]
    );
    Ok(())
}

#[test]
fn publication_rejects_case_variant_workflow_paths_without_rejecting_adjacent_files()
-> serde_json::Result<()> {
    let mut input: DurableTaskEffectInput = serde_json::from_value(
        json!({"operation_id":ID,"fencing_token":7,
        "durable_task":{"durable_task_id":ID,"generation":2},"source_item_id":ID,"covered_input_sequence":3,
        "expected_head":"a".repeat(40),"tested_tree":"b".repeat(40),
        "changes":[{"path":".github/README.md","contents":"eA=="}]}),
    )?;
    assert_eq!(errors(input.validate()), vec![]);
    for path in [".GiThUb/WORKFLOWS/check.yml", ".github/workflows/check.yml"] {
        path.clone_into(&mut required(input.changes.first_mut())?.path);
        assert_eq!(
            errors(input.validate()),
            vec![(
                "changes[0].path".into(),
                "unsupported publication path".into()
            )]
        );
    }
    Ok(())
}

/// Numbers nothing forced are gone: a value past each retired cap proves,
/// and a size past its Postgres `BIGINT` column still does not.
#[test]
fn values_past_the_retired_caps_are_accepted() -> serde_json::Result<()> {
    let past = "m".repeat(2049);
    let profile: DecisionProfile = serde_json::from_value(json!({"model_id":ID,
        "revision":"r".repeat(129),"api":"typesafe","endpoint":past,"model":"decision-model",
        "accepted_models":vec![past.clone(); 9],
        "capabilities":{"noul":true,"choice":true,"score":true},"input_max_bytes":24_577}))?;
    assert_eq!(errors(profile.validate()), vec![]);
    let activity: DurableTaskActivity = serde_json::from_value(json!({"activity_id":ID,
        "durable_task_id":null,"source_turn_id":null,"operation":"capture","phase":"uploading",
        "provider":past,"model":past,"completed":200_001,"total":200_001,
        "bytes":{"acknowledged":68_719_476_737_u64,"missing":0,"reused":0}}))?;
    assert_eq!(errors(activity.validate()), vec![]);
    let mut plaintext: CheckpointPlaintext =
        serde_json::from_value(json!({"sha256":"a".repeat(64),"size":8_388_609,"kind":"file"}))?;
    assert_eq!(errors(plaintext.validate()), vec![]);
    plaintext.size = i64::MAX.unsigned_abs() + 1;
    assert_eq!(
        errors(plaintext.validate())
            .into_iter()
            .map(|(path, _message)| path)
            .collect::<Vec<_>>(),
        vec!["size".to_owned()]
    );
    Ok(())
}

/// utoipa takes only a literal, so the published bound is held to the ceiling.
#[cfg(feature = "openapi")]
#[test]
fn the_published_text_bounds_are_the_ceiling() -> serde_json::Result<()> {
    let schema = serde_json::to_value(
        <af_wire::decision::DecisionModelSettings as utoipa::PartialSchema>::schema(),
    )?;
    for field in ["endpoint", "evaluation_source_url"] {
        assert_eq!(
            schema.pointer(&format!("/properties/{field}/maxLength")),
            Some(&json!(af_validate::TEXT_MAX_CHARS)),
            "{field}"
        );
    }
    assert_eq!(
        schema.pointer("/properties/revision/maxLength"),
        Some(&json!(af_wire::decision::REVISION_MAX_BYTES))
    );
    Ok(())
}

/// A revision fills its btree index row and no more: 2,704 bytes less the
/// tuple header, the `uuid` key and the varlena header.
#[test]
fn a_revision_past_its_index_row_is_refused() -> serde_json::Result<()> {
    use af_wire::decision::REVISION_MAX_BYTES;
    assert_eq!(REVISION_MAX_BYTES, 2676);
    let profile = |revision: String| {
        serde_json::from_value::<DecisionProfile>(json!({"model_id":ID,"revision":revision,
            "api":"typesafe","endpoint":"https://example.test/decide","model":"decision-model",
            "accepted_models":["decision-model"],
            "capabilities":{"noul":true,"choice":true,"score":true},"input_max_bytes":1}))
    };
    assert_eq!(
        errors(profile("r".repeat(REVISION_MAX_BYTES))?.validate()),
        vec![]
    );
    assert_eq!(
        errors(profile("r".repeat(REVISION_MAX_BYTES + 1))?.validate())
            .into_iter()
            .map(|(path, _message)| path)
            .collect::<Vec<_>>(),
        vec!["revision".to_owned()]
    );
    Ok(())
}
