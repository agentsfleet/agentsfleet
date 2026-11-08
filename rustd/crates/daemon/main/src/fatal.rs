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

/// The crate-name prefixes that make a stack frame ours.
///
/// The Ruby took one string (`by='nilavu'`). The daemon links its own crates
/// (`afd_`) and the shared ones (`af_`), so a frame is ours when one of its
/// words starts with either, or is the binary's name.
const OURS: [&str; 3] = ["agentsfleetd", "afd_", "af_"];
/// Where a frame's path turns into a crate name.
const CRATES_SEGMENT: &str = "/crates/";
/// The layer folders a crate sits in under `crates/` (`rustd/Cargo.toml`).
const LAYERS: [&str; 5] = ["foundation", "protocol", "harness", "runner", "daemon"];
/// The folder a layer's binary sits in, so its frames name no crate.
const BINARY: &str = "main";

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

/// Keeps the frames that are ours, each from its crate name onward.
///
/// `/home/build/rustd/crates/daemon/afd_db/src/pool/mod.rs:12:5` becomes
/// `afd_db/src/pool/mod.rs:12:5`. A layer's binary has no crate name of its
/// own, so `/home/build/rustd/crates/daemon/main/src/preflight.rs:180:5`
/// keeps its layer and becomes `daemon/main/src/preflight.rs:180:5`. The last
/// segment alone used to be enough; with every module root now a `mod.rs`,
/// `mod.rs:12:5` no longer names a file. A line with no path (a function
/// symbol) is kept whole.
fn ours_only(backtrace: &str) -> Vec<String> {
    backtrace
        .lines()
        .map(str::trim)
        .filter(|line| is_ours(line))
        .map(crate_relative)
        .collect()
}

/// Whether a word of `line` names one of our crates, or its path is a
/// layer's binary. Words, not substrings: `leaf_node` is not `af_`.
fn is_ours(line: &str) -> bool {
    let binary = line
        .rsplit_once(CRATES_SEGMENT)
        .and_then(|(_, within)| within.split_once('/'))
        .is_some_and(|(layer, rest)| {
            LAYERS.contains(&layer) && rest.split('/').next() == Some(BINARY)
        });
    binary
        || line
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .any(|word| OURS.iter().any(|ours| word.starts_with(ours)))
}

/// The part of a frame a reader navigates by: from the crate directory on,
/// past its layer folder. A binary keeps its layer, since `main` alone names
/// neither.
fn crate_relative(line: &str) -> String {
    match line.rsplit_once(CRATES_SEGMENT) {
        Some((_, within)) => match within.split_once('/') {
            Some((layer, krate))
                if LAYERS.contains(&layer) && krate.split('/').next() != Some(BINARY) =>
            {
                krate.to_owned()
            }
            _ => within.to_owned(),
        },
        None => line.rsplit('/').next().unwrap_or(line).to_owned(),
    }
}

/// Prints a fatal to standard error. The only thing in this crate that does.
#[expect(
    clippy::print_stderr,
    reason = "the fatal renderer must reach stderr with no subscriber installed"
)]
pub fn die(error: &dyn Error) {
    // The last thing the process says before it exits, and it has to arrive
    // whether or not a subscriber was ever installed — a boot that fails BEFORE
    // telemetry is wired is precisely when this runs. A `tracing::error!` here
    // would be dropped by the no-subscriber default, turning a diagnosed
    // failure into a silent exit.
    eprintln!("{}", render(error, Rendering::of_stderr()));
}
