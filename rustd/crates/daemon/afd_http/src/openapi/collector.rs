//! A plane's `OpenAPI` collector, and the per-handler documents that audit it.
//!
//! # Why a collector is not enough to see a name collision
//!
//! utoipa keys components by name alone. A derive gathers every schema its
//! handlers reach into one list and folds that list into a map, so two types
//! publishing one name leave the map holding whichever came last, with no
//! error and no trace of the other. The finished document cannot show it.
//!
//! So [`plane_collector!`](crate::plane_collector) takes the plane's handler
//! roster once and emits two things from it: the collector the plane publishes,
//! and behind the CALLER's `test-util` feature a `handler_documents()` that
//! derives one document per handler. A type reached by two handlers produces
//! the same schema in both, so `afd_api` compares every handler's components
//! and a name carrying two different schemas is a collision. One roster, so the
//! audit cannot cover a different set of handlers than the plane serves.

/// Declares a plane's collector over a handler roster, and its audit.
///
/// The roster is the list utoipa's `paths(...)` takes. Expands, in the calling
/// module, to the collector struct with `#[derive(utoipa::OpenApi)]`, and under
/// the caller's `test-util` feature to `pub fn handler_documents()`, one
/// `OpenAPI` document per handler in roster order.
#[macro_export]
macro_rules! plane_collector {
    (
        $(#[$meta:meta])*
        struct $collector:ident;
        paths($($($segment:ident)::+),+ $(,)?)
    ) => {
        $(#[$meta])*
        #[derive(utoipa::OpenApi)]
        #[openapi(paths($($($segment)::+),+))]
        struct $collector;

        /// One `OpenAPI` document per handler of this plane, in roster order.
        ///
        /// Each carries only the components its own handler reaches, so a
        /// component name two handlers publish differently shows here and
        /// nowhere else. See `afd_http::openapi::collector`.
        #[cfg(feature = "test-util")]
        #[must_use]
        pub fn handler_documents() -> ::std::vec::Vec<utoipa::openapi::OpenApi> {
            ::std::vec![$({
                #[derive(utoipa::OpenApi)]
                #[openapi(paths($($segment)::+))]
                struct One;
                <One as utoipa::OpenApi>::openapi()
            }),+]
        }
    };
}
