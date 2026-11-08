//! Review observes the actual trigger secret, routing install and fleet grant.

mod binding;
mod fixture;
mod grants;
mod mentions;
mod svix;
mod webhooks;

use fixture::{Case, check};
use serde_json::json;

const TOKEN: &str = "private-readiness-token";
const SLACK_SCOPES: &[&str] = &["app_mentions:read", "chat:write", "channels:history"];

fn mention(source: &str) -> serde_json::Value {
    json!({"triggers":[{"type":"mention","source":source,"channels":["C012345678"]}]})
}
