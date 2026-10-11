//! The provider's per-person resource, addressed by subject.
//!
//! The claim read and the metadata write both name one person under the same
//! API base and carry the backend secret there. The subject comes from a token
//! claim, so it is spliced in as ONE escaped path segment: a `/`, `?`, `#` or
//! `%` inside it addresses that person, never a sibling resource.
//!
//! `.` and `..` are refused rather than pushed. A URL path resolves them, so
//! `..` would aim the secret-bearing request at the user COLLECTION instead of
//! at a person, and no escaping can make either name a single user.

use afd_auth::principal::Subject;
use afd_validate::not_relative;
use reqwest::Url;

/// The collection every person lives under, appended to the API base.
const USERS_SEGMENT: &str = "users";

/// The provider's user collection, parsed once at construction.
#[derive(Debug, Clone)]
pub(crate) struct UsersEndpoint(Url);

impl UsersEndpoint {
    /// Parses `{api_base}/users`, or `None` when that is not a URL a person's
    /// resource can be appended to.
    ///
    /// Joined as text before parsing, so a base's own trailing slash survives
    /// exactly as an operator wrote it.
    pub(crate) fn parse(api_base: &str) -> Option<Self> {
        let users = Url::parse(&format!("{api_base}/{USERS_SEGMENT}")).ok()?;
        (!users.cannot_be_a_base()).then_some(Self(users))
    }

    /// The URL of `subject`'s resource, followed by `tail`.
    ///
    /// `None` for a subject that is a dot segment.
    pub(crate) fn person(&self, subject: &Subject, tail: &[&str]) -> Option<Url> {
        let name = subject.as_str();
        not_relative(name, &()).ok()?;
        let mut url = self.0.clone();
        url.path_segments_mut()
            .ok()?
            .push(name)
            .extend(tail.iter().copied());
        Some(url)
    }
}

#[cfg(test)]
#[path = "users/tests.rs"]
mod tests;
