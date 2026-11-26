//! How a failure becomes an [`Error`](super::Error): the lifts, and the raisers
//! that bind what only the call site knows.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2).
af_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    afr_engine::Error => Seam,
);

/// Reports a cgroup control file that refused a write, naming the file. Public:
/// the engine opens the tenant leaf's files for the sandbox entry.
pub fn cgroup(file: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::Cgroup { file, source }.into()
}

/// Reports a cgroup file that could not be read, naming the file.
pub fn cgroup_unreadable(file: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::CgroupUnreadable { file, source }.into()
}

/// Reports a cgroup that would not go, naming it.
pub(crate) fn cgroup_left(path: &std::path::Path) -> impl Fn(std::io::Error) -> Error {
    move |source| {
        ErrorKind::CgroupLeft {
            path: path.to_owned(),
            source,
        }
        .into()
    }
}

/// Reports a cgroup that never reached `state` after it was asked to.
pub(crate) fn cgroup_unsettled(path: &std::path::Path, state: &'static str) -> Error {
    ErrorKind::CgroupUnsettled {
        path: path.to_owned(),
        state,
    }
    .into()
}
