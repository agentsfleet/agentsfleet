//! The plain file tools, over the executor's file calls: `file_write`,
//! `file_append`, `file_delete` and `file_edit`; `file_read` pages in
//! `read.rs`.
//!
//! Every path is checked here before any call: `..` and an absolute path
//! outside `/workspace` are refused with a code, and nothing is read or
//! written. A link that leaves the workspace is the executor's to refuse,
//! through the `cap_std` handle it opens everything under, and its refusal
//! reads back under the same code. The hashed tools and `apply_patch` share
//! the gate, the whole-file read and the failure sorting below.

use std::convert::identity;
use std::path::{Component, Path};

use afr_executor::{Executor, MAX_READ_BYTES, WORKSPACE_ROOT};
use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;

use super::{executor_of, unavailable};
use crate::catalog::{Entry, FILE_APPEND, FILE_DELETE, FILE_EDIT, FILE_WRITE};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// What a path with `..` in it, or an absolute one outside the workspace,
/// reads back after the path.
const LEAVES_WORKSPACE: &str = "leaves the workspace";
/// What a file longer than one read carries reads back, between its path
/// and the byte count.
const LONGER_THAN: &str = "is longer than";

/// A handler's answer as a pipeline: the output, or the refusal that ended
/// the pipeline early. Both read back to the model; [`settled`] joins them.
pub(super) type Answer = Result<ToolOutput, ToolOutput>;

/// The one output an [`Answer`] is.
pub(super) fn settled(answer: Answer) -> ToolOutput {
    answer.unwrap_or_else(identity)
}

/// `path` relative to the workspace, or the refusal: it must name something
/// below the workspace root, an absolute path must be under `/workspace`, and
/// no path may climb out by name. A link out is the executor's to catch,
/// through the one handle it opens everything under.
pub(super) fn inside(path: &str) -> Result<&str, ToolOutput> {
    let given = Path::new(path);
    let relative = given.strip_prefix(WORKSPACE_ROOT).unwrap_or(given);
    if !relative
        .components()
        .any(|part| matches!(part, Component::Normal(_)))
    {
        return Err(ToolOutput::failed(
            ToolErrorCode::InvalidArguments,
            "path must name a file in the workspace",
        ));
    }
    let escapes = relative.is_absolute()
        || relative
            .components()
            .any(|part| part == Component::ParentDir);
    if escapes {
        return Err(ToolOutput::failed(
            ToolErrorCode::PathNotAllowed,
            &format!("{path} {LEAVES_WORKSPACE}"),
        ));
    }
    Ok(relative.to_str().unwrap_or(path))
}

/// What a file call reads back when the executor failed it: the code for a
/// path it refused or a name it has not, the executor's own sentence otherwise.
pub(super) fn failed(failure: &afr_executor::Error) -> ToolOutput {
    if failure.is_path_refused() {
        ToolOutput::failed(ToolErrorCode::PathNotAllowed, &failure.wire_message())
    } else if failure.is_not_found() {
        ToolOutput::failed(ToolErrorCode::FileNotFound, &failure.wire_message())
    } else {
        unavailable(failure)
    }
}

/// The whole of `path` as text, or the refusal: a file longer than one read
/// carries cannot be edited whole, so it is refused rather than cut, and so
/// is one that is not text, which an edit would write back mangled.
pub(super) async fn whole(executor: &dyn Executor, path: &str) -> Result<String, ToolOutput> {
    let fetched = executor
        .read_file(path, MAX_READ_BYTES)
        .await
        .map_err(|failure| failed(&failure))?;
    if fetched.truncated {
        return Err(ToolOutput::failed(
            ToolErrorCode::FileTooLarge,
            &format!("{path} {LONGER_THAN} {MAX_READ_BYTES} bytes"),
        ));
    }
    String::from_utf8(fetched.data.to_vec()).map_err(|_bytes| {
        ToolOutput::failed(
            ToolErrorCode::InvalidArguments,
            &format!("{path} is not text, so it cannot be edited"),
        )
    })
}

/// `file_delete`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Named {
    /// The file, relative to the workspace.
    path: String,
}

/// `file_write`'s and `file_append`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Content {
    /// The file, relative to the workspace; missing directories are made.
    path: String,
    /// The text to write.
    content: String,
}

/// `file_edit`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Replacement {
    /// The file, relative to the workspace.
    path: String,
    /// The text to find; its first occurrence is replaced.
    old_text: String,
    /// What replaces it.
    new_text: String,
}

/// Writes a file, replacing what was there.
#[derive(Debug)]
pub(crate) struct FileWrite;

#[async_trait::async_trait]
impl Handler for FileWrite {
    const ENTRY: &'static Entry = &FILE_WRITE;
    const DESCRIPTION: &'static str = "Write a file in the workspace, replacing what was \
        there, and make any missing directories on the way.";
    type Arguments = Content;

    async fn run(&self, arguments: Content, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(write(&context, arguments).await)
    }
}

async fn write(context: &ToolContext<'_, '_>, arguments: Content) -> Answer {
    let executor = executor_of(context)?;
    let path = inside(&arguments.path)?;
    let bytes = arguments.content.len();
    executor
        .write_file(path, Bytes::from(arguments.content))
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!(
        "Wrote {bytes} bytes to {path}"
    )))
}

/// Adds to the end of a file.
#[derive(Debug)]
pub(crate) struct FileAppend;

#[async_trait::async_trait]
impl Handler for FileAppend {
    const ENTRY: &'static Entry = &FILE_APPEND;
    const DESCRIPTION: &'static str = "Add text to the end of a file in the workspace, \
        making the file when it does not exist.";
    type Arguments = Content;

    async fn run(&self, arguments: Content, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(append(&context, arguments).await)
    }
}

async fn append(context: &ToolContext<'_, '_>, arguments: Content) -> Answer {
    let executor = executor_of(context)?;
    let path = inside(&arguments.path)?;
    let bytes = arguments.content.len();
    executor
        .append_file(path, Bytes::from(arguments.content))
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!(
        "Appended {bytes} bytes to {path}"
    )))
}

/// Removes a file.
#[derive(Debug)]
pub(crate) struct FileDelete;

#[async_trait::async_trait]
impl Handler for FileDelete {
    const ENTRY: &'static Entry = &FILE_DELETE;
    const DESCRIPTION: &'static str = "Delete a file in the workspace. Directories and \
        links are not deleted.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(delete(&context, &arguments.path).await)
    }
}

async fn delete(context: &ToolContext<'_, '_>, path: &str) -> Answer {
    let executor = executor_of(context)?;
    let path = inside(path)?;
    executor
        .delete_file(path)
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!("Deleted {path}")))
}

/// Replaces text in a file.
#[derive(Debug)]
pub(crate) struct FileEdit;

#[async_trait::async_trait]
impl Handler for FileEdit {
    const ENTRY: &'static Entry = &FILE_EDIT;
    const DESCRIPTION: &'static str = "Find text in a file in the workspace and replace its \
        first occurrence. The file must be text and fit one read.";
    type Arguments = Replacement;

    async fn run(&self, arguments: Replacement, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(edit(&context, &arguments).await)
    }
}

async fn edit(context: &ToolContext<'_, '_>, arguments: &Replacement) -> Answer {
    let executor = executor_of(context)?;
    let path = inside(&arguments.path)?;
    if arguments.old_text.is_empty() {
        return Err(ToolOutput::failed(
            ToolErrorCode::InvalidArguments,
            "old_text must not be empty",
        ));
    }
    let text = whole(executor, path).await?;
    if !text.contains(&arguments.old_text) {
        return Err(ToolOutput::failed(
            ToolErrorCode::TextNotFound,
            &format!("{path} does not contain old_text"),
        ));
    }
    let edited = text.replacen(&arguments.old_text, &arguments.new_text, 1);
    executor
        .write_file(path, Bytes::from(edited))
        .await
        .map_err(|failure| failed(&failure))?;
    Ok(ToolOutput::succeeded(format!(
        "Replaced {} bytes with {} bytes in {path}",
        arguments.old_text.len(),
        arguments.new_text.len()
    )))
}

#[cfg(test)]
#[path = "files/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "files/gate_tests.rs"]
mod gate_tests;

#[cfg(test)]
#[path = "files/limits_tests.rs"]
mod limits_tests;
