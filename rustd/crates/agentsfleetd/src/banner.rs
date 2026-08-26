//! The line the daemon prints when it comes up.
//!
//! Small on purpose. A banner is worth exactly one thing — telling whoever is
//! watching that THIS build, with THESE roles resolved, is the process now
//! holding the port — and every line past that is noise in a log someone has
//! to scroll. So: the mark, the version, the roles, the pid.
//!
//! The glyphs appear only on a terminal. Under systemd or Docker the same
//! information arrives as one plain line, because box-drawing characters in
//! journald are a thing to grep past rather than a thing to read.

use crate::tty::{BOLD, DIM, GREEN, Rendering};

/// The wordmark, drawn only when something can render it.
///
/// Two rows of half-block glyphs spelling `AF` — small enough to sit beside the
/// text rather than above it, which is what keeps this one line instead of six.
const MARK: [&str; 2] = ["▄▀█ █▀▀", "█▀█ █▀░"];

/// The product name, spelled the way `AGENTS.md` requires.
const NAME: &str = "agentsfleetd";

/// Renders the startup banner for a given destination.
///
/// Takes `rendering` rather than deciding, so the plain form is assertable
/// without a pseudo-terminal.
#[must_use]
pub fn render(rendering: Rendering, version: &str, roles: &[String], pid: u32) -> String {
    let summary = format!("{} · pid {pid}", roles.join(" · "));

    match rendering {
        Rendering::Plain => format!("{NAME} {version} ready — {summary}"),
        Rendering::Rich => {
            let head = format!(
                "  {}  {} {}",
                rendering.paint(GREEN, MARK[0]),
                rendering.paint(BOLD, NAME),
                rendering.paint(DIM, version),
            );
            let foot = format!(
                "  {}  {}",
                rendering.paint(GREEN, MARK[1]),
                rendering.paint(DIM, &summary),
            );
            format!("{head}\n{foot}")
        }
    }
}

/// Prints the startup banner to standard output.
pub fn show(version: &str, roles: &[String], pid: u32) {
    // The banner is terminal output for a human starting the daemon, not a log
    // event — it is painted, aligned, and read once at boot. Routing it through
    // `tracing` would make it a structured record with no consumer and would
    // silence it entirely whenever no subscriber is installed yet, which is
    // exactly the moment it exists to serve.
    // logging: startup banner is human-facing stdout, not a log record
    println!("{}", render(Rendering::of_stdout(), version, roles, pid));
}
