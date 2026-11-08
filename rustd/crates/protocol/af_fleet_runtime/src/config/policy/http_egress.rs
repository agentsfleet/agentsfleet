//! Authored HTTP destination scope, independent of sandbox networking.

/// Which public HTTPS destinations a definition admits through HTTP tools.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionHttpEgress))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpEgress {
    /// Use additional allowed domains and the optional package registry preset.
    #[default]
    Restricted,
    /// Admit public destinations while retaining address, credential and request restrictions.
    Public,
}
