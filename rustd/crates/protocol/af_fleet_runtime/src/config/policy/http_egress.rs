//! Authored HTTP destination scope, independent of sandbox networking.

/// Which public HTTPS destinations a definition admits through HTTP tools.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema), schema(as = DefinitionHttpEgress))]
// Serialize too: the setup tool's schema lists the values this parser reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpEgress {
    /// Use additional allowed domains and the optional package registry preset.
    #[default]
    Restricted,
    /// Admit public destinations while retaining address, credential and request restrictions.
    Public,
}
