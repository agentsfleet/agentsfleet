//! What a person sends to create a fleet or change one.

use af_validate::{TEXT_MAX_CHARS, nul_free};
use serde::Deserialize;

/// The longest fleet name, in bytes: `af_fleet_runtime::name`'s bound, which
/// the name's URL segments, log scopes and datastore keys share.
const NAME_MAX_BYTES: usize = 64;

/// `POST /v1/workspaces/{workspace_id}/fleets`: a person's first message makes
/// an active fleet and its first turn in one transaction.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateFleetRequest {
    /// What to say to the new fleet: 1 to 1,048,576 characters, with no NUL
    /// character, counted on the decoded text.
    #[garde(length(chars, min = 1, max = TEXT_MAX_CHARS), custom(nul_free))]
    pub message: String,
    /// Required retry identifier: a version 7 universally unique identifier
    /// (UUID). A retry with the same `operation_id` and request answers the
    /// first fleet; the same key with other text answers 409 `UZ-SETUP-005`.
    #[cfg_attr(feature = "openapi", schema(min_length = 36, max_length = 36))]
    #[garde(
        length(bytes, min = 36, max = 36),
        pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$")
    )]
    pub operation_id: String,
    /// The workspace model to use; the workspace's selected model when omitted.
    #[serde(default, deserialize_with = "crate::present")]
    #[cfg_attr(feature = "openapi", schema(nullable = false, min_length = 1))]
    #[garde(inner(length(chars, min = 1, max = TEXT_MAX_CHARS), custom(nul_free)))]
    pub model_ref: Option<String>,
    /// Validated instance slug; `fleet-` and the last 12 hex digits of the new
    /// fleet's id when omitted.
    #[serde(default, deserialize_with = "crate::present")]
    #[cfg_attr(
        feature = "openapi",
        schema(nullable = false, min_length = 1, max_length = 64)
    )]
    #[garde(inner(length(bytes, min = 1, max = NAME_MAX_BYTES)))]
    pub name: Option<String>,
}

/// `PATCH` a fleet under its required resource tag. An omitted field keeps its
/// value; an explicit `null` is refused. Only a signed-in person may change
/// `http_egress` or `package_domains`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct PatchFleetRequest {
    /// `archived` archives the fleet. Terminal.
    #[serde(default, deserialize_with = "crate::present")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    #[garde(skip)]
    pub status: Option<PatchStatus>,
    /// Validated instance slug.
    #[serde(default, deserialize_with = "crate::present")]
    #[cfg_attr(
        feature = "openapi",
        schema(nullable = false, min_length = 1, max_length = 64)
    )]
    #[garde(inner(length(bytes, min = 1, max = NAME_MAX_BYTES)))]
    pub name: Option<String>,
    /// The workspace model the fleet's next run uses.
    #[serde(default, deserialize_with = "crate::present")]
    #[cfg_attr(feature = "openapi", schema(nullable = false, min_length = 1))]
    #[garde(inner(length(chars, min = 1, max = TEXT_MAX_CHARS), custom(nul_free)))]
    pub model_ref: Option<String>,
    /// `public` lets the fleet's HTTP tools reach any public host;
    /// `restricted` keeps them to the hosts of its approved access.
    #[serde(default, deserialize_with = "crate::present")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    #[garde(skip)]
    pub http_egress: Option<af_wire::policy::HttpEgress>,
    /// Whether the fleet's HTTP tools may also reach the package registries.
    #[serde(default, deserialize_with = "crate::present")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    #[garde(skip)]
    pub package_domains: Option<bool>,
}

impl PatchFleetRequest {
    /// Whether the patch changes a network toggle, which only a signed-in
    /// person may do.
    #[must_use]
    pub const fn opens_network(&self) -> bool {
        self.http_egress.is_some() || self.package_domains.is_some()
    }
}

/// The lifecycle target a person may request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchStatus {
    /// Archive the fleet. Terminal.
    Archived,
}

impl PatchStatus {
    /// Existing stored lifecycle vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Archived => "archived",
        }
    }
}

#[cfg(test)]
mod tests;
