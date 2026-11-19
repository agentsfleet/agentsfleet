//! Task deployment policy rejects unsafe values before opening services.

use std::io::Write as _;

use af_core::env::MapEnv;

use super::{ConfigBuilder, FILE_KNOB};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn task_admission_settings_respect_protocol_ceilings() -> TestResult {
    let environment = MapEnv::default();
    for (setting, field) in [
        ("max_unfinished_per_fleet = 17", "max_unfinished_per_fleet"),
        (
            "max_inputs_per_safe_boundary = 5",
            "max_inputs_per_safe_boundary",
        ),
        ("max_actions_per_message = 0", "max_actions_per_message"),
        ("max_actions_per_message = 5", "max_actions_per_message"),
        ("max_objective_bytes = 16385", "max_objective_bytes"),
        ("max_title_bytes = 161", "max_title_bytes"),
        ("max_completion_questions = 7", "max_completion_questions"),
        ("max_question_text_bytes = 513", "max_question_text_bytes"),
        (
            "max_authored_question_bytes = 1025",
            "max_authored_question_bytes",
        ),
    ] {
        let mut file = tempfile::NamedTempFile::new()?;
        writeln!(file, "[fleet.tasks]\n{setting}")?;
        let result = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build();
        let failure = result
            .err()
            .ok_or("unsafe task policy must refuse startup")?;
        assert_eq!(failure.knobs(), [FILE_KNOB]);
        assert!(failure.to_string().contains(field), "{failure}");
    }
    Ok(())
}

#[test]
fn task_allowances_refuse_negative_money_and_impossible_active_lifetimes() -> TestResult {
    let environment = MapEnv::default();
    for (setting, field) in [
        ("model_budget_nanodollars = -1", "model_budget_nanodollars"),
        ("wall_seconds = 60\nrunning_seconds = 61", "running_seconds"),
        ("max_runs = 0", "max_runs"),
        ("max_publication_attempts = 0", "max_publication_attempts"),
        ("max_decision_calls = 0", "max_decision_calls"),
        ("max_fallback_reads = 0", "max_fallback_reads"),
    ] {
        let mut file = tempfile::NamedTempFile::new()?;
        writeln!(file, "[fleet.tasks.defaults]\n{setting}")?;
        let result = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build();
        let failure = result
            .err()
            .ok_or("unsafe task allowance must refuse startup")?;
        assert_eq!(failure.knobs(), [FILE_KNOB]);
        assert!(failure.to_string().contains(field), "{failure}");
    }
    Ok(())
}
