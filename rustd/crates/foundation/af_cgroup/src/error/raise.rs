//! How a failure becomes an [`Error`](super::Error): the raisers that bind
//! what only the call site knows.

use super::{Error, ErrorKind};

/// Reports a cgroup control file that refused a write, naming the file.
pub fn cgroup(file: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::Cgroup { file, source }.into()
}

/// Reports a cgroup file that could not be read, naming the file.
pub fn cgroup_unreadable(file: &'static str) -> impl Fn(std::io::Error) -> Error {
    move |source| ErrorKind::CgroupUnreadable { file, source }.into()
}

/// Reports a cgroup that never reached `state` after it was asked to.
pub(crate) fn cgroup_unsettled(path: &std::path::Path, state: &'static str) -> Error {
    ErrorKind::CgroupUnsettled {
        path: path.to_owned(),
        state,
    }
    .into()
}
