//! What one settled run cost, in the five families the money dashboards read.
//!
//! # Why the model attribution is bounded by the SDK rather than derived
//!
//! The daemon this ports derives a model-attribution cap from its own flush
//! ceiling: so many distinct `(provider, model)` pairs may carry exact
//! attribution before the rest are folded. It has to, because it hand-wrote the
//! aggregator and the ceiling is a fixed array it owns.
//!
//! Here the SDK is the aggregator and enforces a per-stream cardinality limit
//! of its own, which is the same bound arrived at from the other side. What is
//! NOT ported is the derivation, and the difference is worth naming: the Zig
//! omits attribution when its own budget is exhausted and counts the omission,
//! where the SDK folds the excess into `otel.metric.overflow`. This build still
//! counts the omissions it makes for the other two reasons — an unmapped
//! provider and a value too long to carry — so the family is fed either way.

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram};

use crate::error::Result;
use crate::metrics::declared::cost as declared;
use crate::metrics::instrument::Instruments;
use crate::metrics::label::cost::{ChargeClass, ErrorType, TokenType};
use crate::producers::installed;
use crate::semconv;

/// One run's spend, as the caller already holds it.
///
/// A parameter object rather than seven arguments: `input`, `cached_input` and
/// `output` are three token counts of the same type, and two of them
/// transposed would compile and quietly bill a tenant for the wrong direction.
#[derive(Debug, Clone, Copy)]
pub struct Spend<'a> {
    /// The model the run was issued against.
    pub model: &'a str,
    /// The sandbox posture, in the spelling the lease row stored.
    pub posture: &'a str,
    /// Prompt tokens for the whole run, cached ones included.
    pub input_tokens: u64,
    /// The cached subset of the above. Never a third direction.
    pub cached_input_tokens: u64,
    /// Completion tokens for the whole run.
    pub output_tokens: u64,
    /// How long the invocation took.
    pub wall: core::time::Duration,
    /// The coarse verdict, absent on a clean run.
    pub error: Option<ErrorType>,
}

/// The instruments the money path records through.
#[derive(Debug)]
pub struct Handles {
    duration: Histogram<f64>,
    tokens: Histogram<f64>,
    cache_read: Histogram<f64>,
    credits: Counter<u64>,
    samples_dropped: Counter<u64>,
}

impl Handles {
    /// Claims every instrument this domain records through.
    ///
    /// # Errors
    ///
    /// Whatever [`Instruments`] refuses — see [`super::install`].
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            duration: instruments.histogram_f64(&declared::INVOKE_AGENT_DURATION)?,
            tokens: instruments.histogram_f64(&declared::INVOKE_AGENT_TOKEN_USAGE)?,
            cache_read: instruments
                .histogram_f64(&declared::INVOKE_AGENT_CACHE_READ_TOKEN_USAGE)?,
            credits: instruments.counter_u64(&declared::BILLING_CREDIT_CONSUMED)?,
            samples_dropped: instruments.counter_u64(&declared::TELEMETRY_SAMPLES_DROPPED)?,
        })
    }
}

/// Records what one finished invocation spent.
///
/// Four families from one call, because they describe one event and a caller
/// that recorded three of them would leave a run half-attributed — which reads
/// as a model that is cheap rather than as a measurement that is missing.
#[expect(
    clippy::cast_precision_loss,
    reason = "a token count past 2^53 is not a number a runner can report; the               histogram's own top bucket is 2^26"
)]
pub fn invocation(spend: &Spend<'_>) {
    let Some(producers) = installed() else {
        return;
    };
    let model = KeyValue::new(semconv::ATTR_REQUEST_MODEL, spend.model.to_owned());
    let posture = KeyValue::new(semconv::ATTR_EXECUTION_POSTURE, spend.posture.to_owned());

    let mut duration = vec![model.clone(), posture.clone()];
    if let Some(error) = spend.error {
        duration.push(KeyValue::new(semconv::ATTR_ERROR_TYPE, error.as_str()));
    }
    producers
        .cost
        .duration
        .record(spend.wall.as_secs_f64(), &duration);

    for (direction, count) in [
        (TokenType::Input, spend.input_tokens),
        (TokenType::Output, spend.output_tokens),
    ] {
        producers.cost.tokens.record(
            count as f64,
            &[
                model.clone(),
                posture.clone(),
                KeyValue::new(semconv::ATTR_TOKEN_TYPE, direction.as_str()),
            ],
        );
    }

    producers
        .cost
        .cache_read
        .record(spend.cached_input_tokens as f64, &[model, posture]);
}

/// Records a debit against a tenant's wallet.
pub fn credits_consumed(model: &str, posture: &str, class: ChargeClass, nanocredits: u64) {
    if let Some(producers) = installed() {
        producers.cost.credits.add(
            nanocredits,
            &[
                KeyValue::new(semconv::ATTR_REQUEST_MODEL, model.to_owned()),
                KeyValue::new(semconv::ATTR_EXECUTION_POSTURE, posture.to_owned()),
                KeyValue::new(semconv::ATTR_CHARGE_TYPE, class.as_str()),
            ],
        );
    }
}

/// Records samples the exporter shed before they could be exported.
///
/// Rides the same push it describes, which is why it is not the only account of
/// telemetry loss: it arrives only if a LATER export succeeds. The
/// discarded-entries counter beside it is what a dead pipe is caught by.
pub fn samples_dropped(samples: u64) {
    if let Some(producers) = installed() {
        producers.cost.samples_dropped.add(samples, &[]);
    }
}
