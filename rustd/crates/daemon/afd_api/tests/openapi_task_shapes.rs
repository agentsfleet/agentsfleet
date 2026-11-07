//! Real serialized task values must fit the published composite and map shapes.
#![cfg(all(feature = "test-util", feature = "openapi"))]
#![expect(
    clippy::expect_used,
    reason = "schema regression prerequisites fail loudly"
)]

use afd_wire::{
    decision::DecisionAnswer,
    task::{CheckpointEntry, CheckpointFile},
};
use serde_json::{Value, json};

use crate::openapi_contract::document;

#[test]
fn tagged_checkpoint_files_fit_every_closed_member_without_widening_the_wire() {
    let document = document();
    let value = serde_json::to_value(CheckpointEntry::File(CheckpointFile {
        path: "src/main.rs".to_owned(),
        executable: false,
        size: 0,
        objects: Vec::new(),
    }))
    .expect("actual file entry serializes");
    let file = document
        .pointer("/components/schemas/CheckpointEntry/oneOf/0/allOf")
        .and_then(Value::as_array)
        .expect("tagged file composition");
    let mut closed = 0;
    for member in file {
        let member = resolve(&document, member);
        let properties = member
            .get("properties")
            .and_then(Value::as_object)
            .expect("file member properties");
        if member.get("additionalProperties") == Some(&json!(false)) {
            closed += 1;
            for key in value.as_object().expect("serialized object").keys() {
                assert!(properties.contains_key(key), "closed member rejects {key}");
            }
        }
        for required in member
            .get("required")
            .and_then(Value::as_array)
            .expect("required fields")
        {
            assert!(value.get(required.as_str().expect("field name")).is_some());
        }
        assert_eq!(
            properties.get("kind").and_then(|kind| kind.get("enum")),
            Some(&json!(["file"]))
        );
    }
    assert_eq!(closed, 1, "file fields remain closed to unknown properties");
    let mut unknown = value.clone();
    unknown
        .as_object_mut()
        .expect("file object")
        .insert("unexpected".into(), json!(true));
    serde_json::from_value::<CheckpointEntry>(unknown)
        .expect_err("unknown checkpoint property is rejected");
    serde_json::from_value::<CheckpointEntry>(value).expect("known checkpoint fields decode");
}

#[test]
fn score_distribution_keys_are_json_strings_and_runtime_collisions_stay_rejected() {
    let document = document();
    let probabilities = document
        .pointer("/components/schemas/DecisionAnswer/oneOf/2/properties/probabilities")
        .expect("score map schema");
    assert_eq!(
        probabilities.pointer("/propertyNames/type"),
        Some(&json!("string"))
    );
    assert_eq!(
        probabilities.pointer("/additionalProperties/type"),
        Some(&json!("number"))
    );
    let answer = DecisionAnswer::Score {
        score: 0.2,
        confidence: Some(0.8),
        probabilities: [(0, 0.8), (1, 0.2)].into(),
    };
    let value = serde_json::to_value(&answer).expect("score serializes");
    assert_eq!(value.get("probabilities"), Some(&json!({"0":0.8,"1":0.2})));
    assert_eq!(
        serde_json::from_value::<DecisionAnswer>(value).expect("numeric keys decode"),
        answer
    );
    let collision = json!({"type":"score","score":0.2,"confidence":0.8,
        "probabilities":{"0":0.8,"00":0.1,"1":0.2}});
    serde_json::from_value::<DecisionAnswer>(collision)
        .expect_err("colliding score keys are rejected");
}

#[test]
fn checkpoint_plan_creation_and_replay_publish_the_same_response_shape() {
    let document = document();
    let responses = document
        .get("paths")
        .and_then(|paths| paths.get("/v1/runners/me/leases/{lease_id}/tasks/checkpoints"))
        .and_then(|path| path.get("post"))
        .and_then(|operation| operation.get("responses"))
        .expect("checkpoint plan responses");
    let content = |status| {
        responses
            .get(status)
            .and_then(|response| response.get("content"))
    };
    assert!(content("200").is_some(), "replay is a documented success");
    assert_eq!(content("200"), content("201"));
}

fn resolve<'a>(document: &'a Value, schema: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .map_or(schema, |reference| {
            document
                .pointer(reference.strip_prefix('#').expect("local schema reference"))
                .expect("referenced schema exists")
        })
}
