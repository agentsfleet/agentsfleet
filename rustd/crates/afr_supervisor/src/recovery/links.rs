//! Resolve the complete link graph before capture or restore accepts it.

use crate::{Result, error};
use std::collections::{BTreeMap, VecDeque};

/// Lexical containment of each individual target is insufficient: a link to
/// `.` can shorten another link's path before a later `..` escapes the root.
/// Resolve components in filesystem order, with the same complete link map.
pub(super) fn validate<'a>(links: impl Iterator<Item = (&'a str, &'a str)>) -> Result<()> {
    let links: BTreeMap<_, _> = links.collect();
    for (&path, &target) in &links {
        super::files::valid_link(path, target)?;
        let mut resolved: Vec<_> = path
            .rsplit_once('/')
            .map_or(Vec::new(), |(parent, _)| parent.split('/').collect());
        let mut pending: VecDeque<_> = target.split('/').collect();
        let mut followed = 0;
        let mut components = 0;
        while let Some(part) = pending.pop_front() {
            components += 1;
            if components > 8192 {
                return Err(error::config(
                    "checkpoint link resolution exceeds its bound",
                ));
            }
            match part {
                "" | "." => {}
                ".." => {
                    if resolved.pop().is_none() {
                        return Err(error::config("checkpoint link chain escapes its workspace"));
                    }
                }
                part => {
                    resolved.push(part);
                    if let Some(target) = links.get(resolved.join("/").as_str()) {
                        followed += 1;
                        if followed > 40 {
                            return Err(error::config(
                                "checkpoint link chain is cyclic or too deep",
                            ));
                        }
                        resolved.pop();
                        let mut next: VecDeque<_> = target.split('/').collect();
                        next.append(&mut pending);
                        pending = next;
                    }
                }
            }
        }
    }
    Ok(())
}
