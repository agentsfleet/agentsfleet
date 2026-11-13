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

/// When every fixture row was stored.
const STORED_AT: jiff::Timestamp = jiff::Timestamp::constant(1_760_000_000, 0);

/// The tenant every fixture row belongs to.
fn tenant() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-000000000001").expect("the fixture tenant is canonical")
}

/// A stored row, priced and credentialled unless a test says otherwise.
fn entry(id: &str, model: &str) -> Entry {
    Entry {
        id: Uuid7::parse(id).expect("the fixture entry id is canonical"),
        model_name: model.into(),
        secret_ref: "openai-prod".into(),
        workspace_id: None,
        created_at: STORED_AT,
    }
}

fn rate() -> CatalogueRate {
    CatalogueRate {
        metadata: af_core::catalogue::Metadata {
            display_name: "GPT fixture".into(),
            context_cap_tokens: 200_000,
            context_source_url: "https://example.test/models".into(),
            context_verified_at: STORED_AT,
            advanced: false,
        },
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
    assert_eq!(rendered.created_at, STORED_AT);
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

    assert_eq!(response.models, [] as [ModelEntryRow<'_>; 0]);
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
        created_at: STORED_AT,
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
    let decoded: Cursor = struct_cursor::parse(&token).expect("the page issues a parseable token");

    assert_eq!(decoded.v, struct_cursor::VERSION);
    assert_eq!(decoded.created_at, boundary.created_at);
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
                model: "claude-sonnet-5".into(),
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
                model: "claude-haiku-4-5".into(),
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
