//! A cron trigger's schedule: one check for the definition parse and the
//! daemon's schedule routes.

use garde::Unvalidated;
use serde_json::json;

use super::{CronSchedule, REASON_INVALID_SCHEDULE, check_schedule};
use crate::Definition;
use crate::error::{Error, ErrorKind};

/// The minute field spans 0 to 59, so minute 61 is outside it.
const MINUTE_61: &str = "61 * * * *";

/// A definition whose one cron trigger fires on `schedule`.
fn definition(schedule: &str) -> String {
    json!({
        "schema_version": 1,
        "description": "Checks the deploy every morning.",
        "instructions_md": "Check the deploy and report what changed.",
        "config": {"name": "deploy-check", "x-agentsfleet": {
            "triggers": [{"type": "cron", "schedule": schedule, "timezone": "UTC", "message": "Daily check"}],
            "tools": ["agentmail"],
            "budget": {"daily_nanos": "1000000000"}
        }}
    })
    .to_string()
}

/// The verdict [`check_schedule`] gives `expression`, once its bound held.
fn checked(expression: &str) -> Result<(), Error> {
    let proved = Unvalidated::new(CronSchedule { expression })
        .validate()
        .map_err(|source| Error::from(ErrorKind::OutOfBounds { source }))?;
    check_schedule(&proved)
}

/// Whether `expression` breaks [`CronSchedule`]'s bound, reported under its
/// one field.
fn breaks_the_bound(expression: &str) -> bool {
    matches!(
        Unvalidated::new(CronSchedule { expression }).validate(),
        Err(report) if report.iter().map(|(path, _error)| path.to_string()).eq(["expression"])
    )
}

/// Whether `outcome` is the schedule refusal, naming `schedule` under the
/// `triggers` setting.
fn is_schedule_refusal<T>(outcome: &Result<T, Error>) -> bool {
    matches!(
        outcome,
        Err(refused) if matches!(
            refused.kind(),
            ErrorKind::InvalidTriggerSet { reason } if *reason == REASON_INVALID_SCHEDULE
        ) && refused.field() == Some("triggers")
            && refused.to_string().contains("`schedule`")
    )
}

/// The definition parse refuses minute 61 through the same function the
/// daemon's schedule routes call, so a runner's proposal check and a schedule
/// route reach one verdict.
#[test]
fn af_fleet_runtime_refuses_an_invalid_cron_schedule() -> Result<(), Error> {
    assert!(is_schedule_refusal(&Definition::parse(&definition(
        MINUTE_61
    ))));
    assert!(is_schedule_refusal(&checked(MINUTE_61)));

    let accepted = Definition::parse(&definition("0 9 * * *"))?;
    assert!(matches!(
        accepted.runtime().triggers().first(),
        Some(crate::config::Trigger::Cron(cron)) if &*cron.schedule == "0 9 * * *"
    ));
    Ok(())
}

/// The parser reads `@daily` and `MON`; the scheduler is not handed either.
#[test]
fn an_alias_or_a_named_field_is_refused_though_the_parser_reads_it() {
    for expression in [
        "@daily",
        "@hourly",
        " @weekly",
        "0 0 * * MON",
        "0 0 * JAN *",
        "0 0 * * SUN-SAT",
    ] {
        assert!(
            is_schedule_refusal(&checked(expression)),
            "`{expression}` is not five numeric fields"
        );
        assert!(
            is_schedule_refusal(&Definition::parse(&definition(expression))),
            "the definition parse refuses `{expression}` too"
        );
    }
}

/// A step past its field never comes round, and a zero step never advances.
#[test]
fn a_step_outside_its_field_is_refused() -> Result<(), Error> {
    for expression in ["*/61 * * * *", "* */25 * * *", "*/0 * * * *", "*/x * * * *"] {
        assert!(
            is_schedule_refusal(&checked(expression)),
            "`{expression}` has a step no field can take"
        );
    }
    checked("*/60 * * * *")?;
    checked("*/15 * * * *")?;
    Ok(())
}

/// A backwards range is an author's transposition, and two parsers read it two
/// ways; a range whose ends meet names one value.
#[test]
fn a_backwards_range_is_refused_and_an_equal_one_is_not() -> Result<(), Error> {
    assert!(is_schedule_refusal(&checked("0 0 15-1 * *")));
    assert!(is_schedule_refusal(&checked("0 0 15-1/2 * *")));
    checked("0 0 1-15 * *")?;
    checked("0 0 5-5 * *")?;
    checked("0 9 * * 1-5")?;
    Ok(())
}

/// Text that is no cron expression at all, in any script, is the parser's own
/// refusal and reaches the caller as the same one.
#[test]
fn malformed_text_is_refused_by_the_parser() {
    for expression in [
        "every morning",
        "* * * *",
        "* * * * * * *",
        "٣ * * * *",
        "0 9 * * * 🙂",
    ] {
        assert!(
            is_schedule_refusal(&checked(expression)),
            "`{expression}` is not a cron expression"
        );
    }
}

/// A long expression is judged by the parser, not refused for its length:
/// only the text ceiling bounds it, and an empty one holds no bound.
#[test]
fn a_long_cron_expression_is_judged_by_the_parser() {
    let valid = format!("{}7 * * * *", "7,".repeat(100));
    assert!(!breaks_the_bound(&valid), "300 characters hold the bound");
    assert!(
        Definition::parse(&definition(&valid)).is_ok(),
        "a long valid list parses"
    );
    let junk = format!("{} * * * *", "x".repeat(200));
    assert!(!breaks_the_bound(&junk));
    assert!(is_schedule_refusal(&Definition::parse(&definition(&junk))));
    assert!(breaks_the_bound(""), "an empty expression holds no bound");
}
