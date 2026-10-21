//! Whether a failure is the fleet's stored CONFIGURATION being wrong.
//!
//! One question, one table, split from [`super::classify`] at the seam that
//! module's own header already draws. The two tables there are what a CLIENT is
//! told — a registry code and a sentence — and this one is what the admission
//! pass DECIDES, which no client ever sees. The file cap forced the split; the
//! seam was already there.

use super::{Error, ErrorKind};

impl Error {
    /// Whether this failure is a stored-CONFIGURATION fault rather than an
    /// infrastructure one.
    ///
    /// The question the admission pass turns on, and the reason it is one
    /// method rather than a `match` at the call site: a permanent fault earns
    /// the terminal `gate_blocked` row, and a transient one leaves the delivery
    /// leasable for the next poll. A `match` with a catch-all arm would make
    /// the classification a property of the arm ORDER instead of a property of
    /// the failure.
    ///
    /// Exhaustive, so a new kind fails the build until it is classified — the
    /// same device [`Error::code`] uses.
    ///
    /// # Why each kind lands where it does
    ///
    /// `true` is reserved for a document a human has to go and fix:
    ///
    /// - A stored vault body that is not an addressable object.
    /// - A config document that will not parse. It does not become parseable
    ///   by being read again: every poll would re-read the same bytes, fail
    ///   the same rule, and leave the delivery leasable forever, so it earns
    ///   the terminal row that puts the fleet in front of a human.
    /// - A fleet with no reviewed native setup to run.
    ///
    /// Everything else is infrastructure, and infrastructure recovers:
    ///
    /// - **Credential** — the provider family (a stored endpoint the SSRF guard
    ///   refused, a selection naming a vault row nobody holds) is classified in
    ///   `afd_credential`, with the code that raises it. A credential fault is
    ///   not a fleet DOCUMENT fault, which is the question this answers.
    /// - **Lease lifecycle** — the six refusals answer a runner about ONE
    ///   request against ONE lease, and the event behind them stays exactly as
    ///   leasable as it was. A `true` would write a terminal row for a fleet
    ///   whose only problem was that one runner reported late.
    /// - **Messages and the mask** — a refusal answers one post against one
    ///   lease, and the mask failing is this process.
    /// - **Corrupt sequence** — nobody edits a fleet document to fix it.
    /// - **Mint** — a tenant's connection, this deployment's own configuration,
    ///   a vendor, or a human's answer; never the fleet's document. A `true`
    ///   would take a perfectly good fleet out of service until somebody
    ///   cleared it.
    /// - **Memory** — a store that would not answer, or a person reading or
    ///   forgetting what a fleet learned.
    /// - **Entropy** — a host that cannot draw random bytes is this process.
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        match self.inner.kind {
            ErrorKind::VaultDataInvalid
            | ErrorKind::ConfigUnreadable { .. }
            | ErrorKind::SetupUnavailable => true,
            ErrorKind::Datastore { .. }
            | ErrorKind::Runner { .. }
            | ErrorKind::Authoring { .. }
            | ErrorKind::Billing { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Gate { .. }
            | ErrorKind::Queue { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Events { .. }
            | ErrorKind::Outbound { .. }
            | ErrorKind::Rejected { .. }
            | ErrorKind::Mint { .. }
            | ErrorKind::StaleFence
            | ErrorKind::LeaseNotFound
            | ErrorKind::LeaseLost
            | ErrorKind::LeaseMaxRuntime
            | ErrorKind::RenewalNoCredits
            | ErrorKind::BudgetExhausted
            | ErrorKind::MessageNoChannel
            | ErrorKind::MessageLimitReached
            | ErrorKind::Scrub { .. }
            | ErrorKind::SequenceCorrupt
            | ErrorKind::IntegrationNotConnected
            | ErrorKind::MintUnconfigured
            | ErrorKind::GithubReconnectRequired
            | ErrorKind::GithubMintFailed
            | ErrorKind::ConnectorReconnectRequired
            | ErrorKind::ConnectorMintFailed
            | ErrorKind::GrantRequired
            | ErrorKind::Memory { .. }
            | ErrorKind::Entropy { .. } => false,
        }
    }
}
