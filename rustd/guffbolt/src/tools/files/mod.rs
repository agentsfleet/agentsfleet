//! The file tools, over the environment's file calls.
//!
//! `file_write` lives here; `file_read` pages in `read`, `file_edit`
//! replaces texts in `edit`, `apply_patch` sits beside them, `image` shows
//! the model a picture, and `ls`, `find` and `grep` look around. Each tool that changes a file's lines ends its answer with the
//! lines it added and removed (`changes`).
//!
//! Every path is checked here before any call: `..` and an absolute path
//! outside the environment's root are refused with a code, and nothing is
//! read or written. A link that leaves the root is the environment's to
//! refuse, and its refusal reads back under the same code. Every file tool
//! shares the gate, the whole-file read and the failure sorting below.

use std::convert::identity;
use std::path::{Component, Path};

use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool::ToolOutput;
use crate::tools::env::{Env, EnvError};
use crate::tools::process::unavailable;
use crate::tools::{Context, Handler, code};

pub mod apply_patch;
pub mod changes;
pub mod edit;
pub mod image;
pub mod listing;
pub mod read;

pub use self::apply_patch::ApplyPatch;
pub use self::changes::Change;
pub use self::edit::FileEdit;
pub use self::image::ImageTool;
pub use self::listing::{Find, Grep, Ls};
pub use self::read::FileRead;

/// What a path with `..` in it, or an absolute one outside the working
/// directory, reads back after the path.
const LEAVES_WORKING_DIRECTORY: &str = "leaves the working directory";
/// What a file longer than one read carries reads back, between its path
/// and the byte count.
const LONGER_THAN: &str = "is longer than";

/// A handler's answer as a pipeline: the output, or the refusal that ended
/// the pipeline early. Both read back to the model; [`settled`] joins them.
pub(crate) type Answer = Result<ToolOutput, ToolOutput>;

/// The one output an [`Answer`] is.
pub(crate) fn settled(answer: Answer) -> ToolOutput {
    answer.unwrap_or_else(identity)
}

/// `path` relative to `env`'s root: the gate every file tool's path passes.
///
/// A link out is the environment's to catch; this catches a path that
/// leaves by name.
///
/// # Errors
/// The path names nothing below the root, is absolute outside it, or
/// climbs out through `..`.
pub fn inside<'p>(env: &dyn Env, path: &'p str) -> Result<&'p str, ToolOutput> {
    let given = Path::new(path);
    let relative = given.strip_prefix(env.root()).unwrap_or(given);
    if !relative
        .components()
        .any(|part| matches!(part, Component::Normal(_)))
    {
        return Err(ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            "path must name a file in the working directory",
        ));
    }
    let escapes = relative.is_absolute()
        || relative
            .components()
            .any(|part| part == Component::ParentDir);
    if escapes {
        return Err(ToolOutput::failed(
            code::PATH_NOT_ALLOWED,
            &format!("{path} {LEAVES_WORKING_DIRECTORY}"),
        ));
    }
    Ok(relative.to_str().unwrap_or(path))
}

/// What a file call reads back when the environment failed it: the code for
/// a path it refused or a name it has not, its own sentence otherwise.
pub(crate) fn failed(failure: &EnvError) -> ToolOutput {
    if failure.is_path_refused() {
        ToolOutput::failed(code::PATH_NOT_ALLOWED, failure.message())
    } else if failure.is_not_found() {
        ToolOutput::failed(code::FILE_NOT_FOUND, failure.message())
    } else {
        unavailable(failure)
    }
}

/// The refusal for a file at `path` longer than one read of `max_read_bytes`
/// carries: the one wording for a file found that long and for a patch that
/// would make it so.
pub(crate) fn too_long(path: &str, max_read_bytes: u64) -> ToolOutput {
    ToolOutput::failed(
        code::FILE_TOO_LARGE,
        &format!("{path} {LONGER_THAN} {max_read_bytes} bytes"),
    )
}

/// The whole of `path` as text, or the refusal: a file longer than one read
/// carries cannot be edited whole, so it is refused rather than cut, and so
/// is one that is not text, which an edit would write back mangled.
pub(crate) async fn whole(env: &dyn Env, path: &str) -> Result<String, ToolOutput> {
    let most = env.max_read_bytes();
    let fetched = env
        .read_file(path, most)
        .await
        .map_err(|failure| failed(&failure))?;
    if fetched.truncated {
        return Err(too_long(path, most));
    }
    String::from_utf8(fetched.data.to_vec()).map_err(|_bytes| {
        ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            &format!("{path} is not text, so it cannot be edited"),
        )
    })
}

/// `file_write`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// The file, relative to the working directory; missing directories are
    /// made.
    path: String,
    /// The text to write.
    content: String,
}

backed! {
    /// Writes a file, replacing what was there.
    FileWrite over Env
}

impl Handler for FileWrite<'_> {
    const NAME: &'static str = "file_write";
    const DESCRIPTION: &'static str = "Create or replace an entire file, creating missing directories, and return the change. \
        Use file_edit to replace selected text in an existing file, or apply_patch for changes \
        across files.";
    const EXAMPLES: &'static [&'static str] =
        &[r#"{"path":"notes.md","content":"Ready for review.\n"}"#];
    type Arguments = Content;

    async fn run(&self, arguments: Content, _context: Context<'_>) -> ToolOutput {
        settled(write(self.backend, arguments).await)
    }
}

async fn write(env: &dyn Env, arguments: Content) -> Answer {
    let path = inside(env, &arguments.path)?;
    let bytes = arguments.content.len();
    // Read before the write so the answer can count what it replaced; a file
    // that cannot be read whole as text is written uncounted.
    let replaced = changes::before(env, path).await;
    let change = replaced.map(|before| Change::between(path, &before, &arguments.content));
    env.write_file(path, Bytes::from(arguments.content))
        .await
        .map_err(|failure| failed(&failure))?;
    let text = format!("Wrote {bytes} bytes to {path}");
    Ok(ToolOutput::succeeded(changes::report(
        &text,
        change.as_slice(),
    )))
}
