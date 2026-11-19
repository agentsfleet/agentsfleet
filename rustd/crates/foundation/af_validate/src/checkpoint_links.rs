//! Whole-manifest link containment shared by checkpoint publishers and restorers.

use std::collections::{BTreeMap, VecDeque};

/// Follow links in filesystem order after individual paths and targets were validated.
///
/// # Errors
/// Refuses root escapes, cycles and bounded-expansion failures across link chains.
pub fn checkpoint_links<'a>(links: impl Iterator<Item = (&'a str, &'a str)>) -> garde::Result {
    let links: BTreeMap<_, _> = links.collect();
    for (&path, &target) in &links {
        resolve(path, target, &links)?;
    }
    Ok(())
}

fn resolve<'a>(
    path: &'a str,
    target: &'a str,
    links: &BTreeMap<&'a str, &'a str>,
) -> garde::Result {
    let mut resolved: Vec<_> = path
        .rsplit_once('/')
        .map_or(Vec::new(), |(parent, _)| parent.split('/').collect());
    let mut pending: VecDeque<_> = target.split('/').collect();
    let mut followed = 0;
    let mut components = 0;
    while let Some(part) = pending.pop_front() {
        components += 1;
        if components > 8192 {
            return Err(garde::Error::new(
                "checkpoint link resolution exceeds its bound",
            ));
        }
        match part {
            "" | "." => {}
            ".." => {
                if resolved.pop().is_none() {
                    return Err(garde::Error::new(
                        "checkpoint link chain escapes its workspace",
                    ));
                }
            }
            part => {
                resolved.push(part);
                if let Some(target) = links.get(resolved.join("/").as_str()) {
                    followed += 1;
                    if followed > 40 {
                        return Err(garde::Error::new(
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
    Ok(())
}
