//! Exact decimal counters bind activity and model requests to one lease.

use super::Attempt;
use crate::{Result, error::raise::invalid, lease::ActivityInput};

const COUNTER: &str = "Authoring counters must be canonical nonnegative decimal integers";

/// Every draft's private deletion generation, as the draft stream reports it.
///
/// Nothing ever advanced it, so the schema keeps no column for it and the
/// runner protocol no longer carries it; only the observation stream still
/// names it.
pub(crate) const DELETION_GENERATION: i64 = 0;

pub(crate) struct Counters {
    pub(super) fence: i64,
    pub(super) sequence: Option<i64>,
    pub(crate) activity: i64,
}

impl Counters {
    pub(crate) fn renew(request: &garde::Valid<afd_wire::authoring::Renew>) -> Result<Self> {
        Self::lease(&request.fence)
    }

    pub(crate) fn report(request: &garde::Valid<afd_wire::authoring::Report>) -> Result<Self> {
        Self::lease(&request.accounting().fence)
    }

    pub(crate) fn model_call(request: &garde::Valid<crate::model::ModelCallInput>) -> Result<Self> {
        Self::lease(&request.fence)
    }

    fn lease(fence: &str) -> Result<Self> {
        let fence = counter(fence)?;
        if fence == 0 {
            return Err(invalid(COUNTER));
        }
        Ok(Self {
            fence,
            sequence: None,
            activity: 0,
        })
    }

    pub(super) fn binds(&self, attempt: &Attempt) -> bool {
        self.fence == attempt.fencing_token
    }

    pub(crate) fn parse(request: &garde::Valid<ActivityInput>) -> Result<Self> {
        let counters = Self {
            fence: counter(&request.fence)?,
            sequence: Some(counter(&request.sequence)?),
            activity: counter(&request.activity_sequence)?,
        };
        if counters.fence == 0 || counters.sequence == Some(0) || counters.activity == 0 {
            return Err(invalid(COUNTER));
        }
        Ok(counters)
    }
}

/// `raw` as a canonical non-negative decimal: no sign, no leading zero.
fn counter(raw: &str) -> Result<i64> {
    let value = raw.parse::<i64>().map_err(|_overflow| invalid(COUNTER))?;
    if value < 0 || value.to_string() != raw {
        return Err(invalid(COUNTER));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "fixture requests are built to pass the wire bounds"
    )]

    use super::Counters;
    use crate::lease::ActivityInput;
    use afd_core::error_code;
    use afd_wire::authoring::{ActivityEvent, Knowledge, Meter, Renew};

    fn activity(fence: &str, sequence: &str, activity: &str) -> garde::Valid<ActivityInput> {
        garde::Unvalidated::new(ActivityInput {
            fence: fence.into(),
            sequence: sequence.into(),
            activity_sequence: activity.into(),
            event: ActivityEvent::Progress {
                text: "Reading the checkout service".into(),
            },
        })
        .validate()
        .expect("digit counters pass the wire bounds")
    }

    #[test]
    fn a_zero_fence_binds_no_lease() {
        let renew = garde::Unvalidated::new(Renew {
            fence: "0".into(),
            usage: Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                runtime_ms: 1,
                provider_cost_nanos: None,
            },
        })
        .validate()
        .expect("a zero fence passes the wire bounds");
        let refused = Counters::renew(&renew).err().map(|error| error.code());
        assert_eq!(refused, Some(error_code::INVALID_REQUEST));
    }

    #[test]
    fn activity_counters_start_at_one_and_bind_all_three_positions() {
        let counters = Counters::parse(&activity("1", "2", "3")).expect("positive counters");
        assert_eq!(
            (counters.fence, counters.sequence, counters.activity),
            (1, Some(2), 3)
        );
        for (fence, sequence, position) in [
            ("0", "1", "1"),
            ("1", "0", "1"),
            ("1", "1", "0"),
            ("01", "1", "1"),
        ] {
            let refused = Counters::parse(&activity(fence, sequence, position)).err();
            assert_eq!(
                refused.map(|error| error.code()),
                Some(error_code::INVALID_REQUEST),
                "{fence}/{sequence}/{position} names no issued counter"
            );
        }
    }
}
