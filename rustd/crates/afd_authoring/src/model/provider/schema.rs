//! Typed model-facing proposals; the native validator still decides acceptance.

use super::{KEY_DESCRIPTION, KEY_TYPE, PROPOSAL_ARGUMENT};
use afd_fleet_runtime::definition::{
    DEFINITION_SCHEMA_VERSION, MAX_CONFIG_OPS, MAX_DESCRIPTION_CHARS, MAX_INSTRUCTION_EDITS,
    MAX_INSTRUCTIONS_CHARS,
};
use afd_wire::authoring::DECIMAL_BYTES_MAX;
use afd_wire::policy::HttpEgress;
use serde_json::{Value, json};

const KEY_ENUM: &str = "enum";
const KEY_MIN_LENGTH: &str = "minLength";
const KEY_MAX_LENGTH: &str = "maxLength";
const KEY_PATTERN: &str = "pattern";
const KEY_MAX_ITEMS: &str = "maxItems";
const KEY_ITEMS: &str = "items";
const KEY_ADDITIONAL_PROPERTIES: &str = "additionalProperties";
const KEY_PROPERTIES: &str = "properties";
const KEY_REQUIRED: &str = "required";
const KEY_ONE_OF: &str = "oneOf";
const KEY_KIND: &str = "kind";
const KEY_DEFINITION: &str = "definition";
const KEY_BASE_CANDIDATE_ID: &str = "base_candidate_id";
const KEY_BASE_CANDIDATE_REVISION: &str = "base_candidate_revision";
const KEY_CONFIG_OPS: &str = "config_ops";
const KEY_INSTRUCTION_EDITS: &str = "instruction_edits";
const KEY_SCHEMA_VERSION: &str = "schema_version";
const KEY_INSTRUCTIONS_MD: &str = "instructions_md";
const KEY_CONFIG: &str = "config";
const KEY_NAME: &str = "name";
const KEY_POLICY: &str = "x-agentsfleet";
const KEY_TOOLS: &str = "tools";
const KEY_BUDGET: &str = "budget";
const KEY_DAILY_NANOS: &str = "daily_nanos";
const KEY_OP: &str = "op";
const KEY_PATH: &str = "path";
const KEY_VALUE: &str = "value";
const KEY_OLD_TEXT: &str = "old_text";
const KEY_TEXT: &str = "text";
const TYPE_STRING: &str = "string";
const TYPE_ARRAY: &str = "array";
const TYPE_OBJECT: &str = "object";
const TYPE_INTEGER: &str = "integer";
const KIND_INITIALIZE: &str = "initialize";
const KIND_EDIT: &str = "edit";
const ACTION_ADD: &str = "add";
const ACTION_REPLACE: &str = "replace";
const ACTION_REMOVE: &str = "remove";
const ACTION_APPEND: &str = "append";
const DECIMAL_PATTERN: &str = "^[0-9]+$";

pub(super) fn parameters(editing: bool) -> Value {
    let proposal = if editing {
        edit()
    } else {
        object(
            &json!({
                KEY_KIND:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[KIND_INITIALIZE]},
                KEY_DEFINITION:definition()
            }),
            &[KEY_KIND, KEY_DEFINITION],
        )
    };
    object(&json!({PROPOSAL_ARGUMENT:proposal}), &[PROPOSAL_ARGUMENT])
}

fn edit() -> Value {
    object(
        &json!({
            KEY_KIND:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[KIND_EDIT]},
            KEY_BASE_CANDIDATE_ID:{KEY_TYPE:TYPE_STRING,KEY_MIN_LENGTH:36,KEY_MAX_LENGTH:36},
            KEY_BASE_CANDIDATE_REVISION:{KEY_TYPE:TYPE_STRING,KEY_PATTERN:DECIMAL_PATTERN,KEY_MIN_LENGTH:1,KEY_MAX_LENGTH:DECIMAL_BYTES_MAX,
                KEY_DESCRIPTION:"Copy the current candidate revision exactly as a string, such as '1'. Never a number."},
            KEY_CONFIG_OPS:{KEY_TYPE:TYPE_ARRAY,KEY_MAX_ITEMS:MAX_CONFIG_OPS,KEY_ITEMS:config_operation()},
            KEY_INSTRUCTION_EDITS:{KEY_TYPE:TYPE_ARRAY,KEY_MAX_ITEMS:MAX_INSTRUCTION_EDITS,KEY_ITEMS:instruction_edit()}
        }),
        &[
            KEY_KIND,
            KEY_BASE_CANDIDATE_ID,
            KEY_BASE_CANDIDATE_REVISION,
            KEY_CONFIG_OPS,
            KEY_INSTRUCTION_EDITS,
        ],
    )
}

fn object(properties: &Value, required: &[&str]) -> Value {
    json!({KEY_TYPE:TYPE_OBJECT,KEY_ADDITIONAL_PROPERTIES:false,KEY_PROPERTIES:properties,KEY_REQUIRED:required})
}

fn definition() -> Value {
    object(
        &json!({
            KEY_SCHEMA_VERSION:{KEY_TYPE:TYPE_INTEGER,KEY_ENUM:[DEFINITION_SCHEMA_VERSION]},
            KEY_DESCRIPTION:{KEY_TYPE:TYPE_STRING,KEY_MIN_LENGTH:1,KEY_MAX_LENGTH:MAX_DESCRIPTION_CHARS},
            KEY_INSTRUCTIONS_MD:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:MAX_INSTRUCTIONS_CHARS},
            KEY_CONFIG:configuration()
        }),
        &[
            KEY_SCHEMA_VERSION,
            KEY_DESCRIPTION,
            KEY_INSTRUCTIONS_MD,
            KEY_CONFIG,
        ],
    )
}

fn configuration() -> Value {
    object(
        &json!({
            KEY_NAME:{KEY_TYPE:TYPE_STRING},
            KEY_POLICY:{
                KEY_TYPE:TYPE_OBJECT,
                KEY_DESCRIPTION:"Native policy. Other supported runtime fields remain subject to daemon validation.",
                KEY_PROPERTIES:{
                    KEY_TOOLS:list(&json!({KEY_TYPE:TYPE_STRING})),
                    "credentials":list(&json!({KEY_TYPE:TYPE_STRING})),
                    "triggers":list(&json!({KEY_TYPE:TYPE_OBJECT})),
                    "network":network(),
                    "gates":object(&json!({
                        "timeout_ms":{KEY_TYPE:TYPE_INTEGER},
                        "rules":list(&json!({KEY_TYPE:TYPE_OBJECT})),
                        "anomaly_rules":list(&json!({KEY_TYPE:TYPE_OBJECT}))
                    }), &[]),
                    KEY_BUDGET:object(&json!({
                        KEY_DAILY_NANOS:{KEY_TYPE:TYPE_STRING,KEY_PATTERN:DECIMAL_PATTERN},
                        "monthly_nanos":{KEY_TYPE:TYPE_STRING,KEY_PATTERN:DECIMAL_PATTERN}
                    }), &[KEY_DAILY_NANOS])
                },
                KEY_REQUIRED:[KEY_TOOLS,KEY_BUDGET]
            }
        }),
        &[KEY_NAME, KEY_POLICY],
    )
}

fn network() -> Value {
    object(
        &json!({
            "http_egress":{KEY_TYPE:TYPE_STRING,KEY_ENUM:[HttpEgress::Restricted,HttpEgress::Public],
                KEY_DESCRIPTION:"Default restricted. Use public HTTPS only when the customer explicitly requests public egress."},
            "package_manager_domains":{KEY_TYPE:"boolean",
                KEY_DESCRIPTION:"Default false. Permit standard package registry domains through HTTP tools when requested."},
            "allow":list(&json!({KEY_TYPE:TYPE_STRING})),
            "read_only":{KEY_TYPE:"boolean"},
            "read_post_paths":list(&json!({KEY_TYPE:TYPE_STRING}))
        }),
        &[],
    )
}

fn list(items: &Value) -> Value {
    json!({KEY_TYPE:TYPE_ARRAY,KEY_ITEMS:items})
}

fn config_operation() -> Value {
    let write = object(
        &json!({
            KEY_OP:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_ADD,ACTION_REPLACE]},
            KEY_PATH:{KEY_TYPE:TYPE_STRING}, KEY_VALUE:{}
        }),
        &[KEY_OP, KEY_PATH, KEY_VALUE],
    );
    let remove = object(
        &json!({
            KEY_OP:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_REMOVE]}, KEY_PATH:{KEY_TYPE:TYPE_STRING}
        }),
        &[KEY_OP, KEY_PATH],
    );
    json!({KEY_ONE_OF:[write,remove]})
}

fn instruction_edit() -> Value {
    let replace = object(
        &json!({
            KEY_KIND:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_REPLACE]},
            KEY_OLD_TEXT:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:MAX_INSTRUCTIONS_CHARS,
                KEY_DESCRIPTION:"Exact source text that occurs once in the current instructions. Include enough surrounding text to make it unique. Empty only when the current instructions are empty."},
            KEY_TEXT:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:MAX_INSTRUCTIONS_CHARS}
        }),
        &[KEY_KIND, KEY_OLD_TEXT, KEY_TEXT],
    );
    let append = object(
        &json!({
            KEY_KIND:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_APPEND]},
            KEY_TEXT:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:MAX_INSTRUCTIONS_CHARS,
                KEY_DESCRIPTION:"New guidance to append verbatim. Include leading newlines for a new paragraph. Prefer append when adding instructions; do not rewrite existing guidance just to add a rule."}
        }),
        &[KEY_KIND, KEY_TEXT],
    );
    json!({KEY_ONE_OF:[replace,append]})
}
