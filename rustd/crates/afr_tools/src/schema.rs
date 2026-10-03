//! What the model is told about a tool.

/// A tool's description and its parameters' JSON Schema.
///
/// Provider-neutral: each provider wraps it in its own function-calling wire.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    /// What the tool does, as the model reads it.
    pub description: &'static str,
    /// The arguments' JSON Schema, an object schema.
    pub parameters: serde_json::Value,
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
