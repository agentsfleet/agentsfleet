//! What can go wrong while the metric registry is being built.
//!
//! This crate had no fallible function until the registry arrived, and
//! `docs/RUST_ERROR_STANDARD.md` records it that way. Constructing an
//! instrument set from a declared contract ends that, so the type lands on the
//! commit that makes the crate fallible — a crate is not exempt from the
//! standard for having predated it.
//!
//! # What this type does NOT spell
//!
//! There is no variant for a short row, an unknown token, or a bound that will
//! not parse as a number. Those are `csv`'s to report and it reports them
//! better than a hand-written parser could: every one arrives already carrying
//! the record and line it failed on, the field name, and the closed set of
//! variants serde expected. Composing that by `#[from]` (`M-FROM-ERROR`) keeps
//! the whole chain and leaves this type spelling only what `csv` cannot know —
//! the meaning of a row once it has parsed.

#[cfg(test)]
mod tests;

/// The result every fallible function in this crate returns.
///
/// One alias per crate, defaulted to this crate's own [`Error`], so a reader
/// never has to check WHICH error a signature returns to know it is this one
/// (`RUST_ERROR_STANDARD` rule 1).
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A metric contract this crate declined to build a registry from.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The census would not read as the tab-separated table it declares itself.
    ///
    /// Transparent because the reader's own sentence is already the better one:
    /// it names the record, the line, the field and what it expected there.
    /// Restating any of that here would print it twice and add nothing.
    ///
    /// Worth knowing when reading a chain from this crate: `csv::Error`
    /// implements `source()` as the default `None`, so it terminates the chain
    /// and carries its position in `Display` instead. A walker that stops here
    /// has not lost anything — there was never a further link.
    #[error(transparent)]
    Census(#[from] csv::Error),

    /// Two rows declare the same family name.
    ///
    /// Beyond the reader's reach: each row is individually well-formed, and
    /// only the set knows they collide. Left unreported, the second row would
    /// shadow the first and parity would still grade clean — the earlier row's
    /// unit, bounds and temporality simply gone.
    #[error("the census declares family `{family}` twice, on lines {first} and {second}")]
    Duplicate {
        /// The family both rows name.
        family: Box<str>,
        /// The line of the first declaration.
        first: u64,
        /// The line of the second.
        second: u64,
    },

    /// A row's kind and its bucket bounds contradict each other.
    ///
    /// Also beyond the reader's reach: both columns parse perfectly, and the
    /// defect is the pair. A counter carrying bounds would silently drop them;
    /// a histogram without them would export under SDK defaults, which is a
    /// renamed dashboard rather than a failure.
    #[error("the census declares `{family}` a {kind} carrying {bounds} bucket bounds")]
    BoundsMismatch {
        /// The family whose columns disagree.
        family: Box<str>,
        /// The kind the row declares, in the census's own spelling.
        kind: &'static str,
        /// How many bounds it declares alongside that kind.
        bounds: usize,
    },

    /// A family's Rust type and the census disagree about what kind it is.
    ///
    /// The type claims a kind by which trait it implements, so this cannot be
    /// a caller passing the wrong argument — the compiler settles that. It
    /// means the contract on disk and the code were edited apart, which
    /// otherwise surfaces as an instrument built with the wrong aggregation:
    /// a counter's total rendered as a distribution, or a histogram's buckets
    /// silently discarded.
    #[error("the census declares `{family}` a {declared}, but its type claims a {claimed}")]
    KindMismatch {
        /// The family the two disagree about.
        family: Box<str>,
        /// What the census says, in its own spelling.
        declared: &'static str,
        /// What the Rust type's traits claim.
        claimed: &'static str,
    },

    /// The SDK refused the stream a declared family describes.
    ///
    /// Raised eagerly at registry construction rather than inside a View
    /// closure. The SDK's View signature answers `Option<Stream>`, so a refusal
    /// there is indistinguishable from "this view does not apply" and the
    /// family would export under default buckets with nobody told.
    ///
    /// The SDK's refusal is a `Box<dyn Error>` — not `Send + Sync`, so it
    /// cannot be held here, and in this version it is always built from a
    /// `&'static str` with no cause of its own. Carrying its sentence as data
    /// therefore loses no chain, because there is none to lose. This is the one
    /// place the crate holds a foreign message rather than a foreign error, and
    /// it is named here so review need not re-derive why.
    #[error("the SDK refused the stream declared for `{family}`: {reason}")]
    StreamRejected {
        /// The family whose stream configuration was refused.
        family: Box<str>,
        /// The SDK's own sentence, verbatim.
        reason: Box<str>,
    },

    /// Daemon code asked the registry for a family the census does not declare.
    ///
    /// Nothing caused this one: it is a name that was never in the contract,
    /// which is why it carries no source. The standard is explicit that a
    /// variant holding only data does not need an invented cause.
    #[error("no metric family is declared under the name `{family}`")]
    UnknownFamily {
        /// The name the caller asked for.
        family: Box<str>,
    },
}
