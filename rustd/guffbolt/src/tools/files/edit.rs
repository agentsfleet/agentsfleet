//! `file_edit`: replaces texts in one file, each named by text the file holds
//! exactly once.
//!
//! Every edit is matched against the file as it was read, never against an
//! earlier edit's result, so what the model quoted is what is replaced. An
//! edit whose text the file lacks or holds more than once, or that overlaps
//! another, refuses the whole call: the file is written only once every edit
//! has its one place.

use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;

use super::changes::{self, Change};
use super::{Answer, failed, inside, settled, whole};
use crate::tool::ToolOutput;
use crate::tools::env::Env;
use crate::tools::{Context, Handler, code};

/// `file_edit`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edits {
    /// The file, relative to the working directory.
    path: String,
    /// The replacements, at least one. Each is matched against the file as
    /// it was before this call, and no two may overlap.
    edits: Vec<Edit>,
}

/// One replacement in a `file_edit` call.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    /// The text to replace, exactly as the file holds it; the file must hold
    /// it once.
    old_text: String,
    /// What replaces it.
    new_text: String,
}

backed! {
    /// Replaces texts in a file.
    FileEdit over Env
}

impl Handler for FileEdit<'_> {
    const NAME: &'static str = "file_edit";
    const DESCRIPTION: &'static str = "Replace exact text in an existing text file and return the changes. Each old_text must \
        match the original once, and no edits may overlap. All replacements must apply before \
        anything is written. Use file_write to create or replace the whole file, and apply_patch \
        for changes across files.";
    const EXAMPLES: &'static [&'static str] =
        &[r#"{"path":"notes.md","edits":[{"old_text":"draft","new_text":"ready"}]}"#];
    type Arguments = Edits;

    async fn run(&self, arguments: Edits, _context: Context<'_>) -> ToolOutput {
        settled(edit(self.backend, &arguments).await)
    }
}

async fn edit(env: &dyn Env, arguments: &Edits) -> Answer {
    let path = inside(env, &arguments.path)?;
    if arguments.edits.is_empty() {
        return Err(ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            "edits must hold at least one edit",
        ));
    }
    let blank = arguments
        .edits
        .iter()
        .position(|one| one.old_text.is_empty());
    if let Some(index) = blank {
        return Err(ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            &format!("edits[{index}].old_text must not be empty"),
        ));
    }
    let text = whole(env, path).await?;
    let placed = placed(path, &text, &arguments.edits)?;
    let edited = spliced(&text, &placed);
    let change = Change::between(path, &text, &edited);
    env.write_file(path, Bytes::from(edited))
        .await
        .map_err(|failure| failed(&failure))?;
    let count = placed.len();
    let noun = if count == 1 { "edit" } else { "edits" };
    let text = format!("Applied {count} {noun} to {path}");
    Ok(ToolOutput::succeeded(changes::report(&text, &[change])))
}

/// One edit's place in the file: the bytes its text covers, what replaces
/// them, and the edit's index, for a refusal to name.
#[derive(Debug)]
struct Placed<'e> {
    index: usize,
    start: usize,
    end: usize,
    new_text: &'e str,
}

/// Every edit's one place in `text`, the file at `path`, in file order; or
/// the refusal for the first edit with no place or more than one, or for the
/// first two that overlap.
fn placed<'e>(path: &str, text: &str, edits: &'e [Edit]) -> Result<Vec<Placed<'e>>, ToolOutput> {
    let mut placed = Vec::with_capacity(edits.len());
    for (index, one) in edits.iter().enumerate() {
        let mut found = starts(text, &one.old_text);
        let Some(start) = found.next() else {
            return Err(ToolOutput::failed(
                code::TEXT_NOT_FOUND,
                &format!("{path} does not contain edits[{index}].old_text"),
            ));
        };
        let others = found.count();
        if others > 0 {
            let times = others + 1;
            return Err(ToolOutput::failed(
                code::TEXT_NOT_UNIQUE,
                &format!(
                    "edits[{index}].old_text occurs {times} times in {path}; quote more of the \
                     text around it so it occurs once"
                ),
            ));
        }
        placed.push(Placed {
            index,
            start,
            end: start + one.old_text.len(),
            new_text: &one.new_text,
        });
    }
    placed.sort_by_key(|place| place.start);
    let overlap = placed.windows(2).find_map(|pair| match pair {
        [before, after] if before.end > after.start => Some((before.index, after.index)),
        _apart => None,
    });
    if let Some((first, second)) = overlap {
        let (first, second) = (first.min(second), first.max(second));
        return Err(ToolOutput::failed(
            code::INVALID_ARGUMENTS,
            &format!(
                "edits[{first}] and edits[{second}] overlap in {path}; merge them into one edit"
            ),
        ));
    }
    Ok(placed)
}

/// Where `needle`, which is not empty, starts in `text`, overlapping places
/// too: `aa` starts twice in `aaa`, so text that repeats into itself is not
/// taken as held once.
fn starts<'t>(text: &'t str, needle: &'t str) -> impl Iterator<Item = usize> + 't {
    let mut from = 0;
    std::iter::from_fn(move || {
        let start = from + text.get(from..)?.find(needle)?;
        from = start + text.get(start..)?.chars().next()?.len_utf8();
        Some(start)
    })
}

/// `text` with each of `placed`, in file order and apart, replaced.
fn spliced(text: &str, placed: &[Placed<'_>]) -> String {
    let mut edited = String::with_capacity(text.len());
    let mut at = 0;
    for place in placed {
        edited.push_str(text.get(at..place.start).unwrap_or_default());
        edited.push_str(place.new_text);
        at = place.end;
    }
    edited.push_str(text.get(at..).unwrap_or_default());
    edited
}
