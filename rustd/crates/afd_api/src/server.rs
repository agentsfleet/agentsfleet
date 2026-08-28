//! The connection policy this daemon serves under, on both protocol versions.
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

use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto;

/// Room for a request's status line and headers.
///
/// `MAX_REQUEST_HEADER_BYTES` in `http/server.zig`, to the byte. A request
/// whose head does not fit is answered `431 Request Header Fields Too Large`
/// rather than read without bound.
pub const MAX_REQUEST_HEADER_BYTES: usize = 16 * 1024;

/// The largest frame this server will read on an HTTP/2 stream.
///
/// The HTTP/1 ceiling above, applied to the protocol that has its own word for
/// it. Not the same GUARANTEE — h2 bounds headers with
/// `SETTINGS_MAX_HEADER_LIST_SIZE` and this bounds a DATA frame — but the same
/// posture: a client must not be able to make this server allocate without a
/// stated bound.
///
/// Spelled as its own literal rather than cast from the sibling above, because
/// h2's settings are 32-bit by protocol and a cast would be a truncation the
/// compiler is right to flag. The two are asserted equal below, so they cannot
/// drift.
pub const MAX_FRAME_BYTES: u32 = 16 * 1024;

// The one fact both constants state, checked at compile time rather than
// written twice and hoped about.
const _: () = assert!(MAX_FRAME_BYTES as usize == MAX_REQUEST_HEADER_BYTES);

/// The connection builder every listener in this daemon serves with.
///
/// Exposed as a builder rather than applied inside an accept loop because the
/// accept loop belongs to boot, and the POLICY belongs here with the route
/// table and the admission ceiling — the three facts about how this server
/// answers before a handler is chosen.
///
/// # Why the protocol is detected rather than fixed
///
/// A browser opens at most six concurrent HTTP/1.1 connections per origin, and
/// every live event stream holds one of them for as long as a tab is open. A
/// dashboard with a handful of tabs therefore starves its own origin — the
/// stream surface would work and everything else on the page would queue behind
/// it. HTTP/2 multiplexes every request onto one connection and the cap stops
/// being a fact about the product.
///
/// So this is `auto`, not `http1`: it reads the connection preface and serves
/// whichever protocol arrived. HTTP/1.1 clients are unaffected — the curl in
/// every runbook, the stock runner, the Node proxy — and a browser that offers
/// h2 gets it. Prior-knowledge h2c, which is what a proxy terminating TLS in
/// front of this server forwards.
#[must_use]
pub fn connection_builder() -> auto::Builder<TokioExecutor> {
    let mut builder = auto::Builder::new(TokioExecutor::new());
    builder.http1().max_buf_size(MAX_REQUEST_HEADER_BYTES);
    builder
        .http2()
        .max_header_list_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES);
    // hyper's own default is ~400 KB and nothing announces that it changed, so
    // a bound this important is worth one line an operator can grep for when a
    // client starts getting 431s it did not get from the Zig daemon.
    tracing::debug!(
        max_request_header_bytes = MAX_REQUEST_HEADER_BYTES,
        max_frame_bytes = MAX_FRAME_BYTES,
        event = "http_connection_policy_set",
        "connection policy set for both protocol versions"
    );
    builder
}
