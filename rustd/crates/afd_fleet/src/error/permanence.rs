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
    /// # Why a refused endpoint is permanent
    ///
    /// [`ErrorKind::ProviderEndpoint`] answers `true`. Classified transient, a
    /// stored endpoint that fails the SSRF guard would be re-polled forever:
    /// the event never terminates, no terminal row is written, and the only
    /// trace is a warn line repeating at the poll interval. A stored URL
    /// pointing at the metadata service does not become safe by being retried,
    /// so it earns the terminal row (Indy, decided beside the issue-time
    /// debit).
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        match self.inner.kind {
            // A declared credential nobody stored, and a stored body that is
            // not an addressable object, are both things a human has to go and
            // fix.
            | ErrorKind::VaultDataInvalid
            // A document that will not parse does not become parseable by
            // being read again. Every poll would re-read the same bytes, fail
            // the same rule, and leave the delivery leasable forever — so this
            // earns the terminal row, which is the thing that puts the fleet
            // in front of a human.
            | ErrorKind::ConfigUnreadable { .. } => true,
            // Everything else is infrastructure, and infrastructure recovers.
            //
            // The provider family — a stored endpoint the SSRF guard refused,
            // a selection naming a vault row nobody holds — moved to
            // `afd_credential` with the code that raises it, and is classified
            // there. What reaches here is [`ErrorKind::Credential`], whose own
            // plane already decided; it sits in this arm because a credential
            // fault is not a fleet DOCUMENT fault, which is the question this
            // function answers.
            ErrorKind::Datastore { .. }
            | ErrorKind::Billing { .. }
            | ErrorKind::Credential { .. }
            | ErrorKind::Gate { .. }
            | ErrorKind::Queue { .. }
            | ErrorKind::Admission { .. }
            | ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
            | ErrorKind::Events { .. }
            | ErrorKind::Outbound { .. }
            | ErrorKind::Envelope { .. }
            | ErrorKind::EnvelopeMalformed { .. }
            | ErrorKind::Rejected { .. }
            | ErrorKind::Mint { .. }
            // The six lease-lifecycle refusals are not configuration faults at
            // all — nothing is stored wrong and nobody has to go and fix a
            // document. They answer a runner about ONE request against ONE
            // lease, and the event behind them stays exactly as leasable as it
            // was. A `true` here would write a terminal `gate_blocked` row for
            // a fleet whose only problem was that one runner reported late.
            | ErrorKind::StaleFence
            | ErrorKind::LeaseNotFound
            | ErrorKind::LeaseLost
            | ErrorKind::LeaseMaxRuntime
            | ErrorKind::RenewalNoCredits
            | ErrorKind::BudgetExhausted
            // A message refusal answers one post against one lease, and the
            // mask failing is this process; neither is a document to fix.
            | ErrorKind::MessageNoChannel
            | ErrorKind::MessageLimitReached
            | ErrorKind::Scrub { .. }
            // Infrastructure, not configuration: nobody edits a fleet
            // document to fix a corrupt sequence.
            | ErrorKind::SequenceCorrupt
            // None of the four bundle failures is a FLEET's configuration
            // being wrong, which is the only thing this question asks. Three
            // are the deployment's object storage — unset knobs, a store that
            // will not serve, an object nobody should have put there — and the
            // fourth is the ordinary skill-only answer. A `true` on any of
            // them would write a terminal `gate_blocked` row against a fleet
            // whose document is perfectly good, and take it out of service
            // until a human cleared it.
            | ErrorKind::BundleMissing
            | ErrorKind::BundleUnconfigured
            | ErrorKind::BundleStorage { .. }
            | ErrorKind::BundleOversized { .. }
            // Not one of the mint refusals is a FLEET's document being wrong,
            // which is the only thing this question asks. They are a tenant's
            // connection, this deployment's own configuration, a vendor, or a
            // human's answer — and a `true` on any of them would write a
            // terminal `gate_blocked` row against a fleet whose config is
            // perfectly good, taking it out of service until somebody cleared
            // it. Drift is the closest call and still `false`: what changed is
            // the fleet's binding, and the remedy is a human re-answering the
            // card, not an edit to fix a broken document.
            | ErrorKind::IntegrationNotConnected
            | ErrorKind::MintUnconfigured
            | ErrorKind::GithubReconnectRequired
            | ErrorKind::GithubMintFailed
            | ErrorKind::ConnectorReconnectRequired
            | ErrorKind::ConnectorMintFailed
            | ErrorKind::GrantRequired
            // A memory failure is never a fleet's document being wrong: it is
            // a store that would not answer, or a person reading or forgetting
            // what a fleet already learned. A `true` would take a fleet out of
            // service because somebody mistyped a key.
            | ErrorKind::Memory { .. }
            // The login family cannot reach the admission pass at all: it is
            // raised on the device-flow surface, which no event is ever leased
            // through. `false` is the honest answer for a question that never
            // gets asked of it — a `true` would claim a fleet's stored
            // configuration is broken because somebody mistyped six digits.
            // The api-key lifecycle family joins the login one: it is raised on
            // the tenant plane, which no event is ever leased through.
            // The command-line credential family joins them, for the same
            // reason: it is raised on the tenant plane, which no event is ever
            // leased through.
            | ErrorKind::Entropy { .. } => false,
        }
    }
}
