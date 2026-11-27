//! The confinement itself, run where it can be watched: in a child process.
//!
//! [`afr_guest::serve_confined`] confines the process that calls it for
//! good, so it cannot run inside a test harness that has other tests to run,
//! nor inside one that has already started a second thread. This target has
//! no harness: its `main` re-runs itself as a child, the child binds a socket,
//! confines itself and serves, the parent connects and hangs up, and then
//! reads what the child said. Confined, the child then asks the kernel for a
//! namespace each way a tenant could, and starts a thread: the parent reads
//! back what each call answered.
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
/// What the child's line of answers starts with.
#[cfg(target_os = "linux")]
const ANSWERS: &str = "namespaces:";
/// How often, and how many times, the parent tries the child's socket.
const CONNECT_DELAY: Duration = Duration::from_millis(10);
const CONNECT_TRIES: usize = 500;

#[expect(
    clippy::disallowed_methods,
    reason = "the scratch directory is a path the parent set as raw OS bytes; af_core::env reads String values and would lose a non-Unicode path"
)]
#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "stdout is the child's answer to the parent that started it, and a harness-less test reports its failure on stderr"
)]
fn main() -> ExitCode {
    if std::env::var_os(CHILD).is_some() {
        let scratch = PathBuf::from(std::env::var_os(SCRATCH).unwrap_or_default());
        let said = afr_guest::serve_confined(&scratch.join(SOCKET), &scratch.join(ROOT))
            .map_or_else(|refused| refused.to_string(), |()| SERVED.to_owned());
        println!("{said}");
        #[cfg(target_os = "linux")]
        println!("{ANSWERS} {}", asked::answers());
        return ExitCode::SUCCESS;
    }
    match parent() {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
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
    } else if !(afr_guest::landlock_enforceable()
        && afr_guest::landlock_listed(Path::new(afr_guest::LSM_PATH)))
    {
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
const fn is_root() -> bool {
    false
}

fn parent() -> Result<(), String> {
    let scratch = tempfile::Builder::new()
        .prefix("afr-confine")
        .tempdir_in(SCRATCH_PARENT)
        .map_err(|error| error.to_string())?;
    #[expect(
        clippy::disallowed_methods,
        reason = "LLVM_PROFILE_FILE is a path read as raw OS bytes; af_core::env reads String values and would lose a non-Unicode path"
    )]
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
    if output.status.success() && said.contains(wanted) && said.contains(&answers(wanted)) {
        Ok(())
    } else {
        Err(format!(
            "the child said {said:?} ({}), wanted {wanted:?}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// What the child's calls must have answered, given what it said: once the
/// seccomp programs are installed, which both a served child and one refused
/// for a surviving capability got to, a `clone` into a new user namespace and
/// a `setns` are refused, `clone3` is unknown, and a thread still starts.
/// A child refused before then has no program to answer for.
#[cfg(target_os = "linux")]
fn answers(said: &str) -> String {
    if said == "Landlock refused" {
        return ANSWERS.to_owned();
    }
    let (refused, unknown) = (libc::EPERM, libc::ENOSYS);
    format!("{ANSWERS} clone {refused} setns {refused} clone3 {unknown} thread started")
}

/// Nothing is confined where there is no Landlock, so nothing answers.
#[cfg(not(target_os = "linux"))]
const fn answers(_said: &str) -> String {
    String::new()
}

/// The calls a confined child makes, and what each answered.
#[cfg(target_os = "linux")]
mod asked {
    /// The error `call` set, or 0 when it succeeded.
    fn errno(answered: libc::c_long) -> i32 {
        if answered == -1 {
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default()
        } else {
            0
        }
    }

    /// A `clone` into a new user namespace, a `setns`, a `clone3` and a
    /// thread, as one line.
    pub(super) fn answers() -> String {
        let flags = libc::CLONE_NEWUSER | libc::SIGCHLD;
        // SAFETY: a `clone` without a stack is a fork; the child it makes
        // when nothing refuses it ends at once below, touching nothing.
        let forked = unsafe { libc::syscall(libc::SYS_clone, flags, 0, 0, 0, 0) };
        if forked == 0 {
            // SAFETY: `_exit` ends the forked child at once.
            unsafe { libc::_exit(0) };
        }
        let clone = errno(forked);
        // SAFETY: descriptor -1 is none, so the call changes nothing.
        let setns = errno(unsafe { libc::syscall(libc::SYS_setns, -1, 0) });
        // SAFETY: no arguments and a zero size, which the kernel refuses
        // before it reads anything.
        let newer = errno(unsafe { libc::syscall(libc::SYS_clone3, 0, 0) });
        let thread = std::thread::Builder::new()
            .spawn(|| ())
            .map(std::thread::JoinHandle::join);
        let thread = if matches!(thread, Ok(Ok(()))) {
            "started"
        } else {
            "refused"
        };
        format!("clone {clone} setns {setns} clone3 {newer} thread {thread}")
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
            let name: OsString = path.file_name().map_or_default(ToOwned::to_owned);
            fs::copy(&path, collected.join(name))?;
        }
    }
    Ok(())
}
