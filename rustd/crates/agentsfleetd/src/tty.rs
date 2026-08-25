//! Whether this process is writing to a terminal or into a log pipeline.
//!
//! One decision, made once, because the answer changes what everything else in
//! this crate is allowed to emit: box glyphs and ANSI colour belong on a
//! developer's terminal and are corruption in journald. The Ruby original this
//! crate's fatal renderer is modelled on always emitted escapes, which reads
//! beautifully in a console and leaves `\033[1m` littered through a captured
//! log — so the check is here rather than at each call site.

use std::io::IsTerminal as _;

/// Set to any value to suppress colour regardless of the destination.
///
/// The `NO_COLOR` convention, honoured because a daemon is exactly the kind of
/// program someone runs under a wrapper that cannot cope with escapes.
const NO_COLOUR_KNOB: &str = "NO_COLOR";

/// How much decoration the destination can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rendering {
    /// A terminal: colour and box glyphs are safe.
    Rich,
    /// A pipe, a file, or `NO_COLOR`: text only.
    Plain,
}

impl Rendering {
    /// The rule, with both inputs handed in.
    ///
    /// Public and total rather than reading the world itself, because the
    /// alternative is a decision only a pseudo-terminal can exercise. The two
    /// callers below are what touch the process; this is what decides.
    #[must_use]
    pub const fn for_terminal(is_terminal: bool, colour_disabled: bool) -> Self {
        if is_terminal && !colour_disabled {
            Self::Rich
        } else {
            Self::Plain
        }
    }

    /// What standard error can take.
    ///
    /// Standard error rather than standard output because that is where the
    /// fatal path writes, and a daemon commonly has one redirected and not the
    /// other.
    #[must_use]
    pub fn of_stderr() -> Self {
        Self::for_terminal(std::io::stderr().is_terminal(), colour_disabled())
    }

    /// What standard output can take.
    #[must_use]
    pub fn of_stdout() -> Self {
        Self::for_terminal(std::io::stdout().is_terminal(), colour_disabled())
    }

    /// Wraps `text` in `code` when the destination can take it.
    #[must_use]
    pub fn paint(self, code: &str, text: &str) -> String {
        match self {
            Self::Rich => format!("\u{1b}[{code}m{text}\u{1b}[0m"),
            Self::Plain => text.to_owned(),
        }
    }
}

/// Bold red, for the thing that went wrong.
pub const RED: &str = "1;31";

/// Bold cyan, for a stack frame.
pub const CYAN: &str = "36";

/// Bold, for the headline.
pub const BOLD: &str = "1";

/// Dim, for the parts that are context rather than content.
pub const DIM: &str = "2";

/// Green, for the one line that says it worked.
pub const GREEN: &str = "32";

/// Whether the environment asked for no colour at all.
fn colour_disabled() -> bool {
    std::env::var_os(NO_COLOUR_KNOB).is_some()
}
