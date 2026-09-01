//! The registry's wire rendering: a row, a page, and the platform default.
//!
//! Split from the handler half at the file cap. The seam is real rather than
//! arithmetic — everything here turns a store type into a borrowed wire type
//! and decides nothing, so a refusal can never be introduced by an edit in
//! this file.

use afd_core::id::Uuid7;
use afd_core::paging::struct_cursor;
use afd_credential::provider::{PricedDefault, RegistryPage, RegistryRow};
use afd_vault::Descriptor;
use afd_wire::tenant_model_entry::{
    ModelEntriesResponse, ModelEntryRow, PlatformDefaultRow, StoredModelEntry,
};

use super::Cursor;

/// The written row, rendered.
pub(super) fn stored(entry: &afd_credential::provider::Entry) -> StoredModelEntry<'_> {
    StoredModelEntry {
        id: entry.id.as_str(),
        model_id: &entry.model_id,
        secret_ref: &entry.secret_ref,
        created_at: entry.created_at_ms,
    }
}

/// The page, rendered.
pub(super) fn rendered<'p>(
    page: &'p RegistryPage,
    tenant: &Uuid7,
    limit: u32,
) -> ModelEntriesResponse<'p> {
    ModelEntriesResponse {
        models: page.rows.iter().map(row).collect(),
        // Always null: counting a keyset page costs the scan this pagination
        // exists to avoid, and the key stays present rather than vanishing.
        total: None,
        next_cursor: page.next.as_ref().map(|boundary| {
            struct_cursor::render(&Cursor {
                v: struct_cursor::VERSION,
                created_at: boundary.created_at_ms,
                id: boundary.id.as_str().to_owned(),
                tenant_uuid: tenant.as_str().to_owned(),
                limit,
            })
        }),
        platform_default_available: page.platform_default.is_some(),
        platform_default: page.platform_default.as_ref().map(default_row),
    }
}

/// One row, rendered.
///
/// A credential the vault could not describe degrades to an opaque secret with
/// no key and sheds its descriptors — the same shape the workspace secret list
/// gives a row it cannot label, and the reason a dangling reference lists at
/// all instead of failing the page.
fn row(entry: &RegistryRow) -> ModelEntryRow<'_> {
    let rate = entry.rate.as_ref();
    ModelEntryRow {
        id: entry.entry.id.as_str(),
        model_id: &entry.entry.model_id,
        secret_ref: &entry.entry.secret_ref,
        provider: entry.credential.as_ref().and_then(Descriptor::provider),
        kind: entry
            .credential
            .as_ref()
            .map_or(afd_vault::Kind::CustomSecret.as_str(), |held| {
                held.kind().as_str()
            }),
        base_url: entry.credential.as_ref().and_then(Descriptor::base_url),
        has_key: entry.credential.as_ref().is_some_and(|held| held.has_key),
        context_cap_tokens: rate.map(|rate| rate.context_cap_tokens),
        input_nanos_per_mtok: rate.map(|rate| rate.input_nanos_per_mtok),
        cached_input_nanos_per_mtok: rate.map(|rate| rate.cached_input_nanos_per_mtok),
        output_nanos_per_mtok: rate.map(|rate| rate.output_nanos_per_mtok),
        active: entry.active,
        created_at: entry.entry.created_at_ms,
    }
}

/// The platform default, rendered.
fn default_row(priced: &PricedDefault) -> PlatformDefaultRow<'_> {
    let rate = priced.rate.as_ref();
    PlatformDefaultRow {
        provider: &priced.default.provider,
        model: &priced.default.model,
        context_cap_tokens: priced.default.context_cap_tokens,
        input_nanos_per_mtok: rate.map(|rate| rate.input_nanos_per_mtok),
        cached_input_nanos_per_mtok: rate.map(|rate| rate.cached_input_nanos_per_mtok),
        output_nanos_per_mtok: rate.map(|rate| rate.output_nanos_per_mtok),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking on an unmet precondition"
    )]

    use super::*;
    use afd_credential::provider::{Boundary, CatalogueRate, Entry, PlatformDefault};

    /// The context ceiling the fixture's platform default carries.
    ///
    /// Named because the write and the assertion are the same fact: a literal
    /// spelled twice can drift on one side and still read as a passing test.
    const DEFAULT_CAP_TOKENS: u32 = 1_000_000;

    /// The tenant every fixture row belongs to.
    fn tenant() -> Uuid7 {
        Uuid7::parse("019329c5-0000-7000-8000-000000000001")
            .expect("the fixture tenant is canonical")
    }

    /// A stored row, priced and credentialled unless a test says otherwise.
    fn entry(id: &str, model: &str) -> Entry {
        Entry {
            id: Uuid7::parse(id).expect("the fixture entry id is canonical"),
            model_id: model.into(),
            secret_ref: "openai-prod".into(),
            created_at_ms: 1_760_000_000_000,
        }
    }

    fn rate() -> CatalogueRate {
        CatalogueRate {
            context_cap_tokens: 200_000,
            input_nanos_per_mtok: 3_000,
            cached_input_nanos_per_mtok: 300,
            output_nanos_per_mtok: 15_000,
        }
    }

    fn descriptor() -> Descriptor {
        Descriptor {
            classified: afd_vault::Classified::CustomEndpoint {
                provider: "openai".into(),
                base_url: Some("https://api.openai.com".into()),
            },
            has_key: true,
        }
    }

    /// The write path echoes the row it stored, field for field.
    #[test]
    fn should_render_a_written_row_from_the_stored_entry() {
        let written = entry("019329c5-0000-7000-8000-0000000000e1", "gpt-5.1");
        let rendered = stored(&written);

        assert_eq!(rendered.id, written.id.as_str());
        assert_eq!(rendered.model_id, "gpt-5.1");
        assert_eq!(rendered.secret_ref, "openai-prod");
        assert_eq!(rendered.created_at, 1_760_000_000_000);
    }

    /// A fully described row carries its vault labels and its catalogue prices.
    #[test]
    fn should_render_a_described_row_with_its_credential_and_rate() {
        let page = RegistryPage {
            rows: vec![RegistryRow {
                entry: entry("019329c5-0000-7000-8000-0000000000e1", "gpt-5.1"),
                credential: Some(descriptor()),
                rate: Some(rate()),
                active: true,
            }],
            next: None,
            platform_default: None,
        };

        let response = rendered(&page, &tenant(), 25);
        let row = response
            .models
            .first()
            .expect("the page carries its one row");

        assert_eq!(row.provider, Some("openai"));
        assert_eq!(row.kind, afd_vault::Kind::CustomEndpoint.as_str());
        assert_eq!(row.base_url, Some("https://api.openai.com"));
        assert!(row.has_key);
        assert_eq!(row.context_cap_tokens, Some(200_000));
        assert_eq!(row.input_nanos_per_mtok, Some(3_000));
        assert_eq!(row.cached_input_nanos_per_mtok, Some(300));
        assert_eq!(row.output_nanos_per_mtok, Some(15_000));
        assert!(row.active);
    }

    /// A credential the vault cannot describe degrades rather than failing the
    /// page: the row still lists, as an opaque secret holding no key and
    /// carrying none of the descriptors it could not be told.
    ///
    /// This is the dangling-reference case the module note names — a credential
    /// deleted out of band must not cost a tenant the other nineteen rows.
    #[test]
    fn should_degrade_a_row_whose_credential_the_vault_cannot_describe() {
        let page = RegistryPage {
            rows: vec![RegistryRow {
                entry: entry("019329c5-0000-7000-8000-0000000000e2", "claude-opus-5"),
                credential: None,
                rate: None,
                active: false,
            }],
            next: None,
            platform_default: None,
        };

        let response = rendered(&page, &tenant(), 25);
        let row = response
            .models
            .first()
            .expect("the page carries its one row");

        assert_eq!(row.provider, None);
        assert_eq!(row.kind, afd_vault::Kind::CustomSecret.as_str());
        assert_eq!(row.base_url, None);
        assert!(!row.has_key);
        assert_eq!(row.context_cap_tokens, None);
        assert_eq!(row.input_nanos_per_mtok, None);
        assert_eq!(row.cached_input_nanos_per_mtok, None);
        assert_eq!(row.output_nanos_per_mtok, None);
        assert!(!row.active);
    }

    /// A last page carries no cursor, and reports a total of null rather than
    /// dropping the key — counting a keyset page costs the scan the pagination
    /// exists to avoid.
    #[test]
    fn should_render_a_last_page_with_no_cursor_and_a_null_total() {
        let page = RegistryPage {
            rows: Vec::new(),
            next: None,
            platform_default: None,
        };

        let response = rendered(&page, &tenant(), 25);

        assert!(response.models.is_empty());
        assert_eq!(response.total, None);
        assert_eq!(response.next_cursor, None);
        assert!(!response.platform_default_available);
        assert!(response.platform_default.is_none());
    }

    /// The cursor a page hands out is one the walk's own parser accepts, bound
    /// to the tenant and page size it was issued under.
    ///
    /// Rendering and parsing are separate modules, so the claim that binds them
    /// is a round trip rather than either side's own assertion: a token this
    /// page issues must decode to the boundary it was made from.
    #[test]
    fn should_issue_a_cursor_bound_to_its_tenant_and_page_size() {
        let boundary = Boundary {
            created_at_ms: 1_760_000_000_000,
            id: Uuid7::parse("019329c5-0000-7000-8000-0000000000e3")
                .expect("the fixture boundary id is canonical"),
        };
        let page = RegistryPage {
            rows: Vec::new(),
            next: Some(boundary.clone()),
            platform_default: None,
        };

        let response = rendered(&page, &tenant(), 25);
        let token = response
            .next_cursor
            .expect("a page with a next boundary issues a cursor");
        let decoded: Cursor =
            struct_cursor::parse(&token).expect("the page issues a parseable token");

        assert_eq!(decoded.v, struct_cursor::VERSION);
        assert_eq!(decoded.created_at, boundary.created_at_ms);
        assert_eq!(decoded.id, boundary.id.as_str());
        assert_eq!(decoded.tenant_uuid, tenant().as_str());
        assert_eq!(decoded.limit, 25);
    }

    /// The platform default renders priced, and its presence is reported by the
    /// flag the Models page gates its "switch to default" action on.
    #[test]
    fn should_render_a_priced_platform_default_and_flag_it_available() {
        let page = RegistryPage {
            rows: Vec::new(),
            next: None,
            platform_default: Some(PricedDefault {
                default: PlatformDefault {
                    provider: "anthropic".into(),
                    source_workspace_id: Uuid7::parse("019329c5-0000-7000-8000-0000000000b1")
                        .expect("the fixture workspace id is canonical"),
                    model: "claude-sonnet-5".into(),
                    base_url: None,
                    context_cap_tokens: DEFAULT_CAP_TOKENS,
                },
                rate: Some(rate()),
            }),
        };

        let response = rendered(&page, &tenant(), 25);

        assert!(response.platform_default_available);
        let default = response
            .platform_default
            .expect("the flag and the row agree");
        assert_eq!(default.provider, "anthropic");
        assert_eq!(default.model, "claude-sonnet-5");
        assert_eq!(default.context_cap_tokens, DEFAULT_CAP_TOKENS);
        assert_eq!(default.input_nanos_per_mtok, Some(3_000));
        assert_eq!(default.cached_input_nanos_per_mtok, Some(300));
        assert_eq!(default.output_nanos_per_mtok, Some(15_000));
    }

    /// A default the catalogue does not price still renders — the identity is
    /// what the page needs, and an unpriced default is not an absent one.
    #[test]
    fn should_render_an_unpriced_platform_default_without_its_rates() {
        let page = RegistryPage {
            rows: Vec::new(),
            next: None,
            platform_default: Some(PricedDefault {
                default: PlatformDefault {
                    provider: "anthropic".into(),
                    source_workspace_id: tenant(),
                    model: "claude-haiku-4-5".into(),
                    base_url: None,
                    context_cap_tokens: 200_000,
                },
                rate: None,
            }),
        };

        let response = rendered(&page, &tenant(), 25);
        let default = response
            .platform_default
            .expect("an unpriced default still renders");

        assert_eq!(default.model, "claude-haiku-4-5");
        assert_eq!(default.input_nanos_per_mtok, None);
        assert_eq!(default.cached_input_nanos_per_mtok, None);
        assert_eq!(default.output_nanos_per_mtok, None);
    }
}
