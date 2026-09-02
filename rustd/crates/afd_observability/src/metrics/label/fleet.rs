//! What the account, repair and verification paths label their outcomes with.

use crate::metrics::label::closed_set;

/// This one has been seen before, and the first delivery's answer stands.
///
/// One spelling for one fact, shared by the two sets that observe it: a
/// provider result can arrive twice, and so can the event a verification
/// produces. Named rather than written twice so the two cannot be renamed
/// apart while still meaning the same thing.
const REPLAYED: &str = "replayed";

closed_set! {
    /// Why opening an account from a signup delivery did not happen.
    ///
    /// Six, and the count is the point: the first three are the delivery being
    /// wrong and the last three are this daemon being unable to act on a
    /// delivery that was right. An operator seeing a spike needs to know which
    /// half, because only one of them is theirs to fix.
    SignupFailure {
        /// The signature did not verify.
        BadSignature => "bad_sig",
        /// The delivery's timestamp is outside the replay window.
        StaleTimestamp => "stale_ts",
        /// The payload carried no address to open an account against.
        MissingEmail => "missing_email",
        /// The database refused the write.
        DatabaseError => "db_error",
        /// No connection was available to attempt it on.
        PoolUnavailable => "pool_unavailable",
        /// The account opened and the provider would not record that it had.
        MetadataWriteback => "metadata_writeback",
    }
}

closed_set! {
    /// What became of one inbound provider result.
    ProviderResult {
        /// Taken, and it produced a repair.
        Accepted => "accepted",
        /// Seen before; the first delivery's answer stands.
        Replayed => REPLAYED,
        /// Dropped because it normalises to nothing this daemon acts on.
        IgnoredNormalization => "ignored_normalization",
        /// Dropped because it names a repository this deployment does not hold.
        IgnoredRepository => "ignored_repository",
    }
}

closed_set! {
    /// Whether a result could be tied to the repair that caused it.
    ///
    /// `Ambiguous` is its own member rather than folded into `Missed`: a result
    /// matching several repairs is a correlation this daemon declines to guess
    /// at, and one matching none is a result it has no repair for. They are
    /// different investigations.
    Correlation {
        /// Exactly one repair matched.
        Matched => "matched",
        /// No repair matched.
        Missed => "missed",
        /// More than one matched, so none was chosen.
        Ambiguous => "ambiguous",
    }
}

closed_set! {
    /// Whether a verification event was appended or already there.
    SyntheticEvent {
        /// Appended by this pass.
        Emitted => "emitted",
        /// The append-once key answered with an earlier pass's event.
        Replayed => REPLAYED,
    }
}

closed_set! {
    /// Where a verification run got to.
    VerifierRun {
        /// Dispatched onto the fleet's stream.
        Queued => "queued",
        /// Recorded as having produced its event.
        Completed => "completed",
    }
}
