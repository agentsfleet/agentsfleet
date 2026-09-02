//! `/v1/auth/sessions*` — the command-line login handshake.
//!
//! Six verbs over one Redis blob, and every one of them is thin by
//! construction: parse the body into a type that already carries its bounds,
//! call one service method, render the answer. There is no validation here, no
//! status chosen at a call site, and no state machine — the machine is
//! [`afd_tenant::session`]'s, and this module is what puts an HTTP request in
//! front of it.
//!
//! # What is deliberately absent
//!
//! A scratch struct. Each Zig handler opens with `var scratch: RequestScratch =
//! undefined` and fills it on the next line, and the four fields it holds are
//! re-derived per verb whether or not that verb needs them. Here the address
//! and the user agent are an [`Origin`](crate::client::Origin) extractor, so a
//! verb that does not name one does not compute one.

pub(crate) mod session;

pub(crate) use self::session::dashboard::{approve, verify};
pub(crate) use self::session::{delete_all, delete_one, open, poll};
