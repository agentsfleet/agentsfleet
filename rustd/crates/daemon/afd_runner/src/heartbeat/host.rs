//! The host facts a beat carries, proved inside the columns that store them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::runner::HostFacts;

/// The event a beat's refused host facts are logged under.
const EVENT_HOST_FACTS_REFUSED: &str = "runner_host_facts_refused";

/// The refused host-fact fields each runner's last beat carried, so a refusal
/// is logged when it changes rather than on every beat.
pub(crate) type RefusedFacts = Arc<Mutex<HashMap<Uuid7, String>>>;

/// The runner row's three host-fact columns, as one beat binds them. A fact
/// the beat lacks binds `NULL`, and the row keeps the last value it had.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct HostColumns {
    pub(super) cpus: Option<i32>,
    pub(super) memory: Option<i64>,
    pub(super) free_disk: Option<i64>,
}

impl HostColumns {
    /// `facts` proved inside their wire bounds. Facts past them are not facts:
    /// the beat binds none of them, and a refusal new for `runner` is logged.
    pub(super) fn of(refused: &RefusedFacts, runner: &Uuid7, facts: HostFacts) -> Self {
        let proved = garde::Unvalidated::new(facts).validate();
        let mut last = refused.lock().unwrap_or_else(PoisonError::into_inner);
        match proved {
            // Proved inside the columns' types, so no conversion drops a fact.
            Ok(proved) => {
                last.remove(runner);
                Self {
                    cpus: proved.cpu_count.and_then(|cpus| i32::try_from(cpus).ok()),
                    memory: proved
                        .memory_bytes
                        .and_then(|bytes| i64::try_from(bytes).ok()),
                    free_disk: proved
                        .free_disk_bytes
                        .and_then(|bytes| i64::try_from(bytes).ok()),
                }
            }
            Err(report) => {
                let fields = fields(&report);
                if last.get(runner) != Some(&fields) {
                    log_refused(runner, &fields);
                    last.insert(runner.clone(), fields);
                }
                Self::default()
            }
        }
    }
}

/// The refused fields' names, never their values.
fn fields(report: &garde::Report) -> String {
    report
        .iter()
        .map(|(path, _)| path.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Logs the fields a beat's bounds refused.
fn log_refused(runner: &Uuid7, fields: &str) {
    let code = error_code::INVALID_REQUEST.as_str();
    let id = runner.as_str();
    {
        let (error_code, runner_id, event, message) = (
            &code,
            &id,
            &EVENT_HOST_FACTS_REFUSED,
            format_args!("host facts past their bounds; the runner row keeps its last ones"),
        );
        tracing::warn!(message, error_code, runner_id, fields, event);
    };
}
