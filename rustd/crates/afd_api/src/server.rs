//! The HTTP/1 connection policy this daemon serves under.
//!
//! # The same number, for the opposite reason
//!
//! `http/server.zig` sets a 16 KiB header allowance because httpz defaults to
//! 4 KiB and that was too TIGHT: a session bearer runs past a kilobyte on its
//! own, every proxy the request crosses appends forwarding and tracing headers,
//! and the dashboard proxy passes an upstream status through verbatim — so a
//! refusal born in the daemon surfaced in a browser as a 431 against a request
//! whose own headers were small.
//!
//! hyper has the opposite default. Its connection buffer is ~400 KB, so nothing
//! legitimate is ever refused — and a client that opens connections and dribbles
//! header bytes into each one can hold four hundred kilobytes apiece before the
//! server gives up. At the thousand-odd connections this daemon accepts, that is
//! hundreds of megabytes of buffer a caller chooses to allocate.
//!
//! So the port keeps the number and inverts the argument: 16 KiB is a CEILING
//! here where it was a FLOOR there. It stays the same number because it is the
//! same fact about the deployment — the Node proxy in front of this server
//! already tolerates 16 KiB, and a server should be neither the tightest limit
//! in a chain nor unbounded.
//!
//! # What else the buffer bounds
//!
//! `max_buf_size` is the connection's whole read buffer, not a header-only one,
//! so bodies are read in chunks no larger than this too. That costs a syscall
//! per 16 KiB on a large upload and changes nothing about what is accepted:
//! bodies stream, and their own limit is a separate concern from this one.

use hyper::server::conn::http1;

/// Room for a request's status line and headers.
///
/// `MAX_REQUEST_HEADER_BYTES` in `http/server.zig`, to the byte. A request
/// whose head does not fit is answered `431 Request Header Fields Too Large`
/// rather than read without bound.
pub const MAX_REQUEST_HEADER_BYTES: usize = 16 * 1024;

/// The connection builder every listener in this daemon serves with.
///
/// Exposed as a builder rather than applied inside an accept loop because the
/// accept loop belongs to boot, and the POLICY belongs here with the route
/// table and the admission ceiling — the three facts about how this server
/// answers before a handler is chosen.
#[must_use]
pub fn http1_builder() -> http1::Builder {
    let mut builder = http1::Builder::new();
    builder.max_buf_size(MAX_REQUEST_HEADER_BYTES);
    // hyper's own default is ~400 KB and nothing announces that it changed, so
    // a bound this important is worth one line an operator can grep for when a
    // client starts getting 431s it did not get from the Zig daemon.
    tracing::debug!(
        max_request_header_bytes = MAX_REQUEST_HEADER_BYTES,
        event = "http1_policy_set",
        "http/1 connection policy set"
    );
    builder
}
