//! The registry's wire rendering: a row, a page, and the platform default.
//!
//! Split from the handler half at the file cap. The seam is real rather than
//! arithmetic — everything here turns a store type into a borrowed wire type
//! and decides nothing, so a refusal can never be introduced by an edit in
//! this file.

use afd_core::id::Uuid7;
use afd_core::paging::struct_cursor;
use afd_credential::provider::{PricedDefault, RegistryPage, RegistryRow};
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
        provider: entry
            .credential
            .as_ref()
            .and_then(|held| held.provider.as_deref()),
        kind: entry
            .credential
            .as_ref()
            .map_or(afd_vault::Kind::CustomSecret.as_str(), |held| {
                held.kind.as_str()
            }),
        base_url: entry
            .credential
            .as_ref()
            .and_then(|held| held.base_url.as_deref()),
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
