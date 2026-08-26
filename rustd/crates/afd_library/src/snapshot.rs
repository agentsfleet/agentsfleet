//! Deterministic canonical tar creation from already-validated files.

use std::io;

use bytes::Bytes;

use crate::{ImportBody, Result};

const SKILL_PATH: &str = "SKILL.md";
const TRIGGER_PATH: &str = "TRIGGER.md";
const FILE_MODE: u32 = 0o644;

/// Encodes root documents followed by support files as a deterministic tar.
///
/// Call [`crate::prepare`] first. This function deliberately owns no path
/// validation; [`crate::ImportService`] is the public operation that enforces
/// that ordering.
///
/// # Errors
/// Returns a snapshot error if the standard tar encoder refuses a path or I/O.
pub fn canonical_snapshot(body: &ImportBody) -> Result<Bytes> {
    let mut builder = tar::Builder::new(Vec::new());
    append(&mut builder, SKILL_PATH, &body.skill_markdown)?;
    if let Some(trigger) = &body.trigger_markdown {
        append(&mut builder, TRIGGER_PATH, trigger)?;
    }
    for file in &body.support_files {
        append(&mut builder, &file.path, &file.content)?;
    }
    builder.finish()?;
    Ok(Bytes::from(builder.into_inner()?))
}

fn append(builder: &mut tar::Builder<Vec<u8>>, path: &str, content: &[u8]) -> io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(u64::try_from(content.len()).map_err(io::Error::other)?);
    header.set_mode(FILE_MODE);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();
    builder.append_data(&mut header, path, content)
}
