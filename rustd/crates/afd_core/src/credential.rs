//! Field names inside a stored credential handle.
//!
//! # Why these are not in the crate that exchanges them
//!
//! A vault handle is written by one plane and read by another: the broker
//! posts a refresh token to a provider and the vault writes the replacement
//! back. Both name the same JSON field, and while the constant lived in the
//! broker the vault had to reach UP into it — the one import that kept
//! `credential` and `vault` in a dependency cycle after every other edge
//! between them was gone.
//!
//! Here for the reason [`crate::event`]'s column spellings are here: a word two
//! planes must agree on belongs below both of them, not in whichever one
//! happened to write it first.

/// The vault-handle field carrying the refresh token to exchange.
///
/// RFC 6749 names both sides of this wire identically, so it is also the
/// response field a rotated replacement arrives in — and the field a broker's
/// cache identity excludes, which is why it is declared once rather than
/// spelled at each of the three sites (RULE UFS).
pub const FIELD_REFRESH_TOKEN: &str = "refresh_token";
