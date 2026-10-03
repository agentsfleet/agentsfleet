//! What the model is told about a tool.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;

/// The key a generated schema names its type under, which no provider reads.
const TITLE: &str = "title";

/// A tool's description and its parameters' JSON Schema.
///
/// Provider-neutral: each provider wraps it in its own function-calling wire.
/// Built only by [`Schema::of`], so every schema the model reads is derived
/// from the type its arguments parse into and none is written by hand.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    description: &'static str,
    parameters: serde_json::Value,
}

impl Schema {
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

    /// What the tool does, as the model reads it.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        self.description
    }

    /// The arguments' JSON Schema, an object schema.
    #[must_use]
    pub const fn parameters(&self) -> &serde_json::Value {
        &self.parameters
    }
}

/// One tool as the model is offered it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolSpec<'a> {
    /// The name the model calls it by.
    pub name: &'a str,
    /// What the tool does.
    pub description: &'a str,
    /// The arguments' JSON Schema.
    pub parameters: &'a serde_json::Value,
}
