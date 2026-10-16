//! What an operator actually sees: the banner on the way up, the bomb on the
//! way out, and the rule that decides whether either may use colour.
//!
//! These assert on the RENDERED string rather than on a print, which is the
//! only reason they exist in-process at all — `banner::show` and `fatal::die`
//! are one line each precisely so that everything above them is assertable
//! without a pseudo-terminal.
use std::error::Error;
use std::fmt;

use agentsfleetd::banner;
use agentsfleetd::fatal;
use agentsfleetd::tty::Rendering;

/// The escape byte every ANSI sequence starts with.
const ESCAPE: char = '\u{1b}';

/// A version string that is obviously a fixture rather than a real release.
const VERSION: &str = "9.9.9";

/// An error with a cause, so the chain walk has something to walk.
#[derive(Debug)]
struct Layered {
    message: &'static str,
    cause: Option<Box<Layered>>,
}

impl fmt::Display for Layered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for Layered {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause
            .as_ref()
            .map(|cause| cause.as_ref() as &(dyn Error + 'static))
    }
}

fn roles() -> Vec<String> {
    vec!["postgres:api".to_owned(), "redis:api".to_owned()]
}

/// A terminal gets colour; anything else gets text.
///
/// Both inputs matter and both are asserted: `NO_COLOR` on a real terminal is
/// the case the convention exists for, and it is the one a `is_terminal()`
/// check alone would get wrong.
#[test]
fn test_rendering_is_rich_only_on_an_opted_in_terminal() {
    assert_eq!(Rendering::for_terminal(true, false), Rendering::Rich);
    assert_eq!(
        Rendering::for_terminal(true, true),
        Rendering::Plain,
        "NO_COLOR on a terminal must still be plain"
    );
    assert_eq!(Rendering::for_terminal(false, false), Rendering::Plain);
    assert_eq!(Rendering::for_terminal(false, true), Rendering::Plain);
}

/// Painting is a no-op unless the destination can take it.
#[test]
fn test_plain_rendering_emits_no_escape_sequences() {
    let painted = Rendering::Plain.paint("1;31", "danger");
    assert_eq!(
        painted, "danger",
        "a pipe must receive the text and nothing else"
    );

    let painted = Rendering::Rich.paint("1;31", "danger");
    assert!(painted.contains(ESCAPE), "a terminal gets the escape");
    assert!(painted.contains("danger"), "and still gets the text");
}

/// The plain banner is one greppable line naming build, roles and pid.
#[test]
fn test_banner_is_one_plain_line_off_a_terminal() {
    let rendered = banner::render(Rendering::Plain, VERSION, &roles(), 4242);

    assert_eq!(
        rendered.lines().count(),
        1,
        "journald gets one line, not a drawing: {rendered}"
    );
    assert!(!rendered.contains(ESCAPE), "no escapes off a terminal");
    assert!(
        rendered.contains("agentsfleetd"),
        "the product names itself"
    );
    assert!(rendered.contains(VERSION), "the build is identified");
    assert!(rendered.contains("postgres:api"), "the roles are named");
    assert!(rendered.contains("4242"), "the pid is named");
}

/// The rich banner draws the mark, and still carries every fact.
#[test]
fn test_banner_draws_the_mark_on_a_terminal() {
    let rendered = banner::render(Rendering::Rich, VERSION, &roles(), 4242);

    assert_eq!(
        rendered.lines().count(),
        2,
        "the mark is two rows beside the text, not a box: {rendered}"
    );
    assert!(rendered.contains(ESCAPE), "a terminal gets colour");
    assert!(rendered.contains("agentsfleetd"));
    assert!(rendered.contains(VERSION));
    assert!(rendered.contains("redis:api"));
}

/// The fatal renders the message and every cause.
#[test]
fn test_fatal_renders_the_whole_causal_chain() {
    let error = Layered {
        message: "boot refused",
        cause: Some(Box::new(Layered {
            message: "master key unusable",
            cause: Some(Box::new(Layered {
                message: "expected 64 hex characters",
                cause: None,
            })),
        })),
    };

    let rendered = fatal::render(&error, Rendering::Plain);

    assert!(rendered.contains("boot refused"), "the outermost failure");
    assert!(
        rendered.contains("master key unusable"),
        "the cause the Ruby original had no way to show"
    );
    assert!(
        rendered.contains("expected 64 hex characters"),
        "and the cause beneath that one"
    );
    assert_eq!(
        rendered.matches("caused by:").count(),
        2,
        "one line per cause, not per error: {rendered}"
    );
    assert!(!rendered.contains(ESCAPE), "plain stays plain");
}

/// An error with no cause still gets its message.
///
/// The Ruby original returned early when the filtered trace was empty, so the
/// loudest failures produced the quietest output. This is that bug, asserted
/// closed.
#[test]
fn test_fatal_still_speaks_for_an_error_with_no_cause() {
    let error = Layered {
        message: "nothing beneath this",
        cause: None,
    };

    let rendered = fatal::render(&error, Rendering::Plain);

    assert!(rendered.contains("nothing beneath this"));
    assert!(
        !rendered.contains("caused by:"),
        "no chain means no chain lines"
    );
}

/// On a terminal the fatal is coloured; the content is unchanged.
#[test]
fn test_fatal_colours_only_on_a_terminal() {
    let error = Layered {
        message: "boot refused",
        cause: None,
    };

    let rich = fatal::render(&error, Rendering::Rich);
    assert!(rich.contains(ESCAPE), "a terminal gets colour");
    assert!(rich.contains("boot refused"), "and the same message");
}

/// Only our frames survive, each from its crate name onward.
///
/// A captured backtrace is mostly `core::` and `std::` frames, and a trace that
/// is ninety per cent runtime is a trace nobody reads. Our frames keep the path
/// from the crate directory on: with every module root a `mod.rs`, a bare last
/// segment would no longer say which file panicked.
#[test]
fn test_fatal_keeps_only_our_frames_from_their_crate() {
    let backtrace = "\
   0: core::panicking::panic_fmt
             at /rustc/aaaa/library/core/src/panicking.rs:75:14
   1: agentsfleetd::preflight::read_kek
             at /home/build/rustd/crates/agentsfleetd/src/preflight.rs:210:9
   2: afd_crypto::secret::Kek::from_hex
             at /home/build/rustd/crates/afd_crypto/src/secret.rs:50:9
   3: afd_core::error_code::lookup
             at /home/build/rustd/crates/afd_core/src/error_code/mod.rs:12:5
   4: afd_db::config::read
             at /home/build/rustd/crates/afd_db/src/config/mod.rs:40:7
   5: std::rt::lang_start_internal
             at /rustc/aaaa/library/std/src/rt.rs:148:20";

    let error = Layered {
        message: "boot refused",
        cause: None,
    };
    let rendered = fatal::render_with_trace(&error, Rendering::Plain, backtrace);

    for frame in [
        "agentsfleetd/src/preflight.rs:210:9",
        "afd_crypto/src/secret.rs:50:9",
        "afd_core/src/error_code/mod.rs:12:5",
        "afd_db/src/config/mod.rs:40:7",
    ] {
        assert!(
            rendered.contains(frame),
            "our frame keeps its crate path, {frame}: {rendered}"
        );
    }
    assert!(
        rendered.contains("afd_core::error_code::lookup"),
        "a function-symbol line is kept whole: {rendered}"
    );
    assert!(
        !rendered.contains("panicking.rs"),
        "a core:: frame is not ours: {rendered}"
    );
    assert!(
        !rendered.contains("rt.rs"),
        "nor is a std:: frame: {rendered}"
    );
    assert!(
        !rendered.contains("/home/build/"),
        "the build machine's path before the crate is dropped: {rendered}"
    );
}

/// A frame whose path names no crate directory falls back to its last segment.
#[test]
fn test_fatal_frame_outside_the_crates_tree_keeps_its_last_segment() {
    let backtrace = "   1: agentsfleetd::main\n             at /opt/agentsfleetd/main.rs:9:1";
    let error = Layered {
        message: "boot refused",
        cause: None,
    };
    let rendered = fatal::render_with_trace(&error, Rendering::Plain, backtrace);
    assert!(rendered.contains("main.rs:9:1"), "{rendered}");
    assert!(!rendered.contains("/opt/"), "{rendered}");
}

/// A backtrace with nothing of ours in it adds no frame block at all.
#[test]
fn test_fatal_adds_no_frame_block_when_nothing_is_ours() {
    let error = Layered {
        message: "boot refused",
        cause: None,
    };

    let rendered = fatal::render_with_trace(
        &error,
        Rendering::Plain,
        "   0: std::rt::lang_start_internal\n             at /rustc/aaaa/library/std/src/rt.rs:148:20",
    );

    assert!(rendered.contains("boot refused"));
    assert!(
        !rendered.contains("rt.rs"),
        "a trace with none of our frames contributes nothing: {rendered}"
    );
}
