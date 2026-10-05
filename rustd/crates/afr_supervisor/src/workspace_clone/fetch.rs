//! The host-side mirror of one repository: a bare repository fetched over
//! HTTPS, the token riding an in-memory header that no config file ever
//! holds.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use gix::remote::Direction;
use gix::remote::fetch::Status;

/// What the git library fails with, boxed: its calls fail with many types,
/// and the supervisor only reports which step failed and why.
pub(crate) type GitError = Box<dyn std::error::Error + Send + Sync>;
/// The one alias this module's fallible steps spell.
pub(crate) type GitResult<T> = std::result::Result<T, GitError>;

/// The remote every mirror fetches from.
const ORIGIN: &str = "origin";
/// The configuration key an extra request header is set under.
const EXTRA_HEADER: &str = "http.extraHeader";

/// What bringing a mirror up to date found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetched {
    /// The mirror did not exist, so the repository was fetched whole.
    Cloned,
    /// New objects arrived.
    Updated,
    /// Nothing new: the mirror already held every object.
    Unchanged,
}

impl Fetched {
    /// How it reads in a log line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cloned => "cloned",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
        }
    }
}

/// Brings the bare mirror at `mirror` up to date with `url`, presenting
/// `header` with every request. A mirror that will not open is replaced; one
/// that opens but will not fetch is kept, so an outage does not cost the cache.
pub(super) fn fetch(
    url: &str,
    mirror: &Path,
    header: &str,
    stop: &AtomicBool,
) -> GitResult<Fetched> {
    let options = || isolated(header);
    if mirror.exists() {
        match gix::open_opts(mirror, options()) {
            Ok(repository) => return update(&repository, stop),
            Err(_unreadable) => fs::remove_dir_all(mirror)?,
        }
    }
    if let Some(parent) = mirror.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut prepare = gix::clone::PrepareFetch::new(
        url,
        mirror,
        gix::create::Kind::Bare,
        gix::create::Options::default(),
        options(),
    )?;
    prepare.fetch_only(gix::progress::Discard, stop)?;
    Ok(Fetched::Cloned)
}

/// Fetches what `repository`'s origin has that it lacks.
fn update(repository: &gix::Repository, stop: &AtomicBool) -> GitResult<Fetched> {
    let outcome = repository
        .find_remote(ORIGIN)?
        .connect(Direction::Fetch)?
        .prepare_fetch(
            gix::progress::Discard,
            gix::remote::ref_map::Options::default(),
        )?
        .receive(gix::progress::Discard, stop)?;
    Ok(match outcome.status {
        Status::NoPackReceived { .. } => Fetched::Unchanged,
        Status::Change { .. } => Fetched::Updated,
    })
}

/// Opening options that read no configuration from this host, so no system
/// or user setting and no credential helper takes part, with `header` set in
/// memory only.
fn isolated(header: &str) -> gix::open::Options {
    gix::open::Options::isolated().config_overrides([format!("{EXTRA_HEADER}={header}")])
}
