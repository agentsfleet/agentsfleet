// The largest request bodies the daemon reads, so the app's proxies buffer no
// more than the daemon would take. Both are derived in Rust from the one text
// ceiling, 1,048,576 characters, at the widest escape a client may send: an
// astral character as a twelve-byte surrogate pair;
// `rustd/crates/daemon/afd_http/tests/fixtures/body_limits.json` states them,
// and a test on each side reads that file.

/** A setup body: a Definition at its cap and a first message at the ceiling
 * (`afd_http::handler::setup_input::SETUP_BODY_BYTES_MAX`). */
export const SETUP_BODY_LIMIT_BYTES = 25_640_960;

/** A body carrying one text at the ceiling, such as a message typed into a
 * running fleet (`afd_http::handler::setup_input::TEXT_BODY_BYTES_MAX`). */
export const TEXT_BODY_LIMIT_BYTES = 12_599_296;

/** What Next's proxy buffers of one request body (`proxyClientMaxBodySize`).
 * Past it, Next hands the route a truncated body without an error, so it must
 * hold the largest body any route here forwards. */
export const PROXY_BODY_LIMIT_BYTES = Math.max(SETUP_BODY_LIMIT_BYTES, TEXT_BODY_LIMIT_BYTES);
