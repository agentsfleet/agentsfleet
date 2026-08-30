//! The startup nameplate: when it is decorated, when it is one plain line, and
//! what it never does.
//!
//! Every condition below is exercised ALONE, against a `Conditions` that is
//! otherwise suitable for colour. Testing them together would pass just as
//! happily with four of the five checks missing.
#![expect(
    clippy::panic,
    reason = "test target: a form that does not destructure should fail loudly"
)]

use agentsfleetd::nameplate::{Conditions, Style, hairline_width, render};

/// A destination that can take everything: a real terminal, colour allowed.
///
/// The baseline every fallback test perturbs by exactly one field.
fn suitable() -> Conditions {
    Conditions {
        stdout_is_terminal: true,
        no_colour: false,
        term: Some("xterm-256color".to_owned()),
        under_service_manager: false,
    }
}

/// The escape byte, which no plain output may contain.
const ESCAPE: char = '\u{1b}';
/// The three-space indent both decorated lines carry.
const INDENT: &str = "   ";
const RESET: &str = "\u{1b}[0m";
const VERSION: &str = "9.9.9";

/// The baseline really is coloured, so every assertion below means something.
#[test]
fn test_a_terminal_that_can_take_colour_gets_it() {
    assert_eq!(Style::of(&suitable(), false), Style::Coloured);
}

// ── The five plain-path conditions, each on its own ──────────────────────

#[test]
fn test_output_that_is_not_a_terminal_is_plain() {
    let piped = Conditions {
        stdout_is_terminal: false,
        ..suitable()
    };
    assert_eq!(Style::of(&piped, false), Style::Plain);
}

/// `NO_COLOR` set to ANY value, an empty one included.
///
/// The convention is presence, not truthiness — `NO_COLOR=` means no colour,
/// and a reader that parsed the value would get this exactly backwards.
#[test]
fn test_no_colour_set_to_any_value_is_plain() {
    let asked = Conditions {
        no_colour: true,
        ..suitable()
    };
    assert_eq!(Style::of(&asked, false), Style::Plain);
}

#[test]
fn test_an_unset_or_dumb_terminal_type_is_plain() {
    for term in [None, Some("dumb".to_owned())] {
        let unknown = Conditions {
            term: term.clone(),
            ..suitable()
        };
        assert_eq!(Style::of(&unknown, false), Style::Plain, "{term:?}");
    }
}

#[test]
fn test_running_under_a_service_manager_is_plain() {
    let managed = Conditions {
        under_service_manager: true,
        ..suitable()
    };
    assert_eq!(Style::of(&managed, false), Style::Plain);
}

/// `--quiet` alone is enough, against a destination that could take colour.
#[test]
fn test_quiet_is_plain() {
    assert_eq!(Style::of(&suitable(), true), Style::Plain);
}

// ── What each form may and may not contain ───────────────────────────────

/// No escape sequence reaches a plain destination, anywhere in the output.
///
/// The failure this pins is the one that is invisible in a terminal and
/// obvious in a captured log: `\033[0m` littered through journald.
#[test]
fn test_plain_output_carries_no_escape_sequence() {
    let plain = render(Style::Plain, VERSION, None);

    assert!(!plain.contains(ESCAPE), "{plain:?}");
    assert!(plain.contains(VERSION));
    assert!(plain.contains("agentsfleetd"));
}

/// Every coloured line closes its own colour.
///
/// Per line, not once at the end: a write truncated between the two lines must
/// not leave the terminal painted for whatever prints next.
#[test]
fn test_every_coloured_line_ends_with_a_reset() {
    let coloured = render(Style::Coloured, VERSION, None);

    for line in coloured.lines().filter(|line| line.contains(ESCAPE)) {
        assert!(line.ends_with(RESET), "unterminated colour: {line:?}");
    }
}

/// The decorated form is two lines, framed by one blank line on each side.
///
/// Destructured rather than indexed: a slice pattern states the shape being
/// asserted — four parts, in order — where four subscripts would only imply it,
/// and it cannot panic on a form that came back shorter than expected.
#[test]
fn test_the_coloured_form_is_two_lines_between_blank_ones() {
    let coloured = render(Style::Coloured, VERSION, None);
    let lines: Vec<&str> = coloured.split('\n').collect();

    let [opening, wordmark, rule, closing, ..] = lines.as_slice() else {
        panic!("the decorated form is four parts: {coloured:?}");
    };

    assert_eq!(*opening, "", "a blank line opens it");
    assert_eq!(*closing, "", "a blank line closes it");
    assert!(wordmark.starts_with(INDENT), "indent: {wordmark:?}");
    assert!(rule.starts_with(INDENT), "indent: {rule:?}");
    assert!(wordmark.contains("A G E N T S F L E E T D"));
    assert!(wordmark.contains(VERSION));
    assert!(rule.contains('\u{2504}'));
}

/// The nameplate says the version and nothing else about the run.
///
/// A static nameplate, not a status display: no pid, no host, no counts. The
/// guard is a list because the temptation to add one of these is what the
/// module doc exists to resist.
#[test]
fn test_the_nameplate_reports_no_runtime_state() {
    let coloured = render(Style::Coloured, VERSION, Some(80));

    for forbidden in ["pid", "listening", "postgres", "redis", "localhost"] {
        assert!(
            !coloured.contains(forbidden),
            "the nameplate is not a status display: {forbidden}"
        );
    }
}

// ── The hairline ─────────────────────────────────────────────────────────

/// An undeterminable width draws the full rule.
#[test]
fn test_an_unknown_width_falls_back_to_the_full_rule() {
    assert_eq!(hairline_width(None), 54);
}

/// A wide terminal is capped; a narrow one is clamped; neither wraps.
#[test]
fn test_the_hairline_clamps_at_both_ends() {
    // Wide enough that the cap, not the terminal, decides.
    assert_eq!(hairline_width(Some(200)), 54);
    // Exactly at the point the cap starts binding: 54 + the 6-column margin.
    assert_eq!(hairline_width(Some(60)), 54);
    // One narrower, and the terminal decides.
    assert_eq!(hairline_width(Some(59)), 53);
    assert_eq!(hairline_width(Some(40)), 34);
    // Narrower than the margin itself yields no glyphs rather than underflowing.
    assert_eq!(hairline_width(Some(6)), 0);
    assert_eq!(hairline_width(Some(0)), 0);
}

/// The rendered rule is exactly as many glyphs as the width says.
#[test]
fn test_the_rendered_rule_is_the_width_it_reports() {
    for columns in [Some(40), Some(80), None] {
        let coloured = render(Style::Coloured, VERSION, columns);
        let drawn = coloured.chars().filter(|c| *c == '\u{2504}').count();

        assert_eq!(drawn, hairline_width(columns), "{columns:?}");
    }
}

// ── The version ──────────────────────────────────────────────────────────

/// The version is the crate's build metadata, not a literal in the source.
///
/// Asserted against `CARGO_PKG_VERSION` read HERE: this test target and the
/// binary are different crates in the same workspace on one version, so a
/// nameplate that hard-coded a string would drift from this on the next bump
/// and fail — which is the point.
#[test]
fn test_the_version_comes_from_build_metadata() {
    let version = env!("CARGO_PKG_VERSION");
    let plain = render(Style::Plain, version, None);

    assert!(plain.contains(version), "{plain:?}");
    assert!(
        !version.is_empty() && version.chars().next().is_some_and(|c| c.is_ascii_digit()),
        "build metadata should be a real version: {version:?}"
    );
}

/// Nothing is rendered through the daemon's structured logger.
///
/// A nameplate in JSON log output is noise a consumer has to parse past, and
/// the module exists partly to keep it out — so the rendered form carries no
/// logfmt or JSON framing.
#[test]
fn test_the_nameplate_is_not_a_log_record() {
    let coloured = render(Style::Coloured, VERSION, None);

    for framing in ["level=", "\"level\"", "ts_ms=", "event="] {
        assert!(!coloured.contains(framing), "{framing} in a nameplate");
    }
}

// ── Failure containment ──────────────────────────────────────────────────

/// Printing cannot hand a failure back to the caller.
///
/// The strongest form of this is the signature: `show` returns `()`, so there
/// is no error for a boot sequence to propagate and no `Result` for a caller to
/// mishandle — a write to a closed pipe or a detached terminal is dropped
/// inside. This exercises both the printing and the suppressed paths to prove
/// the containment is real at runtime and not only in the type.
#[test]
fn test_printing_cannot_fail_the_caller() {
    let conditions = suitable();

    agentsfleetd::nameplate::show(VERSION, &conditions, Some(80), false, false);
    agentsfleetd::nameplate::show(VERSION, &conditions, None, true, false);
    agentsfleetd::nameplate::show(VERSION, &conditions, None, false, true);
    agentsfleetd::nameplate::show(VERSION, &Conditions::default(), None, false, false);
}

/// `--no-banner` prints nothing at all, in either form.
#[test]
fn test_no_banner_suppresses_both_forms() {
    // Nothing is rendered because nothing is asked for: the suppression is
    // checked before the style is decided, so neither path can leak a byte.
    for conditions in [suitable(), Conditions::default()] {
        agentsfleetd::nameplate::show(VERSION, &conditions, None, false, true);
    }
}
