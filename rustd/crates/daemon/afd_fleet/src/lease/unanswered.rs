//! The fixed text a thread is owed when its event ends with nothing to say.
//!
//! A person who asked from a thread is owed an answer there. A run that failed
//! before it wrote any text has none, and neither has an event a gate refused,
//! and a thread left silent reads as a fleet still working. No model runs: the
//! text names the recorded reason and the command that shows the rest, and it
//! is owed through the ledger an answer is owed through.
//!
//! Written to the obligation alone. The event's `response_text` stays what the
//! run said, so a screen rendering the fleet's answer never shows this text as
//! though the fleet had written it.

use af_core::id::Uuid7;

/// What the thread reads when a run failed before it answered.
///
/// `reason` is the stored failure label, when the runner classified one.
pub(crate) fn failed_run(fleet: &Uuid7, reason: Option<&str>) -> String {
    let named = reason.map_or_else(String::new, |label| format!(" ({label})"));
    format!(
        "This run ended before it could answer{named}. A workspace member can read what \
         happened with `agentsfleet fleet events {fleet}`."
    )
}

/// What the thread reads when a gate refused the event before any run.
pub(crate) fn refused(fleet: &Uuid7, reason: &str) -> String {
    format!(
        "This request was not run ({reason}). A workspace member can read why with \
         `agentsfleet fleet events {fleet}`."
    )
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]
mod tests {
    use af_core::id::Uuid7;

    use super::{failed_run, refused};

    /// A fleet identifier the texts name.
    const FLEET: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0120";

    fn fleet() -> Uuid7 {
        Uuid7::parse(FLEET).expect("the fixture id is canonical")
    }

    #[test]
    fn a_failed_run_names_its_label_and_the_command_that_shows_the_rest() {
        assert_eq!(
            failed_run(&fleet(), Some("timeout_kill")),
            "This run ended before it could answer (timeout_kill). A workspace member can read \
             what happened with `agentsfleet fleet events 019feca5-bc9b-72e8-b71f-e2714f6b0120`."
        );
    }

    #[test]
    fn a_failed_run_the_runner_did_not_classify_names_no_label() {
        assert_eq!(
            failed_run(&fleet(), None),
            "This run ended before it could answer. A workspace member can read what happened \
             with `agentsfleet fleet events 019feca5-bc9b-72e8-b71f-e2714f6b0120`."
        );
    }

    #[test]
    fn a_refused_event_names_the_gate_that_refused_it() {
        assert_eq!(
            refused(&fleet(), af_core::event::label::BALANCE_EXHAUSTED),
            "This request was not run (balance_exhausted). A workspace member can read why with \
             `agentsfleet fleet events 019feca5-bc9b-72e8-b71f-e2714f6b0120`."
        );
    }
}
