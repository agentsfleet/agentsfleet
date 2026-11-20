//! `thread.threads.resume_context`: the context a fleet's next run resumes from.
//!
//! A different table from the turns, with a different lifetime: a turn records
//! that work happened, this row records where the conversation got to, and
//! only the one report statement writes it. It stays here because it is the
//! runner plane's, and `afd_turns` has no operator surface that reads it.

/// The byte ceiling on a stored checkpoint response.
///
/// Four times `afd_turns::view::sql::MAX_FAILURE_DETAIL_BYTES` because this one is
/// the resume cursor a session continues from, where a cause line only has to
/// be readable.
pub const MAX_CHECKPOINT_RESPONSE_BYTES: usize = 2048;
