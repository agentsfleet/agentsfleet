//! One owner-scoped statement reads configured choices and current public pricing.

/// The owner's tenant models and the platform default, each priced from the
/// catalogue, in reference order.
///
/// A choice needs a stored key and a priced catalogue row; whether its saved
/// inference settings route is decided in Rust, the same predicate dispatch
/// uses.
///
/// `$1` owner tenant, `$2` the platform reference, `$3` cursor reference,
/// `$4` limit.
pub(super) const PAGE: &str = include_str!("page.sql");
