//! The proposal tool exposes the same fixed roles the native setup validates.

use af_fleet_runtime::config::decision::{
    ASSOCIATION_CRITERIA, INTENT_OPTIONS, MAX_COMPLETION_STATEMENTS, NEXT_ACTION_OPTIONS,
    NOUL_CRITERIA,
};
use serde_json::{Map, Value, json};

use super::{
    KEY_ITEMS, KEY_MAX_ITEMS, KEY_MAX_LENGTH, KEY_MIN_LENGTH, KEY_PATTERN, KEY_TYPE, TYPE_ARRAY,
    TYPE_STRING, object,
};

const KEY_QUESTION: &str = "question";
const KEY_CRITERIA: &str = "criteria";
const KEY_ID: &str = "id";

pub(super) fn policy() -> Value {
    object(
        &json!({
            "routing": object(&json!({
                "intent": question(&INTENT_OPTIONS),
                "association": question(&ASSOCIATION_CRITERIA),
                "multiple_requests": question(&NOUL_CRITERIA),
            }), &["intent", "association", "multiple_requests"]),
            "completion": object(&json!({
                "statements": {KEY_TYPE: TYPE_ARRAY, "minItems": 1,
                    KEY_MAX_ITEMS: MAX_COMPLETION_STATEMENTS, KEY_ITEMS: statement()},
                "next_action": question(&NEXT_ACTION_OPTIONS),
            }), &["statements", "next_action"]),
        }),
        &["routing", "completion"],
    )
}

fn text() -> Value {
    json!({KEY_TYPE: TYPE_STRING, KEY_MIN_LENGTH: 1, KEY_MAX_LENGTH: 512,
        KEY_PATTERN: "\\S"})
}

fn criteria(keys: &[&str]) -> Value {
    let criteria: Map<String, Value> = keys.iter().map(|key| ((*key).to_owned(), text())).collect();
    object(&Value::Object(criteria), keys)
}

fn question(keys: &[&str]) -> Value {
    object(
        &json!({KEY_QUESTION: text(), KEY_CRITERIA: criteria(keys)}),
        &[KEY_QUESTION, KEY_CRITERIA],
    )
}

fn statement() -> Value {
    object(
        &json!({
            KEY_ID: {KEY_TYPE: TYPE_STRING, KEY_MIN_LENGTH: 1,
                KEY_MAX_LENGTH: 64, KEY_PATTERN: "^[A-Za-z0-9_.-]+$"},
            KEY_QUESTION: text(), KEY_CRITERIA: criteria(&NOUL_CRITERIA),
        }),
        &[KEY_ID, KEY_QUESTION, KEY_CRITERIA],
    )
}
