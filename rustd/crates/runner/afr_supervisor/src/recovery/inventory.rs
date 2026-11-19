//! Frozen inventory keeps descriptor-relative access and charges every path and byte.

use super::files::{self, Entry, Identity, Workspace};
use crate::{Result, error};
use cap_std::fs::Dir;
use std::{collections::BTreeMap, os::unix::fs::MetadataExt as _};
use tokio_util::sync::CancellationToken;

const MAX_BYTES: u64 = 24 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;
const MAX_NAMES: usize = 64 * 1024 * 1024;

pub(super) fn scan(
    workspace: &Workspace,
    stop: &CancellationToken,
) -> Result<BTreeMap<String, Entry>> {
    let mut inventory = Inventory {
        workspace,
        stop,
        entries: BTreeMap::new(),
        pending: vec![String::new()],
        total: 0,
        names: 0,
    };
    while let Some(path) = inventory.pending.pop() {
        inventory.directory(&path)?;
    }
    super::links::validate(
        inventory
            .entries
            .iter()
            .filter_map(|(path, entry)| match entry {
                Entry::Symlink { target } => Some((path.as_str(), target.as_str())),
                Entry::File { .. } | Entry::Directory => None,
            }),
    )?;
    Ok(inventory.entries)
}

struct Inventory<'a> {
    workspace: &'a Workspace,
    stop: &'a CancellationToken,
    entries: BTreeMap<String, Entry>,
    pending: Vec<String>,
    total: u64,
    names: usize,
}

impl Inventory<'_> {
    fn directory(&mut self, path: &str) -> Result<()> {
        self.check_stop()?;
        let directory = self.workspace.directory(path)?;
        for entry in directory.entries()? {
            self.check_stop()?;
            let name = entry?
                .file_name()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| error::config("checkpoint path is not UTF-8"))?;
            let relative = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            files::valid_path(&relative)?;
            self.charge_name(relative.len() + 128)?;
            if self.entries.len() >= MAX_ENTRIES {
                return Err(error::capture_limit("checkpoint entry allowance exceeded"));
            }
            let kind = self.entry(&directory, &name, &relative)?;
            if self.entries.insert(relative, kind).is_some() {
                return Err(error::config("checkpoint path repeated"));
            }
        }
        Ok(())
    }

    fn entry(&mut self, directory: &Dir, name: &str, path: &str) -> Result<Entry> {
        let metadata = directory.symlink_metadata(name)?;
        if metadata.is_dir() {
            self.pending.push(path.to_owned());
            Ok(Entry::Directory)
        } else if metadata.is_symlink() {
            let target = directory
                .read_link(name)?
                .into_os_string()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| error::config("checkpoint link is not UTF-8"))?;
            files::valid_link(path, &target)?;
            self.charge_name(target.len())?;
            Ok(Entry::Symlink { target })
        } else if metadata.is_file() {
            let stat = files::regular(directory, name)?.metadata()?;
            self.total = self
                .total
                .checked_add(stat.len())
                .ok_or_else(|| error::capture_limit("checkpoint size overflow"))?;
            if self.total > MAX_BYTES {
                return Err(error::capture_limit(
                    "checkpoint expanded allowance exceeded",
                ));
            }
            Ok(Entry::File {
                identity: Identity::of(&stat),
                executable: stat.mode() & 0o111 != 0,
            })
        } else {
            Err(error::config("checkpoint contains a special file"))
        }
    }

    fn charge_name(&mut self, bytes: usize) -> Result<()> {
        self.names = self
            .names
            .checked_add(bytes)
            .ok_or_else(|| error::capture_limit("checkpoint path budget overflow"))?;
        if self.names > MAX_NAMES {
            return Err(error::capture_limit("checkpoint path budget exceeded"));
        }
        Ok(())
    }

    fn check_stop(&self) -> Result<()> {
        if self.stop.is_cancelled() {
            Err(error::config("checkpoint inventory cancelled"))
        } else {
            Ok(())
        }
    }
}
