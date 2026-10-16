//! Requirements are copied from validated setup, without substituting live status.

use afd_fleet_runtime::{Definition, config::Budget};
use serde::Serialize;
use serde_json::Value;

/// Declarative resource and policy needs of one immutable Definition.
#[derive(Debug, Clone, Serialize)]
pub struct Requirements {
    /// Workspace credential names; values never appear here.
    pub credentials: Vec<String>,
    /// Explicit egress repository bindings.
    pub repositories: Vec<String>,
    /// Declared tool names.
    pub tools: Vec<String>,
    /// Authored network policy, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Value>,
    /// Authored human approval rules, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_rules: Option<Value>,
    /// Exact positive decimal nanos ceilings.
    pub budget: Budget,
}

impl Requirements {
    /// Derives needs from the same normalized setup used by runtime enforcement.
    #[must_use]
    pub fn of(definition: &Definition) -> Self {
        let runtime = definition.runtime();
        let block = &definition.config().value()["x-agentsfleet"];
        Self {
            credentials: runtime
                .credentials()
                .iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
            repositories: runtime
                .repository_binding()
                .map(|binding| {
                    binding
                        .repositories()
                        .iter()
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            tools: runtime.tools().iter().map(ToString::to_string).collect(),
            network: block.get("network").cloned(),
            approval_rules: block
                .get("gates")
                .and_then(|gates| gates.get("rules"))
                .cloned(),
            budget: runtime.budget(),
        }
    }
}
