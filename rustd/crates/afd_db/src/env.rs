//! Where configuration is read from, as a seam rather than a global.
//!
//! `std::env::set_var` is `unsafe` in edition 2024 because the process
//! environment is shared mutable state and a parallel test suite racing on it
//! is undefined behaviour. So the environment arrives as a parameter: the
//! daemon passes [`ProcessEnv`], and tests pass [`MapEnv`], which is how the
//! role and knob resolution in [`crate::config`] gets exercised at all without
//! one test's `DATABASE_URL_API` leaking into another's.

/// A source of configuration values, keyed by environment-variable name.
pub trait EnvSource {
    /// The value for `key`, or `None` when it is unset.
    ///
    /// A blank or whitespace-only value is the caller's business, not this
    /// trait's: [`crate::config`] treats it as unset, and a source that decided
    /// that here would hide the distinction from anyone who needs it.
    fn get(&self, key: &str) -> Option<String>;
}

/// The real process environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// A fixed set of values, for tests that need to drive resolution without
/// touching the process environment.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone, Default)]
pub struct MapEnv(std::collections::BTreeMap<String, String>);

#[cfg(feature = "test-util")]
impl MapEnv {
    /// Builds an environment from name/value pairs.
    #[must_use]
    pub fn from_pairs<'a, I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        Self(
            pairs
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        )
    }
}

#[cfg(feature = "test-util")]
impl EnvSource for MapEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}
