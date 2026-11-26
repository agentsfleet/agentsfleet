//! What a sweep removes: every microVM a crashed runner left, at boot, or the
//! one a build of unknown outcome may have left.
//!
//! Each sandbox's directory in the state directory is named by its
//! identifier, and holds everything else of it — its jail, its workspace
//! image, its bound namespace — while its cgroup leaf carries the same name.
//! So one listing finds every leftover, and the removal goes in the order a
//! live sandbox releases: the monitor killed through its cgroup, which drops
//! its drive lock and frees its tap, then the namespace unbound, which takes
//! the veth pair with it, then the directory. The host's egress tables a
//! previous process left are the firewall's own sweep.

#[cfg(any(target_os = "linux", test))]
use af_core::id::Uuid7;
#[cfg(any(target_os = "linux", test))]
use afr_engine::Sweep;

/// The sandboxes a sweep of `scope` removes, from the state directory's
/// `entries`, each a name and whether it is a directory: every directory
/// named by a sandbox identifier for leftovers, the one named for a sandbox,
/// and nothing else the directory holds.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn selected(
    scope: Sweep<'_>,
    entries: impl IntoIterator<Item = (String, bool)>,
) -> Vec<Uuid7> {
    entries
        .into_iter()
        .filter(|(_name, is_dir)| *is_dir)
        .filter_map(|(name, _is_dir)| Uuid7::parse(&name).ok())
        .filter(|id| match scope {
            Sweep::Leftovers => true,
            Sweep::Sandbox(wanted) => id == wanted,
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub(crate) use self::linux::sweep;

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::path::Path;

    use af_core::error_code::{Coded as _, Logged};
    use afr_cgroup::RunCgroup;
    use afr_engine::{Result, Sweep, Swept};

    use super::selected;
    use crate::jail::NAMESPACE_FILE;

    /// The event one swept sandbox is logged under.
    const EVENT_SWEPT: &str = "sandbox_swept";
    /// The event a sandbox the sweep could not remove is logged under.
    const EVENT_SWEEP_FAILED: &str = "sandbox_sweep_failed";

    /// Removes what `scope` names under `state_dir`, each cgroup leaf under
    /// `cgroup_root`, counting what it removed and what it left.
    pub(crate) fn sweep(scope: Sweep<'_>, state_dir: &Path, cgroup_root: &Path) -> Swept {
        let Ok(listed) = fs::read_dir(state_dir) else {
            return Swept::default();
        };
        let entries = listed.flatten().map(|entry| {
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            (entry.file_name().to_string_lossy().into_owned(), is_dir)
        });
        let mut swept = Swept::default();
        for id in selected(scope, entries) {
            let sandbox_id = id.as_str();
            let engine = "firecracker";
            match remove(&state_dir.join(sandbox_id), &cgroup_root.join(sandbox_id)) {
                Ok(()) => {
                    swept.destroyed += 1;
                    let event = EVENT_SWEPT;
                    tracing::info!(engine, sandbox_id, event);
                }
                Err(error) => {
                    swept.left += 1;
                    let Logged { error_code, reason } = error.logged();
                    let event = EVENT_SWEEP_FAILED;
                    tracing::warn!(engine, sandbox_id, error_code, reason, event);
                }
            }
        }
        swept
    }

    /// Kills the monitor through its cgroup, unbinds its namespace, then
    /// removes its directory, in the order a live sandbox releases them.
    fn remove(dir: &Path, cgroup: &Path) -> Result<()> {
        if cgroup.is_dir() {
            RunCgroup::leftover(cgroup.to_owned()).remove()?;
        }
        afr_firewall::remove_namespace(&dir.join(NAMESPACE_FILE))?;
        fs::remove_dir_all(dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use af_core::id::Uuid7;
    use afr_engine::Sweep;

    use super::selected;

    const LEFT: &str = "0198f0c2-7a3e-7c1d-9b2e-000000000001";
    const ALSO_LEFT: &str = "0198f0c2-7a3e-7c1d-9b2e-000000000002";

    fn listing() -> Vec<(String, bool)> {
        vec![
            (LEFT.to_owned(), true),
            (ALSO_LEFT.to_owned(), true),
            ("0198f0c2-7a3e-7c1d-9b2e-000000000003".to_owned(), false),
            ("lost+found".to_owned(), true),
            ("not-a-sandbox".to_owned(), true),
        ]
    }

    #[test]
    fn test_a_sweep_selects_only_directories_named_by_a_sandbox() {
        let left = Uuid7::parse(LEFT).unwrap();
        let also_left = Uuid7::parse(ALSO_LEFT).unwrap();

        let leftovers = selected(Sweep::Leftovers, listing());
        let one = selected(Sweep::Sandbox(&also_left), listing());
        let absent = selected(
            Sweep::Sandbox(&Uuid7::parse("0198f0c2-7a3e-7c1d-9b2e-000000000009").unwrap()),
            listing(),
        );

        assert_eq!(leftovers, [left, also_left.clone()]);
        assert_eq!(one, [also_left]);
        assert!(
            absent.is_empty(),
            "a sandbox with no directory has nothing to sweep"
        );
    }
}
