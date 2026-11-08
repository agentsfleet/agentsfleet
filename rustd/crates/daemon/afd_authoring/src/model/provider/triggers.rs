//! Trigger shapes come from the native runtime vocabulary rather than a prompt template.

use af_fleet_runtime::config::{
    KIND_CRON, KIND_MENTION, KIND_WEBHOOK, VERIFIED_WEBHOOK_SOURCES, WebhookFamily,
};
use serde_json::{Value, json};

pub(super) fn schema() -> Value {
    json!({"type":"array", "items":{"oneOf":[
        {"type":"object", "required":["type","source"], "properties":{
            "type":{"const":KIND_WEBHOOK},
            "source":{"type":"string", "description":format!(
                "Verified sources: {}. Other sources require a verifier and credential_name.",
                VERIFIED_WEBHOOK_SOURCES.join(", "))},
            "events":{"type":"array", "items":{"type":"string"}},
            "repositories":{"type":"array", "items":{"type":"string"}},
            "verifier":{"type":"object", "properties":{
                "scheme":{"type":"string", "enum":WebhookFamily::ALL.iter().map(|family|family.token()).collect::<Vec<_>>()},
                "header":{"type":"string", "description":"Required for body-only schemes; forbidden for slack-v0 and svix-v1."}
            }}, "credential_name":{"type":"string"}
        }},
        {"type":"object", "required":["type","schedule"], "properties":{
            "type":{"const":KIND_CRON}, "schedule":{"type":"string"},
            "timezone":{"type":"string"}, "message":{"type":"string"}
        }},
        {"type":"object", "required":["type","source","channels"], "properties":{
            "type":{"const":KIND_MENTION}, "source":{"type":"string"},
            "channels":{"type":"array", "minItems":1, "maxItems":1, "items":{"type":"string"}}
        }}
    ]}})
}
