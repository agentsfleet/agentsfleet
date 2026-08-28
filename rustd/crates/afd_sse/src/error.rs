//! What can go wrong while a stream is being decided.
//!
//! One type for the crate, as `docs/RUST_ERROR_STANDARD.md` requires. It is
//! small on purpose: nothing here reaches Postgres or opens a socket, so the
//! only failure this crate owns is a published payload it cannot forward.
//! Everything else — a hub that closed, a workspace the caller may no longer
//! read — ENDS the stream rather than raising, because a stream that has
//! already sent its headers has no way to answer with a status code.

/// The crate's `Result`, defaulting to [`Error`].
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A frame this crate declined to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The published payload is not a JSON object, so a tag cannot be spliced.
    ///
    /// The multiplex drops the frame instead of emitting one: a `data` line
    /// that is not an object breaks the client's parser for every LATER frame
    /// on the connection, and the durable row is still there to be paged.
    #[error("the published payload is not a JSON object")]
    Untaggable,
}
