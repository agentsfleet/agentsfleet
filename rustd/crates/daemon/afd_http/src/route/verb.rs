//! The HTTP methods a route identity answers.
//!
//! Split from `mod.rs` when that file reached its length cap. The enum was
//! always its own idea — every other type beside it describes what happens
//! BEFORE a handler runs (the guard, the shed class, the capability), where
//! this one describes which requests reach the route at all.

use http::Method;

/// An HTTP verb a route identity serves.
///
/// Kept as a small copyable enum rather than storing [`http::Method`] values in
/// static slices. The inventory is compile-time data, and converting at the
/// router edge is cheaper and clearer than cloning an owned method throughout
/// tests and route metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Verb {
    /// Read a resource or collection.
    Get,
    /// Create beneath a collection.
    Post,
    /// Replace the addressed setting.
    Put,
    /// Partially update the addressed resource.
    Patch,
    /// Remove the addressed resource.
    Delete,
}

impl Verb {
    /// Every verb this daemon can serve, in the order a reader expects.
    ///
    /// `HEAD` is absent for the whole daemon, not merely unlisted here — see
    /// the `HEAD` section of [`crate::route`]'s router documentation. A probe
    /// that walks this slice is therefore walking the complete method surface.
    pub const ALL: &'static [Self] = &[Self::Get, Self::Post, Self::Put, Self::Patch, Self::Delete];

    /// The [`Method`] this verb names.
    ///
    /// One direction only. Nothing needs to go the other way: a request's
    /// method is matched by the router, and this table is read to BUILD
    /// requests and inventories rather than to classify arriving ones.
    #[must_use]
    pub fn method(self) -> Method {
        match self {
            Self::Get => Method::GET,
            Self::Post => Method::POST,
            Self::Put => Method::PUT,
            Self::Patch => Method::PATCH,
            Self::Delete => Method::DELETE,
        }
    }
}
