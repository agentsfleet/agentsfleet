//! What a setup turn's runs used, as its message reports it: the usage
//! lines every run of the turn logged, summed in SQL once no run of it is
//! live, and known once each one reported.

use std::time::Duration;

use af_wire::setup::{Knowledge, Usage};
use afd_billing::RUN_NANOS_PER_SEC;

use crate::{Result, error::ErrorKind};

const MILLIS_PER_SECOND: i128 = 1000;

/// A message's usage columns. All absent while a run of its turn is live,
/// or when no run ran it.
#[derive(sqlx::FromRow)]
pub(crate) struct Metered {
    reported_runtime_ms: Option<i64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    provider_nanos: Option<i64>,
    known: Option<bool>,
}

impl Metered {
    /// The creator-visible usage these columns spell; provider figures stay
    /// absent unless every run of the turn reported them.
    pub(crate) fn usage(&self) -> Result<Option<Usage>> {
        let Some(runtime_ms) = self.reported_runtime_ms else {
            return Ok(None);
        };
        let known = self.known.unwrap_or(false);
        let runtime = Duration::from_millis(u64::try_from(runtime_ms).unwrap_or(0));
        let tokens = |count: Option<i64>| {
            known
                .then(|| u64::try_from(count.unwrap_or(0)))
                .transpose()
                .map_err(|_negative| ErrorKind::Token)
        };
        Ok(Some(Usage {
            knowledge: if known {
                Knowledge::Known
            } else {
                Knowledge::Unknown
            },
            input_tokens: tokens(self.input_tokens)?,
            output_tokens: tokens(self.output_tokens)?,
            provider_cost_nanos: known.then(|| self.provider_nanos.unwrap_or(0).to_string()),
            runtime_cost_nanos: runtime_nanos(runtime)?.to_string(),
        }))
    }
}

/// The runtime charge for `runtime`, rounded up to the nano.
fn runtime_nanos(runtime: Duration) -> Result<i64> {
    i64::try_from(
        (i128::try_from(runtime.as_millis()).map_err(|_overflow| ErrorKind::Balance)?
            * i128::from(RUN_NANOS_PER_SEC)
            + MILLIS_PER_SECOND
            - 1)
            / MILLIS_PER_SECOND,
    )
    .map_err(|_overflow| ErrorKind::Balance.into())
}

#[cfg(test)]
mod tests {
    use af_wire::setup::Knowledge;

    use super::Metered;

    /// A turn whose runs all reported shows their figures; one a lapsed run
    /// left unknown shows only its runtime charge.
    #[test]
    fn a_turns_usage_shows_provider_figures_only_when_known()
    -> Result<(), Box<dyn std::error::Error>> {
        let known = Metered {
            reported_runtime_ms: Some(1_500),
            input_tokens: Some(1_200),
            output_tokens: Some(30),
            provider_nanos: Some(7),
            known: Some(true),
        };
        let usage = known.usage()?.ok_or("a reported turn has usage")?;
        assert_eq!(usage.knowledge, Knowledge::Known);
        assert_eq!(usage.input_tokens, Some(1_200));
        assert_eq!(usage.provider_cost_nanos.as_deref(), Some("7"));
        let unknown = Metered {
            known: Some(false),
            ..known
        };
        let usage = unknown.usage()?.ok_or("a reported turn has usage")?;
        assert_eq!(usage.knowledge, Knowledge::Unknown);
        assert_eq!(
            (usage.input_tokens, usage.provider_cost_nanos),
            (None, None)
        );
        let unreported = Metered {
            reported_runtime_ms: None,
            ..unknown
        };
        assert!(unreported.usage()?.is_none());
        Ok(())
    }
}
