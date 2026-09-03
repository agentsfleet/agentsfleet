//! `/v1/workspaces/{workspace_id}/fleet-libraries` — the gallery a workspace browses.
//!
//! The platform catalogue and this workspace's own entries, as one page. Which
//! library a card came from is its `visibility`, and that field name is shared
//! with a different fact on the admin surface — see [`crate::admin`], where
//! `visibility` is a publication state. The collision is inherited: renaming a
//! shipped v1 field is what `docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids.
//!
//! # Nulls stay on this wire, unlike the tenant registry's
//!
//! The Zig serializes this page with default options, so an absent value is
//! emitted as `null` rather than omitted. That is the opposite of the model
//! registry beside it, and both are deliberate: this page's optional keys are
//! `total` and `next_cursor`, which §3 requires PRESENT on every page, and it
//! has no per-card key whose absence a client union narrows on.
//!
//! # There is no field for bundle content, and no key that would fit one
//!
//! No skill body, no support-file bytes, no object-store key. A read cannot
//! leak what a card has nowhere to carry.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::admin::AdminLibraryRequirements;

/// One gallery card.
//
// `requirements` is the admin surface's type on purpose rather than by
// accident: the two responses share an `OpenAPI` schema, and the Zig's own note
// records that emitting a different shape here made two documented-identical
// payloads disagree about their contents. One type is what stops that.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GalleryCard<'a> {
    /// The entry's identifier — a slug from the platform tier, a UUID from the
    /// workspace's own. Opaque to a caller either way.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// The display name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// The summary from the bundle's own frontmatter.
    #[serde(borrow)]
    pub description: Cow<'a, str>,
    /// Which library the card came from: `platform` or `tenant`.
    #[serde(borrow)]
    pub visibility: Cow<'a, str>,
    /// The repository or template it was onboarded from.
    #[serde(borrow)]
    pub source_ref: Cow<'a, str>,
    /// When it was onboarded, in epoch milliseconds.
    pub created_at: i64,
    /// Credential, tool and host names. Never values.
    #[serde(borrow)]
    pub requirements: AdminLibraryRequirements<'a>,
    /// Per-credential "why this fleet needs it" copy, keyed by name.
    //
    // An empty object for a workspace's own entry: that library derives no
    // reasons, and the read projects `{}` rather than a null so a client has
    // one shape to render for both tiers.
    pub required_credentials_reasons: Value,
}

/// `GET /v1/workspaces/{workspace_id}/fleet-libraries` — one page of the gallery.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GalleryResponse<'a> {
    /// The cards, newest first, platform before tenant within an instant.
    #[serde(borrow)]
    pub items: Vec<GalleryCard<'a>>,
    /// Always null.
    //
    // Counting a keyset page costs the scan the pagination exists to avoid,
    // and §3 declares null to mean "not computed" rather than letting the key
    // vanish from the envelope.
    pub total: Option<u64>,
    /// Where the next page resumes, or null on the last one.
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{GalleryCard, GalleryResponse};
    use crate::admin::AdminLibraryRequirements;
    use serde_json::{Value, json};
    use std::borrow::Cow;

    #[test]
    fn an_empty_page_still_carries_both_navigation_keys() {
        // A client should not have to branch on their absence to tell "no more
        // pages" from "this server is old".
        let body = serde_json::to_string(&GalleryResponse {
            items: vec![],
            total: None,
            next_cursor: None,
        })
        .expect("the page serializes");

        assert_eq!(body, r#"{"items":[],"total":null,"next_cursor":null}"#);
    }

    #[test]
    fn a_card_carries_its_tier_as_visibility_and_no_bundle_content() {
        let body = serde_json::to_string(&GalleryCard {
            id: Cow::Borrowed("reviewer"),
            name: Cow::Borrowed("Reviewer"),
            description: Cow::Borrowed("Reviews code"),
            visibility: Cow::Borrowed("platform"),
            source_ref: Cow::Borrowed("agentsfleet/reviewer"),
            created_at: 1_777_507_200_000,
            requirements: AdminLibraryRequirements {
                credentials: vec![Cow::Borrowed("GITHUB_TOKEN")],
                tools: vec![Cow::Borrowed("bash")],
                network_hosts: vec![Cow::Borrowed("api.github.com")],
                trigger_present: true,
            },
            required_credentials_reasons: json!({"GITHUB_TOKEN": "to open a pull request"}),
        })
        .expect("the card serializes");

        // The order is the Zig's, and the absences are the point: no
        // skill_markdown, no support_files, no content_hash, no snapshot key.
        assert_eq!(
            body,
            r#"{"id":"reviewer","name":"Reviewer","description":"Reviews code","visibility":"platform","source_ref":"agentsfleet/reviewer","created_at":1777507200000,"requirements":{"credentials":["GITHUB_TOKEN"],"tools":["bash"],"network_hosts":["api.github.com"],"trigger_present":true},"required_credentials_reasons":{"GITHUB_TOKEN":"to open a pull request"}}"#
        );
        for leaked in ["skill_markdown", "support_files", "content_hash"] {
            assert!(!body.contains(leaked), "the card carries {leaked}");
        }
    }

    #[test]
    fn a_workspace_entry_renders_an_empty_reasons_object_rather_than_a_null() {
        // One shape for both tiers, so a client renders the chips the same way
        // whichever library the card came from.
        let body = serde_json::to_string(&GalleryCard {
            id: Cow::Borrowed("0195b4ba-8d3a-7f13-8abc-cd0000000002"),
            name: Cow::Borrowed("Internal"),
            description: Cow::Borrowed("Ours"),
            visibility: Cow::Borrowed("tenant"),
            source_ref: Cow::Borrowed("acme/internal"),
            created_at: 1,
            requirements: AdminLibraryRequirements {
                credentials: vec![],
                tools: vec![],
                network_hosts: vec![],
                trigger_present: false,
            },
            required_credentials_reasons: Value::Object(serde_json::Map::new()),
        })
        .expect("the card serializes");

        assert!(body.contains(r#""required_credentials_reasons":{}"#));
        assert!(body.contains(r#""visibility":"tenant""#));
    }
}
