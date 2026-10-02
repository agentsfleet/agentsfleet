//! Where configuration is read from, as a seam rather than a global.
//!
//! `std::env::set_var` is `unsafe` in edition 2024 because the process
//! environment is shared mutable state and a parallel test suite racing on it
//! is undefined behaviour. So the environment arrives as a parameter: the
//! daemon passes [`ProcessEnv`], and tests pass [`MapEnv`], which is how the
//! role and knob resolution in [`crate::config`] gets exercised at all without
//! one test's `DATABASE_URL_API` leaking into another's.

/// The environment variable naming how much to log, read by the daemon and
/// the runner alike.
///
/// Its VALUE is a level — `error`, `warn`, `info`, `debug`, `trace`, `off` —
/// so `AGENTSFLEET_LOG_LEVEL=debug agentsfleetd serve`. Not a file: records go
/// to stderr, and where they go from there is the collector's business.
///
/// Spelled in full rather than as a bare `AGENTSFLEET_LOG`, so the name says
/// which knob it is at the call site and in a deployment manifest.
pub const LOG_LEVEL_VAR: &str = "AGENTSFLEET_LOG_LEVEL";

/// The level [`LOG_LEVEL_VAR`] names, or `fallback` when it is unset or
/// unreadable.
///
/// Falls back rather than refusing: a typo in a debugging aid must not stop a
/// process starting. Generic over the level type so this value layer links no
/// logging crate; each binary passes its subscriber's own.
pub fn log_level<L: core::str::FromStr>(env: &(impl EnvSource + ?Sized), fallback: L) -> L {
    env.get(LOG_LEVEL_VAR)
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(fallback)
}

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

#[cfg(test)]
mod tests {
    use super::{EnvSource, LOG_LEVEL_VAR, log_level};

    /// An environment holding at most the log-level knob.
    struct Level(Option<&'static str>);

    impl EnvSource for Level {
        fn get(&self, key: &str) -> Option<String> {
            (key == LOG_LEVEL_VAR).then_some(self.0?.to_owned())
        }
    }

    #[test]
    fn the_log_level_is_read_or_falls_back() {
        assert_eq!(
            log_level(&Level(Some(" 7 ")), 3_u8),
            7,
            "a readable level is used, trimmed"
        );
        assert_eq!(
            log_level(&Level(Some("loud")), 3_u8),
            3,
            "an unreadable one falls back"
        );
        assert_eq!(log_level(&Level(None), 3_u8), 3, "an unset one falls back");
    }
}
