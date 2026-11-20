//! Typed model-facing proposals; the native validator still decides acceptance.

mod decision;
mod settle;
mod triggers;

pub use settle::settle;

use serde_json::{Value, json};

use super::PROPOSAL_ARGUMENT;
use super::proposal::{CANDIDATE_ID_BYTES, REVISION_DIGITS_MAX};
use crate::config::HttpEgress;
use af_validate::TEXT_MAX_CHARS;

use crate::definition::{
    DEFINITION_SCHEMA_VERSION, MAX_CONFIG_OPS, MAX_DESCRIPTION_CHARS, MAX_INSTRUCTION_EDITS,
};

/// The JSON Schema keyword naming a value's type.
const KEY_TYPE: &str = "type";
/// The JSON Schema keyword describing a field or a policy object.
const KEY_DESCRIPTION: &str = "description";
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
const KEY_REPOSITORIES: &str = "repositories";
/// The settings that only mean something beside a named repository.
const REPOSITORY_SETTINGS: [&str; 4] = [
    "repository_access",
    "repository_base",
    "repository_publication",
    "repository_validation_workflows",
];
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
/// What every optional setting tells the model, so a plain fleet stays plain.
const OMIT: &str = "Omit unless the person asks for it.";
/// What the triggers setting tells the model: chat is not a trigger, a request for none means none.
const TRIGGERS_HELP: &str = "Omit unless the person asks for one. Chat and steer messages always reach a fleet and are never triggers. \
    A trigger starts a run on a schedule (cron), from a webhook or from a Slack mention.";
/// What an edit's two lists tell the model: the one it does not use is left out.
const LEAVE_OUT: &str = "Leave this out when the edit makes no change of this kind.";
/// What every repository setting tells the model.
const WITH_REPOSITORY: &str = "Only together with repositories.";

/// The tool's parameters: an edit of the head candidate while `editing`, a
/// first setup otherwise, so only the action the draft allows takes context.
#[must_use]
pub fn parameters(editing: bool) -> Value {
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
            KEY_BASE_CANDIDATE_ID:{KEY_TYPE:TYPE_STRING,KEY_MIN_LENGTH:CANDIDATE_ID_BYTES,KEY_MAX_LENGTH:CANDIDATE_ID_BYTES},
            KEY_BASE_CANDIDATE_REVISION:{KEY_TYPE:TYPE_STRING,KEY_PATTERN:DECIMAL_PATTERN,KEY_MIN_LENGTH:1,KEY_MAX_LENGTH:REVISION_DIGITS_MAX,
                KEY_DESCRIPTION:"Copy the current candidate revision exactly as a string, such as '1'. Never a number."},
            KEY_CONFIG_OPS:described(json!({KEY_TYPE:TYPE_ARRAY,KEY_MAX_ITEMS:MAX_CONFIG_OPS,KEY_ITEMS:config_operation()}), LEAVE_OUT),
            KEY_INSTRUCTION_EDITS:described(json!({KEY_TYPE:TYPE_ARRAY,KEY_MAX_ITEMS:MAX_INSTRUCTION_EDITS,KEY_ITEMS:instruction_edit()}), LEAVE_OUT)
        }),
        &[KEY_KIND, KEY_BASE_CANDIDATE_ID, KEY_BASE_CANDIDATE_REVISION],
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
            KEY_INSTRUCTIONS_MD:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:TEXT_MAX_CHARS,
                KEY_DESCRIPTION:"The person’s fleet instructions, drafted only from their requests. Preserve supplied wording; ask about missing business decisions."},
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

// Gates are left out of a first setup: their rules name tools, actions and a
// condition grammar the form cannot spell, and a model shown an open object
// invents fields. A person who asks for approvals adds them by edit.
fn configuration() -> Value {
    let [access, base, publication, workflows] = REPOSITORY_SETTINGS;
    object(
        &json!({
            KEY_NAME:{KEY_TYPE:TYPE_STRING,KEY_PATTERN:"^[a-z0-9]+(?:-[a-z0-9]+)*$",
                KEY_DESCRIPTION:"Lowercase fleet slug. Preserve the current name unless the person requests a rename."},
            KEY_POLICY:{
                KEY_TYPE:TYPE_OBJECT,
                KEY_DESCRIPTION:"Every setting except name goes inside this block, budget included, never beside it. An omitted setting means none.",
                KEY_PROPERTIES:{
                    KEY_TOOLS:described(list(&json!({KEY_TYPE:TYPE_STRING})), OMIT),
                    "credentials":described(list(&json!({KEY_TYPE:TYPE_STRING})), OMIT),
                    "triggers":described(triggers::schema(), TRIGGERS_HELP),
                    "network":described(network(), OMIT),
                    "decision_policy":described(decision::policy(),
                        "Omit for ordinary fleets. Only when the person asks for owner-reviewed routing and completion questions."),
                    KEY_REPOSITORIES:described(list(&json!({KEY_TYPE:TYPE_STRING})),
                        "Omit unless the person names a repository, as owner/name; never an empty list."),
                    access:{KEY_TYPE:TYPE_STRING,KEY_ENUM:["read","write"],KEY_DESCRIPTION:WITH_REPOSITORY},
                    base:{KEY_TYPE:TYPE_STRING,KEY_DESCRIPTION:WITH_REPOSITORY},
                    publication:{KEY_TYPE:TYPE_STRING,KEY_ENUM:["event_draft","existing_pr"],
                        KEY_DESCRIPTION:"Only together with repositories. Select existing_pr only when the person explicitly requests repair of assigned existing pull requests; native task publication still validates each target."},
                    workflows:{KEY_TYPE:TYPE_ARRAY,KEY_MAX_ITEMS:16,
                        KEY_DESCRIPTION:"Owner-reviewed workflow paths and exact SHA-256 digests required for existing_pr. Ask for the reviewed identities; never invent approval or hashes.",
                        KEY_ITEMS:object(&json!({"path":{KEY_TYPE:TYPE_STRING},
                            "sha256":{KEY_TYPE:TYPE_STRING,KEY_PATTERN:"^[0-9a-f]{64}$"}}), &["path","sha256"])},
                    KEY_BUDGET:object(&json!({
                        KEY_DAILY_NANOS:{KEY_TYPE:TYPE_STRING,KEY_PATTERN:DECIMAL_PATTERN,
                            KEY_DESCRIPTION:"Positive decimal integer nanos as a string, never dollars or a JSON number: one dollar is \"1000000000\"."},
                        "monthly_nanos":{KEY_TYPE:TYPE_STRING,KEY_PATTERN:DECIMAL_PATTERN}
                    }), &[KEY_DAILY_NANOS])
                },
                KEY_REQUIRED:[KEY_BUDGET]
            }
        }),
        &[KEY_NAME, KEY_POLICY],
    )
}

fn network() -> Value {
    object(
        &json!({
            "http_egress":{KEY_TYPE:TYPE_STRING,KEY_ENUM:[HttpEgress::Restricted,HttpEgress::Public],
                KEY_DESCRIPTION:"Omit unless the person asks for public HTTPS egress."},
            "package_manager_domains":{KEY_TYPE:"boolean",
                KEY_DESCRIPTION:"Omit unless the person asks for package registry access through HTTP tools."},
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

/// `schema` with the sentence that tells the model when to fill it.
fn described(mut schema: Value, description: &str) -> Value {
    if let Some(fields) = schema.as_object_mut() {
        fields.insert(KEY_DESCRIPTION.to_owned(), description.into());
    }
    schema
}

/// Where a setting's path starts and how a list grows, with paths that work.
const PATH_HELP: &str = "JSON Pointer from the document root. It starts with /config, such as /config/x-agentsfleet/budget/daily_nanos, \
    or is /description. To add to a list that exists, end the path with /- and give one item, such as /config/x-agentsfleet/triggers/-. \
    A setting that is absent is added as a new member holding its whole value, a JSON array for a list, \
    such as /config/x-agentsfleet/triggers with [one trigger].";

/// What an edit's value looks like, since its schema declares no type: a trigger as in the first setup, and a list as an array.
const VALUE_HELP: &str = "The new value, shaped as in the first setup. A trigger is \
    {\"type\":\"cron\",\"schedule\":\"0 9 * * *\",\"timezone\":\"UTC\",\"message\":\"Daily check\"}. \
    With a path ending /- the value is that one object; for a list written whole it is a JSON array of them, \
    never an object wrapping an item. A budget amount is a decimal string of nanos.";

fn config_operation() -> Value {
    let path = json!({KEY_TYPE:TYPE_STRING,KEY_DESCRIPTION:PATH_HELP});
    let write = object(
        &json!({
            KEY_OP:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_ADD,ACTION_REPLACE]},
            KEY_PATH:path, KEY_VALUE:{KEY_DESCRIPTION:VALUE_HELP}
        }),
        &[KEY_OP, KEY_PATH, KEY_VALUE],
    );
    let remove = object(
        &json!({
            KEY_OP:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_REMOVE]}, KEY_PATH:path
        }),
        &[KEY_OP, KEY_PATH],
    );
    json!({KEY_ONE_OF:[write,remove]})
}

fn instruction_edit() -> Value {
    let replace = object(
        &json!({
            KEY_KIND:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_REPLACE]},
            KEY_OLD_TEXT:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:TEXT_MAX_CHARS,
                KEY_DESCRIPTION:"Exact source text that occurs once in the current instructions. Include enough surrounding text to make it unique. Empty only when the current instructions are empty."},
            KEY_TEXT:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:TEXT_MAX_CHARS}
        }),
        &[KEY_KIND, KEY_OLD_TEXT, KEY_TEXT],
    );
    let append = object(
        &json!({
            KEY_KIND:{KEY_TYPE:TYPE_STRING,KEY_ENUM:[ACTION_APPEND]},
            KEY_TEXT:{KEY_TYPE:TYPE_STRING,KEY_MAX_LENGTH:TEXT_MAX_CHARS,
                KEY_DESCRIPTION:"New guidance to append verbatim. Include leading newlines for a new paragraph. Prefer append when adding instructions; do not rewrite existing guidance just to add a rule."}
        }),
        &[KEY_KIND, KEY_TEXT],
    );
    json!({KEY_ONE_OF:[replace,append]})
}

#[cfg(test)]
#[path = "../tests/schema.rs"]
mod tests;
