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

/// The vault-handle field naming the Zoho accounts server a refresh token is
/// redeemable at.
///
/// The connect flow writes it and the refresh exchange reads it, so it is
/// declared once, below both (RULE UFS).
pub const FIELD_ACCOUNTS_BASE: &str = "accounts_base";

/// The path every Zoho accounts server serves the token exchange on.
pub const ZOHO_TOKEN_PATH: &str = "/oauth/v2/token";

/// Every Zoho accounts server, one per data centre, United States first.
///
/// The connect flow writes one of these onto a handle and the refresh exchange
/// posts the platform's client secret to nothing else, so both read this one
/// list: a data centre added here is written and accepted together, and the
/// connector's centre enum indexes it, so a centre added there without a server
/// here fails the build. Canada is the irregular entry: a different apex
/// domain, as Zoho documents it.
pub const ZOHO_ACCOUNTS_SERVERS: [&str; 7] = [
    "https://accounts.zoho.com",
    "https://accounts.zoho.eu",
    "https://accounts.zoho.in",
    "https://accounts.zoho.com.au",
    "https://accounts.zoho.com.cn",
    "https://accounts.zoho.jp",
    "https://accounts.zohocloud.ca",
];
