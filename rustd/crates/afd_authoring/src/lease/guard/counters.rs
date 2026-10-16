//! Exact decimal counters bind activity and model requests to one lease.

use super::Attempt;
use crate::{Result, error::raise::invalid, lease::ActivityInput};

const COUNTER: &str = "Authoring counters must be canonical nonnegative decimal integers";

pub(crate) struct Counters {
    pub(super) fence: i64,
    pub(super) generation: i64,
    pub(super) sequence: Option<i64>,
    pub(crate) activity: i64,
}

impl Counters {
    pub(crate) fn renew(request: &garde::Valid<afd_wire::authoring::Renew>) -> Result<Self> {
        Self::lease(&request.fence, &request.deletion_generation)
    }

    pub(crate) fn report(request: &garde::Valid<afd_wire::authoring::Report>) -> Result<Self> {
        let accounting = request.accounting();
        Self::lease(&accounting.fence, &accounting.deletion_generation)
    }

    fn lease(fence: &str, generation: &str) -> Result<Self> {
        let parse = |raw: &str| {
            let value = raw.parse::<i64>().map_err(|_overflow| invalid(COUNTER))?;
            if value < 0 || value.to_string() != raw {
                return Err(invalid(COUNTER));
            }
            Ok(value)
        };
        let fence = parse(fence)?;
        if fence == 0 {
            return Err(invalid(COUNTER));
        }
        Ok(Self {
            fence,
            generation: parse(generation)?,
            sequence: None,
            activity: 0,
        })
    }

    pub(super) fn binds(&self, attempt: &Attempt) -> bool {
        self.fence == attempt.fence && self.generation == attempt.deletion_generation
    }

    pub(crate) fn parse(request: &garde::Valid<ActivityInput>) -> Result<Self> {
        let counter = |raw: &str| {
            let value = raw.parse::<i64>().map_err(|_overflow| invalid(COUNTER))?;
            if value < 0 || value.to_string() != raw {
                return Err(invalid(COUNTER));
            }
            Ok(value)
        };
        let counters = Self {
            fence: counter(&request.fence)?,
            generation: counter(&request.deletion_generation)?,
            sequence: Some(counter(&request.sequence)?),
            activity: counter(&request.activity_sequence)?,
        };
        if counters.fence == 0 || counters.sequence == Some(0) || counters.activity == 0 {
            return Err(invalid(COUNTER));
        }
        Ok(counters)
    }

    pub(crate) fn model_call(request: &garde::Valid<crate::model::ModelCallInput>) -> Result<Self> {
        let parse = |raw: &str| {
            let value = raw.parse::<i64>().map_err(|_overflow| invalid(COUNTER))?;
            if value < 0 || value.to_string() != raw {
                return Err(invalid(COUNTER));
            }
            Ok(value)
        };
        let fence = parse(&request.fence)?;
        if fence == 0 {
            return Err(invalid(COUNTER));
        }
        Ok(Self {
            fence,
            generation: parse(&request.deletion_generation)?,
            sequence: None,
            activity: 0,
        })
    }
}
