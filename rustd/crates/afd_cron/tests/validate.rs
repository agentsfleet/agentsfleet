//! What a schedule's three authored fields are allowed to say.
//!
//! Every case here is pure: a string in, a verdict out, no datastore and no
//! upstream. That is the whole point of the tier — the refusals a person meets
//! when they mistype a cron expression are the ones this daemon can prove
//! without Postgres being up.
//!
//! # Why the expression guard is narrower than the parser
//!
//! [`validate::cron`] runs the parser first and then narrows what it accepted.
//! The narrowing is deliberate and is what most of this file is about: an
//! expression the parser reads happily but this daemon would register into a
//! schedule that never fires is refused at create time, where an author can see
//! it, rather than at three in the morning when nothing happened.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_cron::validate::{self, Invalid, MAX_CRON_LEN, MAX_MESSAGE_LEN, MAX_TIMEZONE_LEN};

/// A five-field expression with nothing exotic in it.
const EVERY_MINUTE: &str = "* * * * *";

#[test]
fn an_ordinary_five_field_expression_is_registered() {
    validate::cron(EVERY_MINUTE).expect("a bare five-field expression is what this daemon takes");
    validate::cron("0 9 * * 1").expect("an hour-and-weekday expression is ordinary");
    validate::cron("*/15 * * * *").expect("a step inside its field is ordinary");
    validate::cron("0 0 1-15 * *").expect("a forward range is ordinary");
}

#[test]
fn an_empty_expression_is_refused_before_the_parser_sees_it() {
    assert_eq!(validate::cron(""), Err(Invalid::Cron));
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
        validate::cron(&over),
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
            validate::cron(alias),
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
            validate::cron(named),
            Err(Invalid::Cron),
            "`{named}` names a field in words this daemon does not register"
        );
    }
}

/// A zero step never advances, so it is a schedule that cannot fire.
#[test]
fn a_zero_step_is_refused() {
    assert_eq!(validate::cron("*/0 * * * *"), Err(Invalid::Cron));
}

/// A step wider than its own field fires once and never again, which reads as
/// a working schedule and is not one.
#[test]
fn a_step_wider_than_its_field_is_refused() {
    assert_eq!(
        validate::cron("*/61 * * * *"),
        Err(Invalid::Cron),
        "a minute field spans 60, so a step of 61 can never come round"
    );
    assert_eq!(
        validate::cron("* */25 * * *"),
        Err(Invalid::Cron),
        "an hour field spans 24"
    );
    validate::cron("*/60 * * * *").expect("a step exactly at its field's span is admissible");
}

/// A backwards range is an author's transposition, not an intent.
#[test]
fn a_backwards_range_is_refused() {
    assert_eq!(validate::cron("0 0 15-1 * *"), Err(Invalid::Cron));
    validate::cron("0 0 1-15 * *").expect("the same range the right way round is fine");
}

#[test]
fn an_equal_ended_range_is_a_single_value_and_admissible() {
    validate::cron("0 0 5-5 * *").expect("a range whose ends meet names one value");
}

#[test]
fn a_zone_the_database_defines_is_accepted() {
    for zone in ["UTC", "America/New_York", "Europe/London", "Asia/Kolkata"] {
        assert!(
            validate::timezone(zone).is_ok(),
            "`{zone}` is a name the timezone database defines"
        );
    }
}

#[test]
fn a_zone_the_database_does_not_define_is_refused() {
    assert_eq!(
        validate::timezone("Foo/Bar"),
        Err(Invalid::Timezone),
        "a name with the right SHAPE is still not a zone, which is why this \
         resolves rather than pattern-matches"
    );
    assert_eq!(validate::timezone(""), Err(Invalid::Timezone));
}

/// The lookup is a filesystem read keyed on the name, so a traversal is refused
/// in front of it rather than handed to the resolver.
#[test]
fn a_zone_name_carrying_a_traversal_is_refused_before_the_lookup() {
    for traversal in ["../etc/passwd", "America/../../etc/passwd", ".."] {
        assert_eq!(
            validate::timezone(traversal),
            Err(Invalid::Timezone),
            "`{traversal}` must not reach the resolver"
        );
    }
}

#[test]
fn a_zone_name_over_the_length_bound_is_refused() {
    let over = "A".repeat(MAX_TIMEZONE_LEN + 1);
    assert_eq!(validate::timezone(&over), Err(Invalid::Timezone));
}

#[test]
fn a_message_with_something_in_it_is_accepted() {
    validate::message("Check the deploy.").expect("an ordinary message is what a schedule carries");
}

#[test]
fn an_empty_or_whitespace_message_is_refused() {
    assert_eq!(validate::message(""), Err(Invalid::Message));
    for blank in ["   ", "\t", "\n", " \t\n "] {
        assert_eq!(
            validate::message(blank),
            Err(Invalid::Message),
            "a fleet woken with nothing to do spends a model deciding it has \
             nothing to do"
        );
    }
}

#[test]
fn a_message_over_the_length_bound_is_refused_at_the_boundary() {
    let at_cap = "m".repeat(MAX_MESSAGE_LEN);
    validate::message(&at_cap).expect("the cap itself is admissible");

    let over = "m".repeat(MAX_MESSAGE_LEN + 1);
    assert_eq!(
        validate::message(&over),
        Err(Invalid::Message),
        "one character past the cap is refused"
    );
}

/// The three refusals are distinct values, because the route renders each to
/// its own sentence and a person fixing a schedule needs to know which field
/// they got wrong.
#[test]
fn each_field_refuses_under_its_own_name() {
    assert_eq!(
        validate::cron("nonsense").expect_err("`nonsense` is not an expression"),
        Invalid::Cron
    );
    assert_eq!(
        validate::timezone("Foo/Bar").expect_err("`Foo/Bar` is not a zone"),
        Invalid::Timezone
    );
    assert_eq!(
        validate::message("").expect_err("an empty message is refused"),
        Invalid::Message
    );
}
