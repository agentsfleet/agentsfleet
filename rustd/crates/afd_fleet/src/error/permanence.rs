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
    /// leasable for the next poll. `resolveTenant` decides the same thing with
    /// a four-arm `switch` and an `else`, which is how the classification came
    /// to be a property of the arm ORDER instead of a property of the failure.
    ///
    /// Exhaustive, so a new kind fails the build until it is classified — the
    /// same device [`Error::code`] uses.
    ///
    /// # A deliberate divergence from the Zig
    ///
    /// [`ErrorKind::ProviderEndpoint`] answers `true`, and the Zig's
    /// `resolveTenant` answers the equivalent `false`: `SecretEndpointInvalid`
    /// is absent from its permanent list and falls through its `else`, so a
    /// stored endpoint that fails the SSRF guard is re-polled forever. The
    /// event never terminates, no terminal row is written, and the only trace
    /// is a warn line repeating at the poll interval.
    ///
    /// That is a latent defect rather than a decision — a stored URL pointing
    /// at the metadata service does not become safe by being retried — and the
    /// reason it is corrected here rather than copied is that the row-parity it
    /// would cost is documentary. The dual-run differ went with the Zig
    /// integration lanes, so nothing compares the two daemons' rows at runtime;
    /// what grades Invariant 5 is REVIEW reading the ported SQL side by side,
    /// and this changes no statement. Registered as a divergence beside the
    /// issue-time debit (Indy, this stream).
    #[must_use]
    pub const fn is_config_permanent(&self) -> bool {
        match self.inner.kind {
            // A declared credential nobody stored, and a stored body that is
            // not an addressable object, are both things a human has to go and
            // fix. `resolveSecretsMap`'s `error.CredentialNotFound` reaches
            // `blockEvent` through the fleet loop's own permanent arm, so this
            // classification is the Zig's rather than a correction to it.
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
            | ErrorKind::Query { .. }
            | ErrorKind::RowMalformed { .. }
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
            | ErrorKind::WriteUnapproved
            | ErrorKind::BindingDrift
            | ErrorKind::WriteSpendExhausted
            // The three memory operator refusals cannot reach the admission
            // pass either: they are raised on the tenant plane, where a person
            // is reading or forgetting what a fleet already learned, and no
            // event is ever leased through it. A `true` on any of them would
            // take a fleet out of service because somebody mistyped a key.
            | ErrorKind::MemoryFleetNotFound
            | ErrorKind::MemoryUnavailable { .. }
            | ErrorKind::MemoryEntryNotFound
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
