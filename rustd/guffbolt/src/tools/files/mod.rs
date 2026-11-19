//! The file tools, over the environment's file calls.
//!
//! `file_write`, `file_append`, `file_delete` and `file_edit` live here;
//! `file_read` pages in `read`, the hashed pair and `apply_patch` beside it,
//! `image` shows the model a picture, and `ls`, `find` and `grep` look
//! around.
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
use crate::tools::env::{Env, EnvError, MAX_READ_BYTES};
use crate::tools::process::{env_of, unavailable};
use crate::tools::{Context, Handler, code};

pub mod apply_patch;
pub mod hashed;
pub mod image;
pub mod listing;
pub mod read;

pub use self::apply_patch::ApplyPatch;
pub use self::hashed::{FileEditHashed, FileReadHashed};
pub use self::image::ImageTool;
pub use self::listing::{Find, Grep, Ls};
pub use self::read::FileRead;

/// What a path with `..` in it, or an absolute one outside the workspace,
/// reads back after the path.
const LEAVES_WORKSPACE: &str = "leaves the workspace";
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
            "path must name a file in the workspace",
        ));
    }
    let escapes = relative.is_absolute()
        || relative
            .components()
            .any(|part| part == Component::ParentDir);
    if escapes {
        return Err(ToolOutput::failed(
            code::PATH_NOT_ALLOWED,
            &format!("{path} {LEAVES_WORKSPACE}"),
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

/// The refusal for a file at `path` longer than one read carries: the one
/// wording for a file found that long and for a patch that would make it so.
pub(crate) fn too_long(path: &str) -> ToolOutput {
    ToolOutput::failed(
        code::FILE_TOO_LARGE,
        &format!("{path} {LONGER_THAN} {MAX_READ_BYTES} bytes"),
    )
}

/// The whole of `path` as text, or the refusal: a file longer than one read
/// carries cannot be edited whole, so it is refused rather than cut, and so
/// is one that is not text, which an edit would write back mangled.
pub(crate) async fn whole(env: &dyn Env, path: &str) -> Result<String, ToolOutput> {
    let fetched = env
        .read_file(path, MAX_READ_BYTES)
        .await
        .map_err(|failure| failed(&failure))?;
    if fetched.truncated {
        return Err(too_long(path));
    }
    String::from_utf8(fetched.data.to_vec()).map_err(|_bytes| {
        ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            &format!("{path} is not text, so it cannot be edited"),
        )
    })
}

/// `file_delete`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Named {
    /// The file, relative to the workspace.
    path: String,
}

/// `file_write`'s and `file_append`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// The file, relative to the workspace; missing directories are made.
    path: String,
    /// The text to write.
    content: String,
}

/// `file_edit`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    /// The file, relative to the workspace.
    path: String,
    /// The text to find; its first occurrence is replaced.
    old_text: String,
    /// What replaces it.
    new_text: String,
}

/// Writes a file, replacing what was there.
#[derive(Debug)]
pub struct FileWrite;

impl Handler for FileWrite {
    const NAME: &'static str = "file_write";
    const DESCRIPTION: &'static str = "Write a file in the workspace, replacing what was \
        there, and make any missing directories on the way.";
    type Arguments = Content;

    async fn run(&self, arguments: Content, context: Context<'_>) -> ToolOutput {
        settled(write(&context, arguments).await)
    }
}

async fn write(context: &Context<'_>, arguments: Content) -> Answer {
    let env = env_of(context)?;
    let path = inside(env, &arguments.path)?;
    let bytes = arguments.content.len();
    env.write_file(path, Bytes::from(arguments.content))
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!(
        "Wrote {bytes} bytes to {path}"
    )))
}

/// Adds to the end of a file.
#[derive(Debug)]
pub struct FileAppend;

impl Handler for FileAppend {
    const NAME: &'static str = "file_append";
    const DESCRIPTION: &'static str = "Add text to the end of a file in the workspace, \
        making the file when it does not exist.";
    type Arguments = Content;

    async fn run(&self, arguments: Content, context: Context<'_>) -> ToolOutput {
        settled(append(&context, arguments).await)
    }
}

async fn append(context: &Context<'_>, arguments: Content) -> Answer {
    let env = env_of(context)?;
    let path = inside(env, &arguments.path)?;
    let bytes = arguments.content.len();
    env.append_file(path, Bytes::from(arguments.content))
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!(
        "Appended {bytes} bytes to {path}"
    )))
}

/// Removes a file.
#[derive(Debug)]
pub struct FileDelete;

impl Handler for FileDelete {
    const NAME: &'static str = "file_delete";
    const DESCRIPTION: &'static str = "Delete a file in the workspace. Directories and \
        links are not deleted.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: Context<'_>) -> ToolOutput {
        settled(delete(&context, &arguments.path).await)
    }
}

async fn delete(context: &Context<'_>, path: &str) -> Answer {
    let env = env_of(context)?;
    let path = inside(env, path)?;
    env.delete_file(path)
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!("Deleted {path}")))
}

/// Replaces text in a file.
#[derive(Debug)]
pub struct FileEdit;

impl Handler for FileEdit {
    const NAME: &'static str = "file_edit";
    const DESCRIPTION: &'static str = "Find text in a file in the workspace and replace its \
        first occurrence. The file must be text and fit one read.";
    type Arguments = Replacement;

    async fn run(&self, arguments: Replacement, context: Context<'_>) -> ToolOutput {
        settled(edit(&context, &arguments).await)
    }
}

async fn edit(context: &Context<'_>, arguments: &Replacement) -> Answer {
    let env = env_of(context)?;
    let path = inside(env, &arguments.path)?;
    if arguments.old_text.is_empty() {
        return Err(ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            "old_text must not be empty",
        ));
    }
    let text = whole(env, path).await?;
    if !text.contains(&arguments.old_text) {
        return Err(ToolOutput::failed(
            code::TEXT_NOT_FOUND,
            &format!("{path} does not contain old_text"),
        ));
    }
    let edited = text.replacen(&arguments.old_text, &arguments.new_text, 1);
    env.write_file(path, Bytes::from(edited))
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!(
        "Replaced {} bytes with {} bytes in {path}",
        arguments.old_text.len(),
        arguments.new_text.len()
    )))
}
