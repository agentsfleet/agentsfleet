//! JSON Pointer traversal with fixed writable roots and exact array indices.
//!
//! Two roots are writable: anything below `config`, and `description`, which
//! is a leaf a batch replaces whole. Adding or removing the description would
//! only ever produce a Definition the parser refuses, so the path refuses first.

use serde_json::Value;

use super::{ConfigOp, EditBatch};
use crate::Result;
use crate::definition::invalid;

const CONFIG: &str = "config";
const DESCRIPTION: &str = "description";
const APPEND: &str = "-";
/// The refusal a path outside the two writable roots earns.
const REASON_UNWRITABLE: &str =
    "only config descendants and the description are writable; the description only by replace";
/// The refusal an array index past the end earns, whether replaced or added.
const REASON_ARRAY_BOUNDS: &str = "array edit is outside its bounds";

pub(super) fn apply(document: &mut Value, batch: &garde::Valid<EditBatch>) -> Result<()> {
    for operation in &batch.config_ops {
        let path = match operation {
            ConfigOp::Add { path, .. }
            | ConfigOp::Replace { path, .. }
            | ConfigOp::Remove { path } => path,
        };
        let mut segments = pointer(path, operation)?;
        let member = segments
            .pop()
            .ok_or_else(|| invalid("edit path has no member"))?;
        let parent = parent(document, &segments)?;
        mutate(parent, &member, operation)?;
    }
    Ok(())
}

fn pointer(path: &str, operation: &ConfigOp) -> Result<Vec<String>> {
    let mut segments = Vec::new();
    for part in path
        .strip_prefix('/')
        .ok_or_else(|| invalid("edit path must be a JSON Pointer"))?
        .split('/')
    {
        let mut segment = String::new();
        let mut chars = part.chars();
        while let Some(character) = chars.next() {
            segment.push(match character {
                '~' => match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _invalid => return Err(invalid("invalid JSON Pointer escape")),
                },
                other => other,
            });
        }
        segments.push(segment);
    }
    let writable = match segments.as_slice() {
        [root, _member, ..] => root == CONFIG,
        [root] => root == DESCRIPTION && matches!(operation, ConfigOp::Replace { .. }),
        [] => false,
    };
    if !writable {
        return Err(invalid(REASON_UNWRITABLE));
    }
    Ok(segments)
}

fn parent<'a>(mut value: &'a mut Value, segments: &[String]) -> Result<&'a mut Value> {
    for segment in segments {
        value = match value {
            Value::Object(object) => object.get_mut(segment),
            Value::Array(array) => array.get_mut(index(segment)?),
            _scalar => None,
        }
        .ok_or_else(|| invalid("edit parent does not exist"))?;
    }
    Ok(value)
}

fn index(segment: &str) -> Result<usize> {
    if segment.is_empty()
        || !segment.bytes().all(|byte| byte.is_ascii_digit())
        || (segment.len() > 1 && segment.starts_with('0'))
    {
        return Err(invalid("array index must be a canonical unsigned decimal"));
    }
    segment
        .parse()
        .map_err(|_overflow| invalid("array index exceeds range"))
}

fn mutate(parent: &mut Value, member: &str, operation: &ConfigOp) -> Result<()> {
    match parent {
        Value::Object(object) => match operation {
            ConfigOp::Add { value, .. } => {
                object.insert(member.into(), value.clone());
            }
            ConfigOp::Replace { value, .. } => {
                *object
                    .get_mut(member)
                    .ok_or_else(|| invalid("replacement member is absent"))? = value.clone();
            }
            ConfigOp::Remove { .. } => {
                object
                    .remove(member)
                    .ok_or_else(|| invalid("removed member is absent"))?;
            }
        },
        Value::Array(array) => {
            let at = if member == APPEND && matches!(operation, ConfigOp::Add { .. }) {
                array.len()
            } else {
                index(member)?
            };
            match operation {
                ConfigOp::Add { value, .. } if at <= array.len() => array.insert(at, value.clone()),
                ConfigOp::Replace { value, .. } => {
                    *array
                        .get_mut(at)
                        .ok_or_else(|| invalid(REASON_ARRAY_BOUNDS))? = value.clone();
                }
                ConfigOp::Remove { .. } if at < array.len() => {
                    array.remove(at);
                }
                _out_of_bounds => return Err(invalid(REASON_ARRAY_BOUNDS)),
            }
        }
        _scalar => return Err(invalid("edit parent must be an object or array")),
    }
    Ok(())
}
