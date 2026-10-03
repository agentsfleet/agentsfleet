//! The GitHub request vocabulary a repository binding compiles into.
//!
//! The daemon writes these spellings into a lease's locked request rules
//! (`afd_gate::policy::egress`); the runner reads the same rules back to tell
//! the model which branch it may publish. One spelling for both ends, so the
//! rule the daemon compiles and the context the runner renders cannot drift.

/// The ref namespace a repair branch is created under.
pub const REFS_HEADS: &str = "refs/heads/";

/// The `git/refs` field naming the ref being created.
pub const FIELD_REF: &str = "ref";
/// The `pulls` field naming the branch a Pull Request opens from.
pub const FIELD_HEAD: &str = "head";
/// The `pulls` field naming the branch it opens into.
pub const FIELD_BASE: &str = "base";
/// The `pulls` field deciding whether it opens as a draft.
pub const FIELD_DRAFT: &str = "draft";

/// The endpoint that publishes a branch.
pub const REFS_PATH: &str = "/git/refs";
/// The endpoint that opens a Pull Request.
pub const PULLS_PATH: &str = "/pulls";

/// The API path of `suffix` under `repository` (`owner/name`).
///
/// `suffix` starts with `/`; `/` alone is the repository's own subtree.
#[must_use]
pub fn path(repository: &str, suffix: &str) -> String {
    format!("/repos/{repository}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::{REFS_PATH, path};

    #[test]
    fn a_suffix_lands_under_the_repository() {
        assert_eq!(path("acme/widgets", REFS_PATH), "/repos/acme/widgets/git/refs");
        assert_eq!(path("acme/widgets", "/"), "/repos/acme/widgets/");
    }
}
