// The largest request bodies the daemon reads, so the app's proxies buffer no
// more than the daemon would take. Both are derived in Rust from the one text
// ceiling, 1,048,576 characters, at JSON's widest escape of six bytes a
// character; `rustd/crates/daemon/afd_http/tests/fixtures/body_limits.json`
// states them, and a test on each side reads that file.

/** A setup body: a Definition at its cap and a first message at the ceiling
 * (`afd_http::handler::setup_input::SETUP_BODY_BYTES_MAX`). */
export const SETUP_BODY_LIMIT_BYTES = 12_861_440;

/** A body carrying one text at the ceiling, such as a message typed into a
 * running fleet (`afd_http::handler::setup_input::TEXT_BODY_BYTES_MAX`). */
export const TEXT_BODY_LIMIT_BYTES = 6_307_840;
