//! Bubblewrap's interpretation of the host's measured capabilities.

use crate::probe::{
    HostProbe, MECHANISM_BUBBLEWRAP, MECHANISM_LANDLOCK, MECHANISM_SECCOMP,
    MECHANISM_TOOLBOX_FILESYSTEM,
};

/// Required mechanisms for the bubblewrap backend; other backends choose
/// their own requirements through `Backend::validate`.
#[derive(Debug, Clone, Copy)]
pub struct Requirements;

impl Requirements {
    /// The first mechanism every sandbox needs that this host lacks, or `None`
    /// when a sandbox can be built here.
    #[must_use]
    pub fn missing(host: &HostProbe) -> Option<&'static str> {
        [
            (host.landlock, MECHANISM_LANDLOCK),
            (host.seccomp, MECHANISM_SECCOMP),
            (host.bubblewrap, MECHANISM_BUBBLEWRAP),
            (host.toolbox_filesystem, MECHANISM_TOOLBOX_FILESYSTEM),
        ]
        .into_iter()
        .find_map(|(present, name)| (!present).then_some(name))
        .or_else(|| host.missing_controller())
    }
}
