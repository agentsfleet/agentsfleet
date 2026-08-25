//! The one place an error becomes something an operator reads, then an exit.
//!
//! # Lineage
//!
//! Modelled on a Ruby `GlobalExceptions` mixin Indy wrote years ago: print the
//! message, filter the backtrace down to OUR frames, keep only the last path
//! segment of each, and finish with a bomb and `We flunked!`. The instinct it
//! encodes is the right one — a stack trace that is ninety per cent framework
//! is a trace nobody reads — and it is kept here.
//!
//! Four things are deliberately different, and each is a bug the original had:
//!
//! 1. **Colour is conditional.** The Ruby emitted `\033[1m` unconditionally, so
//!    a captured log got escape sequences instead of emphasis. [`Rendering`]
//!    decides once, and honours `NO_COLOR`.
//! 2. **It writes to standard error.** The original logged at DEBUG, so the
//!    account of why the process died could be filtered out by log level — the
//!    one message that must never be suppressed.
//! 3. **It walks the `source()` chain.** Ruby had no equivalent; in Rust the
//!    causal chain is where the actual explanation lives, and a backtrace is
//!    the fallback rather than the substance.
//! 4. **An empty trace still prints.** `unless trace.empty?` meant an error
//!    raised outside our files printed a message and no bomb — the loudest
//!    failures got the quietest output.
//!
//! # One place
//!
//! [`report`] is the only renderer of a fatal in this crate, and `main` is the
//! only caller. Nothing else prints on the way out: a library that writes to
//! stderr on failure gives a caller no way not to.

use std::error::Error;

use crate::tty::{BOLD, CYAN, DIM, RED, Rendering};

/// The crate-name fragments that make a stack frame ours.
///
/// The Ruby took one string (`by='nilavu'`); this workspace ships a binary and
/// eight libraries, and every one of them shares the `afd_` prefix by
/// convention — so two fragments cover the tree and nothing else.
const OURS: [&str; 2] = ["agentsfleetd", "afd_"];

/// The bomb, kept verbatim from the Ruby because it earned its place.
const BOMB: &str = r"       ,--.!,
    __/   -*-
    ,####.  '|`
    ######
    `####'";

/// What the bomb says.
const FLUNKED: &str = "We flunked!";

/// Renders `error` and everything that caused it, for an operator, on the way out.
///
/// Returns the text rather than printing it, so the shape is testable — the
/// printing is one line in [`die`]. That split is the whole reason this is
/// assertable at all: a function that only ever wrote to stderr would need a
/// child process to check.
#[must_use]
pub fn render(error: &dyn Error, rendering: Rendering) -> String {
    render_with_trace(
        error,
        rendering,
        &std::backtrace::Backtrace::capture().to_string(),
    )
}

/// [`render`], with the backtrace handed in rather than captured.
///
/// Split out because `Backtrace::capture()` returns `Disabled` unless
/// `RUST_BACKTRACE` is set, so the frame-filtering half of this renderer is
/// unreachable from a test that calls [`render`] — the branch would be dead
/// code that only ever runs in production, which is the worst place to find
/// out it is wrong.
#[must_use]
pub fn render_with_trace(error: &dyn Error, rendering: Rendering, backtrace: &str) -> String {
    let mut out = String::new();

    out.push_str(&rendering.paint(RED, &format!("✗ {error}")));

    // The causal chain: Ruby had no equivalent, and in Rust this is where the
    // explanation actually lives. `source()` walks from the outermost failure
    // down to the syscall that started it.
    let mut cause = error.source();
    let mut depth = 1_usize;
    while let Some(current) = cause {
        out.push('\n');
        out.push_str(&rendering.paint(
            DIM,
            &format!("{:indent$}caused by: ", "", indent = depth * 2),
        ));
        out.push_str(&rendering.paint(BOLD, &current.to_string()));
        cause = current.source();
        depth += 1;
    }

    let frames = ours_only(backtrace);
    if !frames.is_empty() {
        out.push('\n');
        out.push_str(&rendering.paint(CYAN, &frames.join("\n")));
    }

    out.push('\n');
    out.push_str(&rendering.paint(RED, BOMB));
    out.push_str("  ");
    out.push_str(&rendering.paint(BOLD, FLUNKED));
    out
}

/// Keeps the frames that are ours, as their last path segment.
///
/// Both halves are the Ruby's: `backtrace.grep(/nilavu/)` then
/// `ft.split('/').last`. A frame reading
/// `agentsfleetd/src/preflight.rs:180:5` becomes `preflight.rs:180:5`, which is
/// the part a reader navigates by.
fn ours_only(backtrace: &str) -> Vec<String> {
    backtrace
        .lines()
        .map(str::trim)
        .filter(|line| OURS.iter().any(|ours| line.contains(ours)))
        .filter_map(|line| line.rsplit('/').next())
        .map(str::to_owned)
        .collect()
}

/// Prints a fatal to standard error. The only thing in this crate that does.
pub fn die(error: &dyn Error) {
    // The last thing the process says before it exits, and it has to arrive
    // whether or not a subscriber was ever installed — a boot that fails BEFORE
    // telemetry is wired is precisely when this runs. A `tracing::error!` here
    // would be dropped by the no-subscriber default, turning a diagnosed
    // failure into a silent exit.
    // logging: fatal renderer must reach stderr with no subscriber installed
    eprintln!("{}", render(error, Rendering::of_stderr()));
}
