//! Generic page envelopes shared by several resources.
//!
//! utoipa's `ToSchema` derive expands a generic field into
//! `if let Some(..) = generics.get_mut(..) { .. } else { .. }` (utoipa-gen 6.0.1,
//! `src/component.rs:1240`), which Clippy reads as hand-written code. The derived
//! impl is its own item, so only this module can carry the expectation, and
//! `expect` fails the build once utoipa stops generating that shape.
#![cfg_attr(
    feature = "openapi",
    expect(
        clippy::option_if_let_else,
        reason = "utoipa's ToSchema derive expands generic fields into if-let/else"
    )
)]

use serde::Serialize;
use std::borrow::Cow;

/// One page of results. `total` is null when unavailable; `next_cursor` is null on the last
/// page.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct CursorPage<T> {
    /// Current bounded rows.
    pub items: Vec<T>,
    /// Known total, or null.
    pub total: Option<u64>,
    /// Use this value unchanged for the next page, or stop when null.
    pub next_cursor: Option<String>,
}

// Generic over the item, because the envelope is the same for every paged
// resource on these planes and a per-resource copy is a per-resource chance
// for the keys to drift apart. The three keys are pinned by the integration
// test that counts them.
/// One result page with `items`, `total`, and `next_cursor`. A null `next_cursor` means the
/// last page.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageResponse<'a, T> {
    /// The rows on this page, in the requested order.
    pub items: Vec<T>,
    /// Total matching records across all pages. Use `next_cursor` to determine whether
    /// another page exists.
    pub total: i64,
    /// Where the next page resumes, or `null` on the last page.
    ///
    /// Always emitted, never omitted, for the module's null rule.
    pub next_cursor: Option<Cow<'a, str>>,
}
