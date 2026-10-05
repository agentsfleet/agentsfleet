//! What this crate refuses: a knob it cannot read, an exporter that will not
//! build, a census the instrument layer rejects.
//!
//! One error type with `pub type Result<T, E = Error>` beside it, under the
//! `afd_core::error_shell!` hull every `rustd` crate carries. [`Refused`] is
//! the one finer-grained type, kept because a caller DISCRIMINATES on it: the
//! daemon's preflight turns each refused knob into a fault it reports beside
//! every other fault, so it needs the knob and the sentence as data rather
//! than a rendered chain (`docs/RUST_ERROR_STANDARD.md`, the carve-out).

use afd_core::error_code::{self, ErrorCode};

/// The result every fallible function in this crate returns.
pub type Result<T, E = Error> = core::result::Result<T, E>;

afd_core::error_shell!(
    /// A telemetry transport this crate declined to build, with the backtrace
    /// of where it was refused.
    pub struct Error(ErrorKind);
);

/// One knob this crate could not read, and why.
///
/// `Copy` and fieldless beyond two static strings: it is raised once per
/// boot at most, and a caller that aggregates faults copies it into its own
/// record. The sentence is the operator's — it says what the knob accepts,
/// never what it was set to, because an endpoint is read from the same place
/// as the credential beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{knob}: {why}")]
pub struct Refused {
    /// The environment variable an operator has to fix.
    pub knob: &'static str,
    /// What that variable accepts.
    pub why: &'static str,
}

/// Every way this crate fails.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorKind {
    /// A knob was set to something this build cannot use.
    #[error(transparent)]
    Knob {
        /// Which knob, and what it accepts.
        source: Refused,
    },

    /// The exporter would not build from an accepted configuration.
    ///
    /// Not over-strictness: every knob was already accepted, so a failure
    /// here is a defect, and a process that served on through it would export
    /// nothing and look exactly like a collector that is down.
    #[error("the telemetry exporter would not build")]
    Exporter {
        /// The exporter's reason.
        #[source]
        source: opentelemetry_otlp::ExporterBuildError,
    },

    /// The metric contract and the code disagree.
    #[error("the metric contract was refused")]
    Contract {
        /// The instrument layer's reason.
        #[source]
        source: afd_observability::Error,
    },
}

afd_core::error_lifts!(Error, ErrorKind:
    Refused => Knob,
    opentelemetry_otlp::ExporterBuildError => Exporter,
    afd_observability::Error => Contract,
);

impl Error {
    /// The registry code an operator reads this under.
    ///
    /// A refused knob is the one that names something an operator can fix;
    /// the other two are defects in the build.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self.kind() {
            ErrorKind::Knob { .. } => error_code::STARTUP_ENV_CHECK,
            ErrorKind::Exporter { .. } | ErrorKind::Contract { .. } => {
                error_code::INTERNAL_OPERATION_FAILED
            }
        }
    }

    /// The knob this failure refused, when it refused one.
    #[must_use]
    pub fn refused(&self) -> Option<Refused> {
        match self.kind() {
            ErrorKind::Knob { source } => Some(*source),
            _built => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use afd_core::error_code;

    use super::{Error, Refused};

    /// A census whose one row declares a kind the vocabulary does not spell.
    const SEEDED_WRONG: &str = "name\tkind\tnumber\tunit\ttemporality\tlabels\tbounds\tpolicy\tlive_read\tcategory\twatch_for\n\
                                a.family\tbogus\tu64\t1\tcumulative\t-\t-\tfixed:1\tno\ttraffic\tnothing\n";

    /// A refused knob carries itself as data and answers the configuration
    /// code; a refused contract answers the internal one and carries no knob.
    #[test]
    fn a_refused_knob_is_data_and_a_refused_contract_is_not() {
        let refused = Refused {
            knob: "A_KNOB",
            why: "a sentence",
        };
        let knob = Error::from(refused);
        assert_eq!(knob.refused(), Some(refused));
        assert_eq!(knob.code(), error_code::STARTUP_ENV_CHECK);
        assert!(knob.to_string().contains("A_KNOB: a sentence"));
        assert!(
            knob.source().is_none(),
            "a refusal is the whole story; nothing caused it"
        );

        let Err(census) = afd_observability::metrics::registry::Registry::read(SEEDED_WRONG) else {
            unreachable!("a census declaring a kind nobody spelled does not read");
        };
        let contract = Error::from(census);
        assert_eq!(contract.refused(), None);
        assert_eq!(contract.code(), error_code::INTERNAL_OPERATION_FAILED);
        assert!(
            contract.source().is_some(),
            "the instrument layer's own sentence survives as the cause"
        );
    }
}
