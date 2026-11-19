//! `thread.threads.resume_context`: the context a fleet's next run resumes from.
//!
//! Split from the event statements when the event table moved to
//! `afd_turns`. This is a DIFFERENT table with a different lifetime: an event
//! row records that work happened, this row records where the conversation got
//! to, and only the one report statement writes it. It stayed here
//! because it is the runner plane's, and moving it would have given the event
//! crate a table no operator surface reads.

/// The byte ceiling on a stored checkpoint response.
///
/// Four times `afd_turns::view::sql::MAX_FAILURE_DETAIL_BYTES` because this one is
/// the resume cursor a session continues from, where a cause line only has to
/// be readable.
pub const MAX_CHECKPOINT_RESPONSE_BYTES: usize = 2048;
