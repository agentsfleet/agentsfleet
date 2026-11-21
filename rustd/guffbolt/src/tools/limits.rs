//! What the built-in tools hold a run to, chosen by the embedder when it
//! builds them ([`ToolsBuilder::with_limits`](super::ToolsBuilder::with_limits)).
//!
//! Every default is the value the tool ran with before the embedder could
//! choose: a builder given no limits builds the tools it always built. Each
//! value lives here and nowhere else; a tool reads the limit it was built
//! with, and so does what the model is told about it.

use std::time::Duration;

/// The bounds the built-in tools apply to a run's calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How long `shell` and `git` run a command when the model names no
    /// timeout.
    pub default_timeout: Duration,
    /// The shortest `exec_command` and `write_stdin` wait for output.
    pub min_yield: Duration,
    /// The longest `exec_command` and `write_stdin` wait for output; at
    /// least `min_yield`.
    pub max_yield: Duration,
    /// Lines `file_read` returns when the model names no limit.
    pub default_read_lines: usize,
    /// The longer side `image` scales a larger picture down to, in pixels.
    pub max_image_dimension: u32,
    /// The largest picture `image` shows the model, after scaling, in bytes.
    pub max_image_bytes: u64,
    /// The longest `sleep` waits.
    pub max_sleep: Duration,
    /// The longest `wait_agent` waits for a child.
    pub max_wait: Duration,
}

impl Limits {
    /// The bounds the tools held before an embedder could choose them.
    pub const DEFAULT: Self = Self {
        default_timeout: Duration::from_secs(10),
        min_yield: Duration::from_millis(250),
        max_yield: Duration::from_secs(30),
        default_read_lines: 2000,
        // The longest side the image wires take without scaling it again.
        max_image_dimension: 2048,
        // The Messages wire's cap per image, the stricter of the two wires
        // that take one.
        max_image_bytes: 5 * 1024 * 1024,
        max_sleep: Duration::from_hours(12),
        // Zero is allowed, to only look.
        max_wait: Duration::from_hours(1),
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// `span` as the model reads it: its milliseconds, and the largest whole
/// hours, minutes or seconds it is, when it is one.
pub(crate) fn spoken(span: Duration) -> String {
    let ms = super::process::saturating_millis(span);
    [(3_600_000, "hour"), (60_000, "minute"), (1_000, "second")]
        .into_iter()
        .find(|&(unit, _)| ms >= unit && ms.is_multiple_of(unit))
        .map_or_else(
            || format!("{ms} milliseconds"),
            |(unit, name)| {
                let count = ms / unit;
                let plural = if count == 1 { "" } else { "s" };
                format!("{ms} milliseconds ({count} {name}{plural})")
            },
        )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::spoken;

    #[test]
    fn a_span_is_read_in_milliseconds_and_its_largest_whole_unit() {
        assert_eq!(
            spoken(Duration::from_hours(12)),
            "43200000 milliseconds (12 hours)"
        );
        assert_eq!(
            spoken(Duration::from_secs(90)),
            "90000 milliseconds (90 seconds)"
        );
        assert_eq!(
            spoken(Duration::from_mins(1)),
            "60000 milliseconds (1 minute)"
        );
        assert_eq!(spoken(Duration::from_millis(250)), "250 milliseconds");
    }
}
