//! What the model is told about a tool.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::Deserialize;

/// The key a generated schema names its type under, which no provider reads.
const TITLE: &str = "title";

/// A tool's description and its parameters' JSON Schema.
///
/// Provider-neutral: each provider wraps it in its own function-calling wire.
/// Built only by [`Schema::of`], so what a model is told a tool takes is always
/// derived from the type the tool's arguments parse into, never hand-written
/// beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    /// What the tool does, as the model reads it.
    description: &'static str,
    /// The arguments' JSON Schema, an object schema.
    parameters: serde_json::Value,
}

impl Schema {
    /// What the tool does, as the model reads it.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        self.description
    }

    /// The arguments' JSON Schema: one flat object schema.
    #[must_use]
    pub const fn parameters(&self) -> &serde_json::Value {
        &self.parameters
    }

    /// The schema of `T`, the type a call's arguments parse into: one flat
    /// object, with no meta-schema line and no definitions to resolve, which
    /// every provider's function-calling wire reads.
    #[must_use]
    pub fn of<T: JsonSchema>(description: &'static str) -> Self {
        let mut parameters = SchemaSettings::draft2020_12()
            .with(|settings| {
                settings.meta_schema = None;
                settings.inline_subschemas = true;
            })
            .into_generator()
            .into_root_schema_for::<T>();
        parameters.remove(TITLE);
        Self {
            description,
            parameters: parameters.to_value(),
        }
    }
}

// The arguments a tool that reads none takes, `list_agents` and every test
// tool among them: its schema is the empty object that refuses every key,
// derived like a real tool's rather than written as JSON beside it. The doc
// line below is what schemars hands the model as the schema's description,
// so it is written for the model.
/// Takes no arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "schemars renders a unit struct as `null`; the braces make it the empty object every provider's function wire expects"
)]
pub struct NoArguments {}
