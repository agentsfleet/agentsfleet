//! The confinement itself, run where it can be watched: in a child process.
//!
//! [`afr_sandbox::serve_sandboxed`] confines the process that calls it for
//! good, so it cannot run inside a test harness that has other tests to run,
//! nor inside one that has already started a second thread. This target has
//! no harness: its `main` re-runs itself as a child, the child confines itself
//! and tries to serve, and the parent reads what the child said.
//!
//! The child's coverage is kept. A confined process may write only beneath
//! `/tmp` and the sandbox's own directories, so the child writes its profile
//! to a scratch directory there and the parent moves it beside its own.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Set in the child's environment; the child confines itself when it sees it.
const CHILD: &str = "AFR_CONFINE_CHILD";
/// Where an instrumented build writes its coverage.
const PROFILE: &str = "LLVM_PROFILE_FILE";
/// The child's profile, inside the scratch directory: one file per process.
const CHILD_PROFILE: &str = "confine-%p-%m.profraw";
/// The only place a confined process may write that outlives it.
const SCRATCH_PARENT: &str = "/tmp";

fn main() -> ExitCode {
    if std::env::var_os(CHILD).is_some() {
        let said = afr_sandbox::serve_sandboxed()
            .err()
            .map(|refused| refused.to_string())
            .unwrap_or_default();
        // logging: stdout is this child's answer to the parent that started it
        println!("{said}");
        return ExitCode::SUCCESS;
    }
    match parent() {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            // logging: a harness-less test reports its failure on stderr
            eprintln!("confine: {failure}");
            ExitCode::FAILURE
        }
    }
}

/// What the child must say, given what this host is and who runs the test.
fn expected() -> &'static str {
    if !cfg!(target_os = "linux") {
        // Where Landlock does not exist, nothing is ever served.
        "landlock is unavailable"
    } else if !afr_sandbox::probe(&afr_sandbox::ProbePaths::default()).landlock {
        "Landlock refused"
    } else if is_root() {
        // Root keeps capabilities through every other step, so it is refused
        // at the last one rather than served.
        "a capability survived"
    } else {
        // Confined, then the executor finds no sandbox socket directory:
        // every step of the confinement ran.
        "the sandbox's executor failed"
    }
}

#[cfg(target_os = "linux")]
fn is_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|ids| ids.split_whitespace().nth(1))
        == Some("0")
}

#[cfg(not(target_os = "linux"))]
fn is_root() -> bool {
    false
}

fn parent() -> Result<(), String> {
    let scratch = tempfile::Builder::new()
        .prefix("afr-confine")
        .tempdir_in(SCRATCH_PARENT)
        .map_err(|error| error.to_string())?;
    let profile = std::env::var_os(PROFILE).map(PathBuf::from);
    let mut child = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
    child.env(CHILD, "1");
    if profile.is_some() {
        child.env(PROFILE, scratch.path().join(CHILD_PROFILE));
    }
    let output = child.output().map_err(|error| error.to_string())?;
    if let Some(dir) = profile.as_deref().and_then(Path::parent) {
        keep_profiles(scratch.path(), dir).map_err(|error| error.to_string())?;
    }
    let said = String::from_utf8_lossy(&output.stdout);
    let wanted = expected();
    if output.status.success() && said.contains(wanted) {
        Ok(())
    } else {
        Err(format!(
            "the child said {said:?} ({}), wanted {wanted:?}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Moves the child's profiles from `scratch` to where this run's coverage is
/// collected. Copied, not renamed: `/tmp` is often another file system.
fn keep_profiles(scratch: &Path, collected: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(scratch)? {
        let path = entry?.path();
        let name: OsString = path.file_name().map(ToOwned::to_owned).unwrap_or_default();
        fs::copy(&path, collected.join(name))?;
    }
    Ok(())
}
