//! The confinement itself, run where it can be watched: in a child process.
//!
//! [`afr_sandbox::serve_confined`] confines the process that calls it for
//! good, so it cannot run inside a test harness that has other tests to run,
//! nor inside one that has already started a second thread. This target has
//! no harness: its `main` re-runs itself as a child, the child binds a socket,
//! confines itself and serves, the parent connects and hangs up, and then
//! reads what the child said.
//!
//! The child's coverage is kept. A confined process may write only beneath
//! `/tmp` and the sandbox's own directories, so the child writes its profile
//! to a scratch directory there and the parent moves it beside its own.

use std::ffi::OsString;
use std::fs;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use backon::{BlockingRetryable as _, ConstantBuilder};

/// Set in the child's environment; the child confines itself when it sees it.
const CHILD: &str = "AFR_CONFINE_CHILD";
/// Where an instrumented build writes its coverage.
const PROFILE: &str = "LLVM_PROFILE_FILE";
/// What every coverage profile's file name ends with.
const PROFILE_EXTENSION: &str = "profraw";
/// The child's profile, inside the scratch directory: one file per process.
const CHILD_PROFILE: &str = "confine-%p-%m.profraw";
/// The only place a confined process may write that outlives it.
const SCRATCH_PARENT: &str = "/tmp";
/// Set in the child's environment: the scratch directory it serves in.
const SCRATCH: &str = "AFR_CONFINE_SCRATCH";
/// The socket the child binds, inside the scratch directory.
const SOCKET: &str = "executor.sock";
/// The executor's root, inside the scratch directory.
const ROOT: &str = "root";
/// What the child says once it served a connection to its end.
const SERVED: &str = "served";
/// How often, and how many times, the parent tries the child's socket.
const CONNECT_DELAY: Duration = Duration::from_millis(10);
const CONNECT_TRIES: usize = 500;

fn main() -> ExitCode {
    if std::env::var_os(CHILD).is_some() {
        let scratch = PathBuf::from(std::env::var_os(SCRATCH).unwrap_or_default());
        let said = afr_sandbox::serve_confined(&scratch.join(SOCKET), &scratch.join(ROOT))
            .map_or_else(|refused| refused.to_string(), |()| SERVED.to_owned());
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
        // Bound, confined, then served the parent's connection to its end:
        // every step ran.
        SERVED
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
    fs::create_dir(scratch.path().join(ROOT)).map_err(|error| error.to_string())?;
    let mut child = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
    child
        .env(CHILD, "1")
        .env(SCRATCH, scratch.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if profile.is_some() {
        child.env(PROFILE, scratch.path().join(CHILD_PROFILE));
    }
    let mut running = child.spawn().map_err(|error| error.to_string())?;
    // A child that serves waits for this connection; one that refused has
    // exited, and there is nothing to connect to.
    let socket = scratch.path().join(SOCKET);
    let connect = || -> std::io::Result<Option<UnixStream>> {
        if running.try_wait()?.is_some() {
            return Ok(None);
        }
        UnixStream::connect(&socket).map(Some)
    };
    let connected = connect
        .retry(
            ConstantBuilder::default()
                .with_delay(CONNECT_DELAY)
                .with_max_times(CONNECT_TRIES),
        )
        .sleep(std::thread::sleep)
        .call();
    // Hanging up is what ends the child's session.
    drop(connected);
    let output = running
        .wait_with_output()
        .map_err(|error| error.to_string())?;
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
        // The scratch directory also holds the child's socket and root.
        if path
            .extension()
            .is_some_and(|extension| extension == PROFILE_EXTENSION)
        {
            let name: OsString = path.file_name().map(ToOwned::to_owned).unwrap_or_default();
            fs::copy(&path, collected.join(name))?;
        }
    }
    Ok(())
}
