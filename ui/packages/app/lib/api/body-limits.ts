// The largest request body the daemon reads, so the app's proxies buffer no
// more than the daemon would take. It is derived in Rust from the one text
// ceiling, 1,048,576 characters, at the widest escape a client may send: an
// astral character as a twelve-byte surrogate pair;
// `rustd/crates/daemon/afd_http/tests/fixtures/body_limits.json` states it,
// and a test on each side reads that file.

/** A body carrying one text at the ceiling, such as a fleet's first message or
 * a message typed into a running fleet
 * (`afd_http::handler::setup_input::TEXT_BODY_BYTES_MAX`). Next's proxy
 * buffers this much of one request body (`proxyClientMaxBodySize`): past it,
 * Next hands the route a truncated body without an error. */
export const TEXT_BODY_LIMIT_BYTES = 12_599_296;
