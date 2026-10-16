//! Platform curation owns display copy and visibility, never private chat.

use super::LibraryCard;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Metadata returned only through the separately scoped operator plane.
#[derive(Debug, Serialize)]
pub struct PlatformLibrary {
    /// Native revision metadata without instructions or private content.
    pub card: LibraryCard,
    /// Explicit platform publication state.
    pub published: bool,
    /// Last mutation, in Unix milliseconds.
    pub updated_at: i64,
}

/// Conditional operator-owned metadata edit.
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct PlatformPatch {
    /// Display label; an absent field preserves it.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(chars, min = 1, max = super::MAX_TITLE_CHARS), custom(afd_validate::rules::nul_free)))]
    pub name: Option<String>,
    /// Curated description, independent of immutable authored content.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, max = 2000), custom(afd_validate::rules::nul_free)))]
    pub description: Option<String>,
    /// Explanations keyed by credential name, containing no credential values.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(custom(reasons))]
    pub required_credentials_reasons: Option<BTreeMap<String, String>>,
    /// Publish or withdraw an already staged native entry.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(skip)]
    pub published: Option<bool>,
}

#[derive(garde::Validate)]
struct Reason<'a> {
    #[garde(
        length(bytes, min = 1, max = 200),
        custom(afd_validate::rules::nul_free)
    )]
    credential: &'a str,
    #[garde(length(bytes, max = 500), custom(afd_validate::rules::nul_free))]
    text: &'a str,
}

#[expect(clippy::ref_option, reason = "garde passes the field by reference")]
fn reasons<C>(value: &Option<BTreeMap<String, String>>, _context: &C) -> garde::Result {
    use garde::Validate as _;
    #[derive(garde::Validate)]
    struct Count<'a> {
        #[garde(length(max = 32))]
        items: &'a BTreeMap<String, String>,
    }
    if let Some(items) = value {
        Count { items }
            .validate()
            .map_err(|_invalid| garde::Error::new("too many credential reasons"))?;
        for (credential, text) in items {
            Reason { credential, text }
                .validate()
                .map_err(|_invalid| garde::Error::new("invalid credential reason"))?;
        }
    }
    Ok(())
}
