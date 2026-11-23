//! Requirements are copied from validated setup, without substituting live status.

use af_fleet_runtime::{Definition, config::Budget};
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
            repositories: runtime.repository_binding().map_or_default(|binding| {
                binding
                    .repositories()
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            }),
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

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "fixed test fixtures fail loudly when their contract changes"
)]
mod tests {
    use af_fleet_runtime::Definition;

    use super::Requirements;

    /// A setup whose block adds each of `binding`'s fields to the fixture's.
    fn setup(binding: &[(&str, serde_json::Value)]) -> Definition {
        let mut block = serde_json::json!({
            "triggers": [], "tools": [], "credentials": [], "network": {"allow": []},
            "budget": {"daily_nanos": af_core::money::NANOS_PER_USD.to_string()}
        });
        let fields = block
            .as_object_mut()
            .expect("the fixture block is an object");
        for (field, value) in binding {
            fields.insert((*field).to_owned(), value.clone());
        }
        let json = serde_json::json!({
            "schema_version": 1,
            "description": "Reviews pull requests.",
            "instructions_md": "Review.",
            "config": {"name": "pull-request-review", "x-agentsfleet": block},
        });
        Definition::parse(&json.to_string()).expect("the fixture setup parses")
    }

    /// The repositories a review shows are the ones the setup binds its
    /// credentials to, and none when it binds none.
    #[test]
    fn a_reviews_repositories_are_the_setups_binding() {
        let bound = setup(&[
            (
                "repositories",
                serde_json::json!(["acme/checkout", "acme/billing"]),
            ),
            ("repository_access", serde_json::json!("read")),
        ]);
        assert_eq!(
            Requirements::of(&bound).repositories,
            ["acme/checkout", "acme/billing"]
        );
        let unbound = setup(&[]);
        assert_eq!(
            Requirements::of(&unbound).repositories,
            Vec::<String>::new()
        );
    }
}
