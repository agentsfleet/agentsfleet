//! The HTTP catalogue seam: the priced model library, read and never written,
//! and the one conversion both API planes use between stored evidence and
//! prices and their wire spelling.

use std::borrow::Cow;

use afd_api_wire::catalogue::{CatalogueMetadata, CataloguePrice};
use afd_core::catalogue::{Metadata, Price};
use afd_tenant::models::{Boundary, LibraryPage};

/// The model catalogue's one read — a bounded page in normalized order.
///
/// Read-only by construction: the admin CRUD that writes these rows is
/// M179's surface, and a trait with no write verb cannot grow one by
/// accident. Takes ALREADY-PARSED values — the filter is normalized and the
/// boundary is a decoded cursor — so there is no validation arm in any
/// implementation, and none a stub could get differently right.
pub trait ModelCatalogue: Send + Sync + std::fmt::Debug + 'static {
    /// One bounded page of the catalogue; advanced models only when
    /// `advanced` asks for them.
    ///
    /// # Errors
    /// Reports a datastore that would not answer — under the library
    /// family's own code — and a row this daemon cannot read.
    fn page(
        &self,
        filter: Option<&str>,
        advanced: bool,
        after: Option<&Boundary>,
        limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<LibraryPage>> + Send;
}

/// The production surface answers it directly.
impl ModelCatalogue for afd_tenant::models::Models {
    fn page(
        &self,
        filter: Option<&str>,
        advanced: bool,
        after: Option<&Boundary>,
        limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<LibraryPage>> + Send {
        Self::page(self, filter, advanced, after, limit)
    }
}

/// A stored model's evidence as either API plane serves it.
#[must_use]
pub fn metadata_wire(metadata: &Metadata) -> CatalogueMetadata<'static> {
    CatalogueMetadata {
        display_name: Cow::Owned(metadata.display_name.clone()),
        context_cap_tokens: metadata.context_cap_tokens,
        context_source_url: Cow::Owned(metadata.context_source_url.clone()),
        context_verified_at: metadata.context_verified_at,
        advanced: metadata.advanced,
    }
}

/// A stored price as either API plane serves it.
#[must_use]
pub fn price_wire(price: &Price) -> CataloguePrice<'static> {
    CataloguePrice {
        effective_from: price.effective_from,
        effective_until: price.effective_until,
        prompt_tokens_above: price.prompt_tokens_above,
        input_nanos_per_mtok: price.input_nanos_per_mtok,
        cached_input_nanos_per_mtok: price.cached_input_nanos_per_mtok,
        cache_write_nanos_per_mtok: price.cache_write_nanos_per_mtok,
        cache_write_5m_nanos_per_mtok: price.cache_write_5m_nanos_per_mtok,
        cache_write_1h_nanos_per_mtok: price.cache_write_1h_nanos_per_mtok,
        output_nanos_per_mtok: price.output_nanos_per_mtok,
        source_url: Cow::Owned(price.source_url.clone()),
        verified_at: price.verified_at,
        conditions: Cow::Owned(price.conditions.clone()),
    }
}

/// Validated admin evidence, in the shape the store writes.
#[must_use]
pub fn metadata_store(metadata: CatalogueMetadata<'_>) -> Metadata {
    Metadata {
        display_name: metadata.display_name.into_owned(),
        context_cap_tokens: metadata.context_cap_tokens,
        context_source_url: metadata.context_source_url.into_owned(),
        context_verified_at: metadata.context_verified_at,
        advanced: metadata.advanced,
    }
}

/// A validated admin price, in the shape the store writes.
#[must_use]
pub fn price_store(price: CataloguePrice<'_>) -> Price {
    Price {
        effective_from: price.effective_from,
        effective_until: price.effective_until,
        prompt_tokens_above: price.prompt_tokens_above,
        input_nanos_per_mtok: price.input_nanos_per_mtok,
        cached_input_nanos_per_mtok: price.cached_input_nanos_per_mtok,
        cache_write_nanos_per_mtok: price.cache_write_nanos_per_mtok,
        cache_write_5m_nanos_per_mtok: price.cache_write_5m_nanos_per_mtok,
        cache_write_1h_nanos_per_mtok: price.cache_write_1h_nanos_per_mtok,
        output_nanos_per_mtok: price.output_nanos_per_mtok,
        source_url: price.source_url.into_owned(),
        verified_at: price.verified_at,
        conditions: price.conditions.into_owned(),
    }
}
