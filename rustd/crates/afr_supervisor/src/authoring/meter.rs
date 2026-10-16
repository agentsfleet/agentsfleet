//! Shared observations never turn an unresolved provider call into zero usage.

use crate::{Result, error};
use afd_wire::authoring::{Knowledge, Meter, ModelUsage};
use std::sync::{Arc, Mutex};
use tokio::time::Instant;

/// The refusal a poisoned meter lock earns: a panic interrupted an update.
const METER_INTERRUPTED: &str = "authoring meter was interrupted";

#[derive(Debug, Clone)]
pub(super) struct Observed {
    started: Instant,
    meter: Arc<Mutex<Meter>>,
}

impl Observed {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            meter: Arc::new(Mutex::new(Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                provider_cost_nanos: Some("0".to_owned()),
                runtime_ms: 0,
            })),
        }
    }

    pub(super) fn snapshot(&self) -> Result<Meter> {
        let mut meter = self
            .meter
            .lock()
            .map_err(|_poisoned| error::config(METER_INTERRUPTED))?
            .clone();
        meter.runtime_ms = afd_core::clock::saturating_millis(self.started.elapsed()).min(300_000);
        Ok(meter)
    }

    pub(super) fn uncertain(&self) -> Result<()> {
        let mut meter = self
            .meter
            .lock()
            .map_err(|_poisoned| error::config(METER_INTERRUPTED))?;
        meter.knowledge = Knowledge::Unknown;
        meter.input_tokens = None;
        meter.output_tokens = None;
        meter.provider_cost_nanos = None;
        Ok(())
    }

    pub(super) fn accept(&self, usage: &ModelUsage) -> Result<()> {
        let ModelUsage::Known {
            input_tokens,
            output_tokens,
            provider_cost_nanos,
        } = usage
        else {
            return self.uncertain();
        };
        let mut meter = self
            .meter
            .lock()
            .map_err(|_poisoned| error::config(METER_INTERRUPTED))?;
        let cost = provider_cost_nanos
            .parse::<u64>()
            .map_err(|_unparsed| error::config("the daemon returned an invalid provider cost"))?;
        let prior = meter
            .provider_cost_nanos
            .as_ref()
            .and_then(|cost| cost.parse::<u64>().ok())
            .ok_or_else(|| error::config("known provider usage followed an uncertain dispatch"))?;
        let input = meter
            .input_tokens
            .and_then(|value| value.checked_add(*input_tokens))
            .ok_or_else(|| error::config("the daemon input usage overflowed"))?;
        let output = meter
            .output_tokens
            .and_then(|value| value.checked_add(*output_tokens))
            .ok_or_else(|| error::config("the daemon output usage overflowed"))?;
        meter.calls = meter
            .calls
            .checked_add(1)
            .ok_or_else(|| error::config("the call count overflowed"))?;
        meter.input_tokens = Some(input);
        meter.output_tokens = Some(output);
        meter.provider_cost_nanos = Some(
            prior
                .checked_add(cost)
                .ok_or_else(|| error::config("the daemon provider cost overflowed"))?
                .to_string(),
        );
        Ok(())
    }
}
