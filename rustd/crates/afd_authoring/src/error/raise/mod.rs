//! Context belongs at the fallible boundary; causes stay typed inside the hull.

use super::{Error, ErrorKind};

pub(crate) fn query(context: &'static str) -> impl FnOnce(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

pub(crate) fn invalid(detail: &'static str) -> Error {
    ErrorKind::Invalid { detail }.into()
}

/// Stored drafts, messages and candidates hold private text, and serde quotes
/// the value it refused, so the cause keeps only its position and expected type.
pub(crate) fn stored(source: serde_json::Error) -> Error {
    ErrorKind::Stored {
        source: afd_core::json::Unreadable::from(source),
    }
    .into()
}

#[cfg(test)]
mod tests;
