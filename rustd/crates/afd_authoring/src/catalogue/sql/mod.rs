//! One owner-scoped statement reads configured choices and current public pricing.

/// The owner's tenant models and the platform default, each priced from the
/// catalogue, in reference order.
///
/// `$1` owner tenant, `$2` the platform reference, `$3` cursor reference,
/// `$4` limit, `$5` the OpenAI-compatible provider, which needs no stored key.
pub(super) const PAGE: &str = include_str!("page.sql");
