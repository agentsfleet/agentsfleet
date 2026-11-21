//! What the built-in tools hold a run to, chosen by the embedder when it
//! builds them ([`ToolsBuilder::with_limits`](super::ToolsBuilder::with_limits)).
//!
//! Every default is the constant the tool ran with before the embedder could
//! choose: a builder given no limits builds the tools it always built.

use std::time::Duration;

use super::agents::WAIT_MAX;
use super::clock::SLEEP_MAX;
use super::files::image::{IMAGE_MAX_BYTES, IMAGE_MAX_DIMENSION};
use super::files::read::LINES_DEFAULT;
use super::process::exec_session::{YIELD_MAX, YIELD_MIN};
use super::process::oneshot::TIMEOUT_DEFAULT;

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
        default_timeout: TIMEOUT_DEFAULT,
        min_yield: YIELD_MIN,
        max_yield: YIELD_MAX,
        default_read_lines: LINES_DEFAULT,
        max_image_dimension: IMAGE_MAX_DIMENSION,
        max_image_bytes: IMAGE_MAX_BYTES,
        max_sleep: SLEEP_MAX,
        max_wait: WAIT_MAX,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
