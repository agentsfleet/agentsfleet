//! Typed error set + resumable-vs-not classification for the Redis layer.
//!
//! `isResumable` mirrors the reference pool's `Protocol.isResumable`
//! shape but enforces classification at compile time via Zig's exhaustive
//! switch: adding a variant to `RedisError` makes `isResumable` fail to
//! compile until the new variant is explicitly classified resumable-or-not.
//! No quiet "did I update the list?" at review time.
//!
//! Consumed by the Client retry layer (recycle vs close on error) and by
//! the typed XADD/XACK error variants in the Client façade. Read timeouts
//! (SO_RCVTIMEO) surface as opaque `ReadFailed` — std.Io.Reader doesn't
//! expose errno, so we don't pretend to distinguish.

/// Transport- and server-level errors the Redis client can surface.
///
/// Resumable variants (server-side errors with the RESP frame boundary
/// intact) recycle the pooled connection; non-resumable variants close
/// the connection and dial fresh on retry.
pub const RedisError = error{
    // ── Server-side — connection stayed in protocol sync ────────────────
    RedisCommandError,
    RedisXaddFailed,
    RedisXackFailed,
    // ── Transport-level — connection no longer trustworthy ──────────────
    BrokenPipe,
    ConnectionResetByPeer,
    ReadFailed,
    WriteFailed,
    /// Server flushed bytes past the parsed RESP reply — the transport
    /// buffer is in protocol desync. The next read on this conn would
    /// start mid-frame. Non-resumable: poison the connection, dial fresh.
    RedisProtocolDesync,
};

/// Resumable = the same connection can serve the next request (server-side
/// error, RESP frame boundary intact). Non-resumable = close the
/// connection; the retry layer dials fresh. Compile-time exhaustive over
/// every `RedisError` variant — a new variant is a compile error here
/// until it joins one of the two arms.
pub fn isResumable(err: RedisError) bool {
    return switch (err) {
        error.RedisCommandError, error.RedisXaddFailed, error.RedisXackFailed => true,
        error.BrokenPipe, error.ConnectionResetByPeer, error.ReadFailed, error.WriteFailed, error.RedisProtocolDesync => false,
    };
}
