//! Wire shapes for platform Fleet-library and bundle administration.
//!
//! Split from [`admin_catalogue`](crate::admin_catalogue), which names the
//! priced-model half. Both reach callers through [`crate::admin`], so the two
//! files are a reading convenience and not a boundary any consumer sees.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Platform Fleet-library onboarding request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryImport<'a> {
    /// `upload`, `github`, or first-party `template`.
    #[serde(borrow)]
    pub source_kind: Cow<'a, str>,
    /// Repository, template id, or upload provenance.
    #[serde(borrow)]
    pub source_ref: Cow<'a, str>,
    /// Optional GitHub branch, tag, or commit.
    #[serde(borrow, rename = "ref")]
    pub revision: Option<Cow<'a, str>>,
    /// Explicitly permits replacing a slug owned by another source.
    pub replace: bool,
    /// Inline root document for uploads.
    #[serde(borrow)]
    pub skill_markdown: Option<Cow<'a, str>>,
    /// Optional inline trigger document for uploads.
    #[serde(borrow)]
    pub trigger_markdown: Option<Cow<'a, str>>,
    /// Attachments are fetched from repositories; inline uploads reject these.
    pub support_files: Vec<serde_json::Value>,
}

/// Content-free requirements shown on one Fleet-library row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryRequirements<'a> {
    /// Credential names only.
    #[serde(borrow)]
    pub credentials: Vec<Cow<'a, str>>,
    /// Required tool names.
    #[serde(borrow)]
    pub tools: Vec<Cow<'a, str>>,
    /// Declared outbound hosts.
    #[serde(borrow)]
    pub network_hosts: Vec<Cow<'a, str>>,
    /// Whether a trigger document exists.
    pub trigger_present: bool,
}

/// One metadata-only platform Fleet-library row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryItem<'a> {
    /// Slug identity.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Curated description.
    #[serde(borrow)]
    pub description: Cow<'a, str>,
    /// GitHub owner/repository.
    #[serde(borrow)]
    pub source_repo: Cow<'a, str>,
    /// Fetched revision.
    #[serde(borrow)]
    pub source_ref: Cow<'a, str>,
    /// Draft or public.
    #[serde(borrow)]
    pub visibility: Cow<'a, str>,
    /// Content identity, never support-file bytes.
    #[serde(borrow)]
    pub content_hash: Option<Cow<'a, str>>,
    /// Derived requirement names and trigger presence.
    #[serde(borrow)]
    pub requirements: AdminLibraryRequirements<'a>,
    /// Operator-authored per-credential reason copy.
    pub required_credentials_reasons: serde_json::Value,
    /// Last mutation instant in epoch milliseconds.
    pub updated_at: i64,
    /// Strong version over the editable row surface.
    #[serde(borrow)]
    pub etag: Cow<'a, str>,
}

/// Admin Fleet-library list response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibrariesResponse<'a> {
    /// Every draft and public row.
    #[serde(borrow)]
    pub entries: Vec<AdminLibraryItem<'a>>,
}

/// Successful platform Fleet-library onboarding.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryCreated<'a> {
    /// Slug derived from `SKILL.md`.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name derived from `SKILL.md`.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Which library the entry now stands in.
    ///
    /// `platform` from the operator's catalogue and `tenant` from a workspace
    /// onboard — both verbs answer this shape, and the tier is what differs.
    #[serde(borrow)]
    pub visibility: Cow<'a, str>,
    /// Content identity of the validated bundle.
    #[serde(borrow)]
    pub content_hash: Cow<'a, str>,
    /// Credential/tool/host names without support-file paths.
    #[serde(borrow)]
    pub requirements: AdminLibraryRequirements<'a>,
}

/// One public Fleet Bundle gallery row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetBundleItem<'a> {
    /// Stable catalogue slug.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Curated summary.
    #[serde(borrow)]
    pub description: Cow<'a, str>,
    /// Credential names, never values.
    #[serde(borrow)]
    pub required_credentials: Vec<Cow<'a, str>>,
    /// Install-gate explanation keyed by credential name.
    pub required_credentials_reasons: serde_json::Value,
    /// Required tool identifiers.
    #[serde(borrow)]
    pub required_tools: Vec<Cow<'a, str>>,
    /// Declared outbound hosts.
    #[serde(borrow)]
    pub network_hosts: Vec<Cow<'a, str>>,
}

/// Public Fleet Bundle gallery response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetBundlesResponse<'a> {
    /// Every published row carrying current bundle content.
    #[serde(borrow)]
    pub items: Vec<FleetBundleItem<'a>>,
}

/// Partial operator edit for one Fleet-library row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Validate)]
#[serde(default)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryPatch<'a> {
    /// Replacement display name.
    #[serde(borrow)]
    // `inner` reaches through the `Option`: absent means "leave it alone" on
    // a PATCH, and only a name that was SENT is bounded.
    #[garde(inner(length(bytes, min = 1, max = LIBRARY_NAME_MAX_BYTES)))]
    pub name: Option<Cow<'a, str>>,
    /// Replacement description.
    #[serde(borrow)]
    #[garde(skip)]
    pub description: Option<Cow<'a, str>>,
    /// Replacement GitHub owner/repository.
    #[serde(borrow)]
    // PARSED rather than bounded — `Repository::parse` proves an `owner/repo`
    // shape that no length can — and it answers a different public code than a
    // length break does, which a report cannot carry.
    #[garde(skip)]
    pub source_repo: Option<Cow<'a, str>>,
    /// Replacement branch or tag.
    #[serde(borrow)]
    // Proven by `valid_revision` for the reason `source_repo` is: a git
    // reference is a grammar, not a size.
    #[garde(skip)]
    pub source_ref: Option<Cow<'a, str>>,
    /// Operator-authored reason copy.
    // Free-form JSON whose keys are credential NAMES an operator chose, so
    // there are no fields for `length` to hang off: the caps below are one
    // custom rule over the whole object.
    #[garde(custom(reasons_bounded))]
    pub required_credentials_reasons: Option<serde_json::Value>,
    /// Publish or withdraw.
    #[garde(skip)]
    pub published: Option<bool>,
}

/// The longest display name a library row carries.
pub const LIBRARY_NAME_MAX_BYTES: usize = 200;

/// How many credentials one library may explain.
pub const REASONS_MAX: usize = 32;

/// The longest credential name a reason may be filed under.
pub const REASON_CREDENTIAL_MAX_BYTES: usize = 200;

/// The longest reason copy an operator may author.
pub const REASON_MAX_BYTES: usize = 500;

/// What [`reasons_bounded`] reports. The route answers its own sentence.
const REASONS_OUT_OF_BOUNDS: &str = "reason copy exceeds the install gate's caps";

/// Holds an operator's reason copy to the install gate's caps.
///
/// At most [`REASONS_MAX`] entries, each credential name at most
/// [`REASON_CREDENTIAL_MAX_BYTES`] and each reason at most
/// [`REASON_MAX_BYTES`]. A value that is not an object of strings is not
/// judged here: that is its SHAPE, which the route reads with a sentence of
/// its own, and a bound has nothing to say about a value it cannot measure.
///
/// # Errors
/// [`REASONS_OUT_OF_BOUNDS`] when the object breaks any of the three caps.
#[expect(
    clippy::ref_option,
    reason = "garde fixes the custom-rule signature at `fn(&T, &C)`, and the field is an `Option`"
)]
fn reasons_bounded<C>(value: &Option<serde_json::Value>, _context: &C) -> garde::Result {
    let Some(reasons) = value.as_ref().and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    let within = reasons.len() <= REASONS_MAX
        && reasons.iter().all(|(credential, reason)| {
            credential.len() <= REASON_CREDENTIAL_MAX_BYTES
                && reason
                    .as_str()
                    .is_none_or(|copy| copy.len() <= REASON_MAX_BYTES)
        });
    if within {
        Ok(())
    } else {
        Err(garde::Error::new(REASONS_OUT_OF_BOUNDS))
    }
}

#[cfg(test)]
#[path = "admin_library/tests.rs"]
mod tests;
