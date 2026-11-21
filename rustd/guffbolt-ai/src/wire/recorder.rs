//! An observer a test reads back: every call, in order, owned.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::observe::{Calibration, Observer, TurnObservation};
use super::retry::Retrying;
use crate::Error;

/// One observer call, as a test reads it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Seen {
    Started,
    FirstText(Duration),
    Ended(String, TurnObservation),
    Measured(Measured),
    Retrying(Retrying),
    /// The opening that failed, and the status its failure carried.
    Reopened(usize, Option<u16>),
}

/// [`Calibration`], owned.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "the fields mirror Calibration's, whose names an embedder logs"
)]
pub(crate) struct Measured {
    pub(crate) route_id: String,
    pub(crate) model_id: String,
    pub(crate) provider_api: &'static str,
    pub(crate) request_kind: &'static str,
    pub(crate) method: &'static str,
    pub(crate) revision: &'static str,
    pub(crate) measured_input_tokens: u64,
    pub(crate) actual_input_tokens: u64,
    pub(crate) measure_us: u64,
}

/// Records every observer call it hears.
#[derive(Debug, Default)]
pub(crate) struct Recorder(Mutex<Vec<Seen>>);

impl Recorder {
    pub(crate) fn new() -> Arc<Self> {
        Arc::default()
    }

    fn push(&self, seen: Seen) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(seen);
    }

    /// Every call so far, in order.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Every turn that ended.
    pub(crate) fn ended(&self) -> Vec<TurnObservation> {
        self.seen()
            .into_iter()
            .filter_map(|seen| match seen {
                Seen::Ended(_, turn) => Some(turn),
                _ => None,
            })
            .collect()
    }

    /// Every measurement reported.
    pub(crate) fn measured(&self) -> Vec<Measured> {
        self.seen()
            .into_iter()
            .filter_map(|seen| match seen {
                Seen::Measured(measured) => Some(measured),
                _ => None,
            })
            .collect()
    }

    /// Every first text, by when it came.
    pub(crate) fn first_texts(&self) -> Vec<Duration> {
        self.seen()
            .into_iter()
            .filter_map(|seen| match seen {
                Seen::FirstText(after) => Some(after),
                _ => None,
            })
            .collect()
    }
}

impl Observer for Recorder {
    fn turn_started(&self) {
        self.push(Seen::Started);
    }

    fn first_text(&self, after: Duration) {
        self.push(Seen::FirstText(after));
    }

    fn turn_ended(&self, model: &str, turn: &TurnObservation) {
        self.push(Seen::Ended(model.to_owned(), *turn));
    }

    fn input_measured(&self, calibration: &Calibration<'_>) {
        self.push(Seen::Measured(Measured {
            route_id: calibration.route_id.to_owned(),
            model_id: calibration.model_id.to_owned(),
            provider_api: calibration.provider_api,
            request_kind: calibration.request_kind,
            method: calibration.method,
            revision: calibration.revision,
            measured_input_tokens: calibration.measured_input_tokens,
            actual_input_tokens: calibration.actual_input_tokens,
            measure_us: calibration.measure_us,
        }));
    }

    fn retrying(&self, retry: Retrying) {
        self.push(Seen::Retrying(retry));
    }

    fn reopened(&self, attempt: usize, failure: &Error) {
        self.push(Seen::Reopened(attempt, failure.status()));
    }
}
