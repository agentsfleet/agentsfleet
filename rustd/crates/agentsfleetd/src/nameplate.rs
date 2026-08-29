//! The nameplate the daemon prints before it does anything else.
//!
//! Deliberately a STATIC nameplate and not a status display. Its only input is
//! the version string: no roles, no pid, no host, no counts, no timestamps.
//! That is what lets it be the first write of the process — nothing has been
//! resolved yet at the moment it prints, so anything it displayed would either
//! be a lie or would force the print to wait for boot.
//!
//! Runtime status is a different job with a different moment, and it already
//! has one: [`crate::banner`] prints the listening address and the resolved
//! datastore roles AFTER boot succeeds. The two are not variants of each other.
//!
//! # No daemon internals
//!
//! Nothing here reads configuration, opens anything, or calls into the rest of
//! the crate — a nameplate that could fail the boot it announces would be a
//! poor trade. [`Conditions`] is plain data so the decision is a total function
//! over it, and the one function that touches the process environment does
//! nothing else.

use std::io::Write as _;

/// The product name, letter-spaced, hard-coded.
///
/// Spelled out rather than derived from the crate name at runtime: deriving it
/// would put a loop and an allocation on the first line of the process to
/// reproduce a constant, and the derivation is not the interesting part.
const WORDMARK: &str = "A G E N T S F L E E T D";

/// The plain-path name, which is the crate's own spelling.
const PLAIN_NAME: &str = "agentsfleetd";

/// Three spaces, on both decorated lines.
const INDENT: &str = "   ";

/// What sits between the wordmark and the version: three spaces, two middle
/// dots, three spaces.
const SEPARATOR: &str = "   \u{b7}\u{b7}   ";

/// The hairline glyph — U+2504, box drawings light triple dash horizontal.
const HAIRLINE_GLYPH: char = '\u{2504}';

/// The widest hairline drawn, and the width used whenever the terminal's is
/// unknown.
const HAIRLINE_MAX: usize = 54;

/// Room left at the right of the terminal, so the rule never wraps.
const HAIRLINE_MARGIN: usize = 6;

/// Bone, for the wordmark.
const BONE: &str = "\u{1b}[38;2;232;232;227m";
/// Grey, for the separator and the version.
const GREY: &str = "\u{1b}[38;2;107;107;102m";
/// Dim green, for the hairline.
const DIM_GREEN: &str = "\u{1b}[38;2;62;107;24m";
/// Ends every coloured line, so no colour state crosses a line boundary.
const RESET: &str = "\u{1b}[0m";

/// The environment variable that suppresses colour whatever the destination is.
const NO_COLOUR_KNOB: &str = "NO_COLOR";
/// The terminal-type variable. Unset, or `dumb`, means no escapes.
const TERM_KNOB: &str = "TERM";
/// The value of [`TERM_KNOB`] that means "assume nothing".
const TERM_DUMB: &str = "dumb";
/// Set by the service manager for every unit it starts, which is the most
/// reliable "this output is going to a journal" signal there is.
const SERVICE_MANAGER_KNOB: &str = "INVOCATION_ID";

/// How the nameplate should be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// A terminal a person is looking at: 24-bit colour and the hairline.
    Coloured,
    /// A pipe, a file, or a journal: one uncoloured line.
    Plain,
}

/// What the DESTINATION says about itself.
///
/// Handed in rather than sensed here, because the alternative is a decision
/// only a pseudo-terminal and three environment variables can exercise. The
/// same shape `tty::Rendering::for_terminal` uses, widened to the conditions
/// this nameplate answers to.
///
/// `--quiet` is deliberately NOT a field. It is what the operator ASKED for,
/// not something the destination reports, and folding a request in beside three
/// sensed facts made the struct read as one kind of thing when it is two.
#[derive(Debug, Clone, Default)]
pub struct Conditions {
    /// Whether standard output is a terminal.
    pub stdout_is_terminal: bool,
    /// Whether `NO_COLOR` is set to ANY value, an empty one included.
    pub no_colour: bool,
    /// The value of `TERM`, absent when unset.
    pub term: Option<String>,
    /// Whether a service manager started this process.
    pub under_service_manager: bool,
}

impl Conditions {
    /// Reads the process environment. The only function here that does.
    #[must_use]
    pub fn sense() -> Self {
        Self {
            stdout_is_terminal: std::io::IsTerminal::is_terminal(&std::io::stdout()),
            no_colour: std::env::var_os(NO_COLOUR_KNOB).is_some(),
            term: std::env::var(TERM_KNOB).ok(),
            under_service_manager: std::env::var_os(SERVICE_MANAGER_KNOB).is_some(),
        }
    }
}

impl Style {
    /// The rule, as a total function over the destination and the request.
    ///
    /// Five independent reasons to fall back, each sufficient on its own. They
    /// are listed rather than combined into one boolean so that a reader can
    /// see which condition fired and a test can exercise exactly one.
    #[must_use]
    pub fn of(conditions: &Conditions, quiet: bool) -> Self {
        let unsuitable = !conditions.stdout_is_terminal
            || conditions.no_colour
            || conditions.under_service_manager
            || quiet
            || conditions
                .term
                .as_deref()
                .is_none_or(|term| term == TERM_DUMB);

        if unsuitable {
            Self::Plain
        } else {
            Self::Coloured
        }
    }
}

/// How wide the hairline is drawn for a terminal of `columns` width.
///
/// `None` — a width that could not be determined — draws the full
/// [`HAIRLINE_MAX`], which is also the ceiling for any terminal wide enough to
/// take it. Saturating rather than wrapping: a terminal narrower than the
/// margin yields zero, and a hairline of no glyphs is a hairline that does not
/// wrap.
#[must_use]
pub fn hairline_width(columns: Option<usize>) -> usize {
    columns.map_or(HAIRLINE_MAX, |width| {
        width.saturating_sub(HAIRLINE_MARGIN).min(HAIRLINE_MAX)
    })
}

/// Renders the nameplate, blank line before and after, with no trailing write.
///
/// Takes `style` and `columns` rather than sensing them, so both paths are
/// assertable without a pseudo-terminal.
#[must_use]
pub fn render(style: Style, version: &str, columns: Option<usize>) -> String {
    match style {
        Style::Plain => format!("{PLAIN_NAME} {version}\n"),
        Style::Coloured => {
            let hairline: String =
                std::iter::repeat_n(HAIRLINE_GLYPH, hairline_width(columns)).collect();
            // Each line closes its own colour: a reset per line is what stops a
            // truncated write leaving the terminal painted.
            format!(
                "\n{INDENT}{BONE}{WORDMARK}{RESET}{GREY}{SEPARATOR}{version}{RESET}\n\
                 {INDENT}{DIM_GREEN}{hairline}{RESET}\n\n"
            )
        }
    }
}

/// Prints the nameplate. Never fails the caller, whatever standard output does.
///
/// `quiet` is `--quiet`, which reduces the nameplate to one plain line, and
/// `suppressed` is `--no-banner`, which prints nothing at all.
pub fn show(
    version: &str,
    conditions: &Conditions,
    columns: Option<usize>,
    quiet: bool,
    suppressed: bool,
) {
    if suppressed {
        return;
    }
    let rendered = render(Style::of(conditions, quiet), version, columns);

    // One write, because two would let a concurrent writer land between the
    // wordmark and its rule.
    //
    // The result is dropped deliberately and this is the whole of the "banner
    // failure never affects startup" requirement: a closed pipe, a full disk or
    // a detached terminal must not stop the daemon from coming up, and there is
    // no sensible recovery from failing to print a decoration.
    //
    // logging: the nameplate is the process's terminal identity, painted and
    // read once by a person; through the structured logger it would become a
    // record with no consumer, and would be silenced entirely at the one moment
    // it exists for, since no subscriber is installed this early.
    let mut out = std::io::stdout().lock();
    drop(out.write_all(rendered.as_bytes()));
    drop(out.flush());
}

/// The nameplate as it appears above `--help`.
///
/// `clap` prints help and exits inside its own parse, before the caller gets a
/// chance to print anything — so the help path cannot be served by the call in
/// `main`. This is handed to `before_help` instead, which puts the nameplate
/// above the usage text in the same write clap already makes.
///
/// The destination is sensed here rather than passed, because there is no
/// parsed command line to consult: on the help path the flags that would carry
/// `--quiet` are exactly what clap is still in the middle of interpreting. The
/// environment alone decides, which is why `--help --quiet` is still decorated.
#[must_use]
pub fn for_help() -> String {
    // No trailing blank line: clap adds its own separation before the usage
    // block, and a second one leaves a hole in the middle of the help page.
    render(
        Style::of(&Conditions::sense(), false),
        env!("CARGO_PKG_VERSION"),
        None,
    )
    .trim_end()
    .to_owned()
}
