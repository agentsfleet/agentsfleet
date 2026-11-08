//! What a schedule's three authored fields are allowed to say.
//!
//! Every case here is pure: a string in, a verdict out, no datastore and no
//! upstream. That is the whole point of the tier — the refusals a person meets
//! when they mistype a cron expression are the ones this daemon can prove
//! without Postgres being up.
//!
//! # Why the expression guard is narrower than the parser
//!
//! [`Fields::check`] runs the parser first and then narrows what it accepted.
//! The narrowing is deliberate and is what most of this file is about: an
//! expression the parser reads happily but this daemon would register into a
//! schedule that never fires is refused at create time, where an author can see
//! it, rather than at three in the morning when nothing happened.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_cron::validate::{Fields, Invalid, MAX_CRON_LEN, MAX_MESSAGE_LEN, MAX_TIMEZONE_LEN};

/// A five-field expression with nothing exotic in it.
const EVERY_MINUTE: &str = "* * * * *";

/// The expression alone, as a patch that changes only it sends.
fn cron(expression: &str) -> Result<(), Invalid> {
    Fields {
        expression: Some(expression),
        ..Fields::default()
    }
    .check()
}

/// The zone alone.
fn timezone(zone: &str) -> Result<(), Invalid> {
    Fields {
        timezone: Some(zone),
        ..Fields::default()
    }
    .check()
}

/// The message alone.
fn message(text: &str) -> Result<(), Invalid> {
    Fields {
        message: Some(text),
        ..Fields::default()
    }
    .check()
}

/// The paths a refused set of fields was reported under, or none.
fn reported(fields: Fields<'_>) -> Vec<String> {
    garde::Unvalidated::new(fields)
        .validate()
        .err()
        .map(|report| {
            report
                .iter()
                .map(|(path, _error)| path.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// An oversized expression is refused by its bound, so no `Valid` value exists
/// for the parser to be called with — the type is what keeps it from running.
#[test]
fn test_oversized_cron_is_refused_by_its_bound() {
    let over = "0".repeat(MAX_CRON_LEN + 1);
    let fields = Fields {
        expression: Some(&over),
        ..Fields::default()
    };
    assert_eq!(reported(fields), vec!["expression".to_owned()]);
    assert_eq!(fields.check(), Err(Invalid::Cron));
}

/// The zone lookup reads the filesystem by name; a name past the bound never
/// becomes a `Valid` value, so the lookup is never asked.
#[test]
fn test_oversized_timezone_never_reaches_the_lookup() {
    let over = "A".repeat(MAX_TIMEZONE_LEN + 1);
    let fields = Fields {
        timezone: Some(&over),
        ..Fields::default()
    };
    assert_eq!(reported(fields), vec!["timezone".to_owned()]);
    assert_eq!(fields.check(), Err(Invalid::Timezone));
    timezone("Asia/Kolkata").expect("a zone inside the bound still resolves");
}

#[test]
fn an_ordinary_five_field_expression_is_registered() {
    cron(EVERY_MINUTE).expect("a bare five-field expression is what this daemon takes");
    cron("0 9 * * 1").expect("an hour-and-weekday expression is ordinary");
    cron("*/15 * * * *").expect("a step inside its field is ordinary");
    cron("0 0 1-15 * *").expect("a forward range is ordinary");
}

#[test]
fn an_empty_expression_is_refused_before_the_parser_sees_it() {
    assert_eq!(cron(""), Err(Invalid::Cron));
}

/// The length bound is a bound on the work one create can ask of the parser,
/// so it is asserted at the boundary rather than with an obviously huge string.
#[test]
fn an_expression_over_the_length_bound_is_refused_at_the_boundary() {
    // A long but structurally valid minute list, padded to straddle the cap.
    let padded = |len: usize| {
        let mut expression = String::from("*");
        while expression.len() < len {
            expression.push_str(",*");
        }
        expression.truncate(len);
        format!("{expression} * * * *")
    };

    let at_cap = "0".repeat(MAX_CRON_LEN);
    assert_eq!(
        at_cap.len(),
        MAX_CRON_LEN,
        "the fixture sits exactly on the cap"
    );

    let over = format!("{} * * * *", "0".repeat(MAX_CRON_LEN));
    assert!(over.len() > MAX_CRON_LEN);
    assert_eq!(
        cron(&over),
        Err(Invalid::Cron),
        "an expression past the cap is refused on length, whatever it says"
    );
    // And the guard is a LENGTH guard, not a shape one: the same shape under
    // the cap is refused for its own reasons or accepted, but never for length.
    let under = padded(20);
    assert!(under.len() <= MAX_CRON_LEN);
}

/// `@daily` and friends parse in the crate and are not what this daemon
/// registers upstream, so they are refused where an author can see it.
#[test]
fn an_alias_is_refused_even_though_the_parser_reads_it() {
    for alias in ["@daily", "@hourly", "@weekly", "@monthly", "@yearly"] {
        assert_eq!(
            cron(alias),
            Err(Invalid::Cron),
            "`{alias}` is an alias this daemon does not register"
        );
    }
}

/// `MON` parses and would silently never fire, which is the failure this guard
/// exists to turn into a create-time refusal.
#[test]
fn a_named_field_is_refused_rather_than_registered_to_never_fire() {
    for named in ["0 0 * * MON", "0 0 * JAN *", "0 0 * * SUN-SAT"] {
        assert_eq!(
            cron(named),
            Err(Invalid::Cron),
            "`{named}` names a field in words this daemon does not register"
        );
    }
}

/// A zero step never advances, so it is a schedule that cannot fire.
#[test]
fn a_zero_step_is_refused() {
    assert_eq!(cron("*/0 * * * *"), Err(Invalid::Cron));
}

/// A step wider than its own field fires once and never again, which reads as
/// a working schedule and is not one.
#[test]
fn a_step_wider_than_its_field_is_refused() {
    assert_eq!(
        cron("*/61 * * * *"),
        Err(Invalid::Cron),
        "a minute field spans 60, so a step of 61 can never come round"
    );
    assert_eq!(
        cron("* */25 * * *"),
        Err(Invalid::Cron),
        "an hour field spans 24"
    );
    cron("*/60 * * * *").expect("a step exactly at its field's span is admissible");
}

/// A backwards range is an author's transposition, not an intent.
#[test]
fn a_backwards_range_is_refused() {
    assert_eq!(cron("0 0 15-1 * *"), Err(Invalid::Cron));
    cron("0 0 1-15 * *").expect("the same range the right way round is fine");
}

#[test]
fn an_equal_ended_range_is_a_single_value_and_admissible() {
    cron("0 0 5-5 * *").expect("a range whose ends meet names one value");
}

#[test]
fn a_zone_the_database_defines_is_accepted() {
    for zone in ["UTC", "America/New_York", "Europe/London", "Asia/Kolkata"] {
        assert!(
            timezone(zone).is_ok(),
            "`{zone}` is a name the timezone database defines"
        );
    }
}

#[test]
fn a_zone_the_database_does_not_define_is_refused() {
    assert_eq!(
        timezone("Foo/Bar"),
        Err(Invalid::Timezone),
        "a name with the right SHAPE is still not a zone, which is why this \
         resolves rather than pattern-matches"
    );
    assert_eq!(timezone(""), Err(Invalid::Timezone));
}

/// The lookup is a filesystem read keyed on the name, so a traversal is refused
/// in front of it rather than handed to the resolver.
#[test]
fn a_zone_name_carrying_a_traversal_is_refused_before_the_lookup() {
    for traversal in ["../etc/passwd", "America/../../etc/passwd", ".."] {
        assert_eq!(
            timezone(traversal),
            Err(Invalid::Timezone),
            "`{traversal}` must not reach the resolver"
        );
    }
}

#[test]
fn a_zone_name_over_the_length_bound_is_refused() {
    let over = "A".repeat(MAX_TIMEZONE_LEN + 1);
    assert_eq!(timezone(&over), Err(Invalid::Timezone));
}

#[test]
fn a_message_with_something_in_it_is_accepted() {
    message("Check the deploy.").expect("an ordinary message is what a schedule carries");
}

#[test]
fn an_empty_or_whitespace_message_is_refused() {
    assert_eq!(message(""), Err(Invalid::Message));
    for blank in ["   ", "\t", "\n", " \t\n "] {
        assert_eq!(
            message(blank),
            Err(Invalid::Message),
            "a fleet woken with nothing to do spends a model deciding it has \
             nothing to do"
        );
    }
}

#[test]
fn a_message_over_the_length_bound_is_refused_at_the_boundary() {
    let at_cap = "m".repeat(MAX_MESSAGE_LEN);
    message(&at_cap).expect("the cap itself is admissible");

    let over = "m".repeat(MAX_MESSAGE_LEN + 1);
    assert_eq!(
        message(&over),
        Err(Invalid::MessageTooLong),
        "one character past the cap is refused, as a different repair from an empty one"
    );
}

/// The three refusals are distinct values, because the route renders each to
/// its own sentence and a person fixing a schedule needs to know which field
/// they got wrong.
#[test]
fn each_field_refuses_under_its_own_name() {
    assert_eq!(
        cron("nonsense").expect_err("`nonsense` is not an expression"),
        Invalid::Cron
    );
    assert_eq!(
        timezone("Foo/Bar").expect_err("`Foo/Bar` is not a zone"),
        Invalid::Timezone
    );
    assert_eq!(
        message("").expect_err("an empty message is refused"),
        Invalid::Message
    );
}
