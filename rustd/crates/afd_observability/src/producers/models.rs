//! Per-call latency and expiring catalogue metadata; callbacks never query the database.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::Result;
use crate::metrics::declared::models as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::label::models::CallOutcome;
use crate::producers::installed;
use crate::semconv;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram};

/// Four metadata series per model stay below the SDK's 2,000-series ceiling.
pub const MAX_MODELS: u32 = 256;
/// A stalled sampler cannot leave an indefinitely live catalogue claim.
const MAX_AGE: Duration = Duration::from_secs(120);

/// Non-secret facts supplied by the canonical model catalogue and route resolver.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Vendor model identifier, never a row UUID or display label.
    pub model: String,
    /// Public provider identity; private names are omitted from exported attributes.
    pub provider: String,
    /// The catalogue's verified context limit.
    pub context_tokens: u64,
    /// Configured completion limit, absent for an unconfigured route.
    pub output_tokens: Option<u64>,
    /// Whether the configured codec offers tools for this model.
    pub tools: Option<bool>,
    /// Whether the configured API transports tool-result images.
    pub image_transport: Option<bool>,
}

struct Snapshot {
    models: Vec<ModelInfo>,
    clipped: bool,
    at: Instant,
}
#[derive(Clone, Copy)]
enum Field {
    Limits,
    Routes,
    Age,
    Clipped,
}

/// Instruments and the latest bounded snapshot.
#[derive(Debug)]
pub struct Handles {
    duration: Histogram<f64>,
    tokens: Counter<u64>,
    cache_read: Counter<u64>,
    cache_write: Counter<u64>,
    first: Histogram<f64>,
    snapshot: Arc<Mutex<Option<Snapshot>>>,
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("models", &self.models.len())
            .finish_non_exhaustive()
    }
}

impl Handles {
    /// Claims the call histograms and snapshot-backed gauges.
    ///
    /// # Errors
    /// Refuses a census/instrument disagreement.
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        let snapshot: Arc<Mutex<Option<Snapshot>>> = Arc::new(Mutex::new(None));
        for (family, field) in [
            (&declared::LIMITS, Field::Limits),
            (&declared::ROUTES, Field::Routes),
            (&declared::SNAPSHOT_AGE, Field::Age),
            (&declared::CLIPPED, Field::Clipped),
        ] {
            let state = Arc::clone(&snapshot);
            instruments.gauge_u64(family, move || {
                state
                    .try_lock()
                    .ok()
                    .and_then(|guard| {
                        guard
                            .as_ref()
                            .map(|snapshot| readings(snapshot, field, Instant::now()))
                    })
                    .unwrap_or_default()
            })?;
        }
        Ok(Self {
            snapshot,
            tokens: instruments.counter_u64(&declared::TOKENS)?,
            cache_read: instruments.counter_u64(&declared::CACHE_READ)?,
            cache_write: instruments.counter_u64(&declared::CACHE_WRITE)?,
            duration: instruments.histogram_f64(&declared::CALL_DURATION)?,
            first: instruments.histogram_f64(&declared::FIRST_TOKEN_DURATION)?,
        })
    }
}

fn attributes(model: &str, provider: &str) -> Vec<KeyValue> {
    let mut values = vec![KeyValue::new(semconv::ATTR_REQUEST_MODEL, model.to_owned())];
    if let Some(provider) = semconv::provider::normalize(provider) {
        values.push(KeyValue::new(semconv::ATTR_PROVIDER_NAME, provider));
    }
    values
}

fn readings(snapshot: &Snapshot, field: Field, now: Instant) -> Vec<Reading> {
    let age = now.saturating_duration_since(snapshot.at);
    match field {
        Field::Age => return vec![Reading::unlabelled(age.as_secs())],
        Field::Clipped => return vec![Reading::unlabelled(u64::from(snapshot.clipped))],
        Field::Limits | Field::Routes => {}
    }
    if age >= MAX_AGE {
        return Vec::new();
    }
    snapshot
        .models
        .iter()
        .flat_map(|model| {
            let entries = if matches!(field, Field::Limits) {
                [
                    ("context", Some(model.context_tokens)),
                    ("output", model.output_tokens),
                ]
            } else {
                [
                    ("tools", model.tools.map(u64::from)),
                    ("image_transport", model.image_transport.map(u64::from)),
                ]
            };
            entries.into_iter().filter_map(move |(key, value)| {
                value.map(|value| {
                    let mut labels = attributes(&model.model, &model.provider);
                    labels.push(KeyValue::new(
                        if matches!(field, Field::Limits) {
                            semconv::LABEL_LIMIT
                        } else {
                            semconv::LABEL_CAPABILITY
                        },
                        key,
                    ));
                    Reading {
                        attributes: labels,
                        value,
                    }
                })
            })
        })
        .collect()
}

/// Records one provider stream call, excluding post-stream usage recovery.
pub fn call(
    model: &str,
    provider: &str,
    outcome: CallOutcome,
    elapsed: Duration,
    first_text: Option<Duration>,
) {
    if let Some(producers) = installed() {
        let mut labels = attributes(model, provider);
        labels.push(KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()));
        producers
            .models
            .duration
            .record(elapsed.as_secs_f64(), &labels);
        if let Some(first) = first_text {
            producers.models.first.record(first.as_secs_f64(), &labels);
        }
    }
}

/// Replaces the whole metadata snapshot; missing/removed rows cannot linger.
pub fn catalogue(models: Option<(Vec<ModelInfo>, bool)>) {
    if let Some(producers) = installed()
        && let Ok(mut target) = producers.models.snapshot.lock()
    {
        *target = models.map(|(mut models, mut clipped)| {
            clipped |= models.len() > MAX_MODELS as usize;
            models.truncate(MAX_MODELS as usize);
            Snapshot {
                models,
                clipped,
                at: Instant::now(),
            }
        });
    }
}

/// Records only token counts the provider actually reported; input includes cache subsets.
pub fn tokens(model: &str, provider: &str, usage: afd_wire::usage::ObservedTokens) {
    if let Some(producers) = installed() {
        let base = attributes(model, provider);
        for (kind, count) in [
            (crate::metrics::label::cost::TokenType::Input, usage.input),
            (crate::metrics::label::cost::TokenType::Output, usage.output),
        ] {
            if let Some(count) = count {
                let mut labels = base.clone();
                labels.push(KeyValue::new(semconv::ATTR_TOKEN_TYPE, kind.as_str()));
                producers.models.tokens.add(count, &labels);
            }
        }
        if let Some(count) = usage.cache_read {
            producers.models.cache_read.add(count, &base);
        }
        if let Some(count) = usage.cache_write {
            producers.models.cache_write.add(count, &base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogue_metadata_expires_without_inventing_zero() {
        let snapshot = Snapshot {
            models: vec![ModelInfo {
                model: "fixture".into(),
                provider: "openai".into(),
                context_tokens: 8192,
                output_tokens: None,
                tools: Some(false),
                image_transport: None,
            }],
            clipped: false,
            at: Instant::now(),
        };
        let debug = format!("{snapshot:?}");
        assert!(debug.contains("models: 1"));
        assert!(!debug.contains("fixture"));
        assert!(!debug.contains("openai"));
        assert_eq!(readings(&snapshot, Field::Limits, snapshot.at).len(), 1);
        assert_eq!(
            readings(&snapshot, Field::Routes, snapshot.at)
                .first()
                .map(|reading| reading.value),
            Some(0)
        );
        assert_eq!(
            readings(&snapshot, Field::Limits, snapshot.at + MAX_AGE).len(),
            0
        );
        assert_eq!(
            readings(&snapshot, Field::Routes, snapshot.at + MAX_AGE).len(),
            0
        );
        assert_eq!(
            readings(&snapshot, Field::Age, snapshot.at + MAX_AGE)
                .first()
                .map(|reading| reading.value),
            Some(MAX_AGE.as_secs())
        );
    }
}
