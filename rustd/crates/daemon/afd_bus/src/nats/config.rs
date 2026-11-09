//! Boot reads one setting: `NATS_URL`, comma-separated seeds with credentials in
//! the userinfo. The development namespace prefixes every subject.
//!
//! The password is letters and digits: `#`, `?` or `/` would end the URL's
//! userinfo here, and the Fly node (`deploy/fly/nats-*/Dockerfile`) splits the
//! same string on `@` and `:`.

use std::str::FromStr as _;

use af_core::env::{EnvSource, optional};
use async_nats::ServerAddr;
use garde::{Unvalidated, Validate};

use super::Tuning;
use crate::{Result, error::ErrorKind};

/// Comma-separated seed URLs, such as `nats://user:pass@nats-1:4222,nats://user:pass@nats-2:4222`.
pub const URL_ENV: &str = "NATS_URL";
/// Optional development namespace; empty keeps the deployed subject spelling.
pub const NAMESPACE_ENV: &str = "AGENTSFLEET_DEV_NAMESPACE";
const MAX_NAMESPACE_BYTES: usize = 64;

/// Proven connection settings; holding one means every seed parsed.
#[derive(Debug, Clone)]
pub struct Config {
    pub(super) seeds: Vec<ServerAddr>,
    /// The seeds' shared user and password; the client authenticates only
    /// from options, never from a URL's userinfo.
    pub(super) credentials: Option<(String, String)>,
    pub(super) prefix: String,
    pub(super) tuning: Tuning,
}

#[derive(Validate)]
struct Namespace<'a> {
    #[garde(
        length(bytes, min = 1, max = MAX_NAMESPACE_BYTES),
        custom(af_validate::charset(is_namespace_char))
    )]
    value: &'a str,
}

const fn is_namespace_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '-' | '_')
}

impl Config {
    /// Resolve the seeds and namespace once at boot, before any connection.
    ///
    /// # Errors
    /// Refuses an unset `NATS_URL`, a seed that is not a NATS URL, or a
    /// namespace outside its charset, each as `UZ-STARTUP-001`.
    pub fn from_env(env: &(impl EnvSource + ?Sized)) -> Result<Self> {
        let raw = optional(env, URL_ENV).ok_or(ErrorKind::NatsUrlUnset)?;
        let seeds = raw
            .split(',')
            .map(str::trim)
            .filter(|seed| !seed.is_empty())
            .map(ServerAddr::from_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| ErrorKind::NatsUrl { source })?;
        if seeds.is_empty() {
            return Err(ErrorKind::NatsUrlUnset.into());
        }
        let credentials = credentials_of(&seeds)?;
        let prefix = match optional(env, NAMESPACE_ENV) {
            None => String::new(),
            Some(namespace) => prefix_of(&namespace)?,
        };
        Ok(Self {
            seeds,
            credentials,
            prefix,
            tuning: Tuning::default(),
        })
    }

    /// Replace the default limits, such as from a configuration file.
    ///
    /// # Errors
    /// `UZ-STARTUP-001` when a limit is outside its range.
    pub fn with_tuning(mut self, tuning: Tuning) -> Result<Self> {
        self.tuning = tuning.prove()?.into_inner();
        Ok(self)
    }

    /// The proven limits this connection uses.
    #[must_use]
    pub const fn tuning(&self) -> &Tuning {
        &self.tuning
    }
}

fn credentials_of(seeds: &[ServerAddr]) -> Result<Option<(String, String)>> {
    let pair = |seed: &ServerAddr| {
        seed.username().map(|user| {
            (
                user.to_owned(),
                seed.password().unwrap_or_default().to_owned(),
            )
        })
    };
    let first = seeds.first().and_then(pair);
    if seeds.iter().any(|seed| pair(seed) != first) {
        return Err(ErrorKind::NatsCredentials.into());
    }
    Ok(first)
}

fn prefix_of(namespace: &str) -> Result<String> {
    let proved = Unvalidated::new(Namespace { value: namespace })
        .validate()
        .map_err(|source| ErrorKind::NatsNamespace { source })?;
    Ok(format!("dev.{}.", proved.value))
}

#[cfg(test)]
mod tests {
    use af_core::{env::MapEnv, error_code};

    use super::{Config, NAMESPACE_ENV, Tuning, URL_ENV};

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
    const TWO_SEEDS: &str = "nats://agentsfleet:agentsfleet@127.0.0.1:7001, nats://agentsfleet:agentsfleet@127.0.0.1:7002";

    #[test]
    fn boot_refuses_an_unset_nats_url() -> TestResult {
        for env in [
            MapEnv::default(),
            MapEnv::from_pairs([(URL_ENV, "  ")]),
            MapEnv::from_pairs([(URL_ENV, " , ")]),
        ] {
            let error = Config::from_env(&env)
                .err()
                .ok_or("an empty NATS_URL must refuse boot")?;
            assert_eq!(error.code(), error_code::STARTUP_ENV_CHECK);
            assert!(error.to_string().contains(URL_ENV), "{error}");
        }
        Ok(())
    }

    #[test]
    fn every_comma_separated_seed_is_kept_with_its_credentials() -> TestResult {
        let config = Config::from_env(&MapEnv::from_pairs([(URL_ENV, TWO_SEEDS)]))?;
        assert_eq!(config.seeds.len(), 2);
        assert_eq!(
            config.credentials,
            Some(("agentsfleet".to_owned(), "agentsfleet".to_owned()))
        );
        assert_eq!(
            config.seeds.get(1).map(async_nats::ServerAddr::port),
            Some(7002)
        );
        assert!(
            config.prefix.is_empty(),
            "no namespace keeps deployed subjects"
        );
        Ok(())
    }

    #[test]
    fn seeds_that_disagree_on_credentials_are_refused() -> TestResult {
        for raw in [
            "nats://a:x@127.0.0.1:7001,nats://b:x@127.0.0.1:7002",
            "nats://a:x@127.0.0.1:7001,nats://127.0.0.1:7002",
        ] {
            let error = Config::from_env(&MapEnv::from_pairs([(URL_ENV, raw)]))
                .err()
                .ok_or("mixed credentials must refuse boot")?;
            assert_eq!(error.code(), error_code::STARTUP_ENV_CHECK);
            assert!(
                !error.to_string().contains("x@"),
                "no credential in the message"
            );
        }
        let open = Config::from_env(&MapEnv::from_pairs([(URL_ENV, "nats://127.0.0.1:7001")]))?;
        assert_eq!(
            open.credentials, None,
            "a seed without userinfo is unauthenticated"
        );
        Ok(())
    }

    #[test]
    fn a_seed_that_is_not_a_nats_url_is_refused() -> TestResult {
        let env = MapEnv::from_pairs([(URL_ENV, "nats://127.0.0.1:7001,http://[::1")]);
        let error = Config::from_env(&env)
            .err()
            .ok_or("a malformed seed must refuse boot")?;
        assert_eq!(error.code(), error_code::STARTUP_ENV_CHECK);
        assert_eq!(error.reason(), "configuration");
        Ok(())
    }

    #[test]
    fn the_namespace_prefixes_subjects_and_its_charset_is_enforced() -> TestResult {
        let named = MapEnv::from_pairs([(URL_ENV, TWO_SEEDS), (NAMESPACE_ENV, "indy-1")]);
        assert_eq!(Config::from_env(&named)?.prefix, "dev.indy-1.");
        let long = "x".repeat(65);
        for bad in ["has.dot", "has space", long.as_str()] {
            let env = MapEnv::from_pairs([(URL_ENV, TWO_SEEDS), (NAMESPACE_ENV, bad)]);
            let error = Config::from_env(&env)
                .err()
                .ok_or("an invalid namespace must be refused")?;
            assert_eq!(error.code(), error_code::STARTUP_ENV_CHECK);
        }
        Ok(())
    }

    #[test]
    fn tuning_replaces_the_defaults_only_when_proven() -> TestResult {
        let config = Config::from_env(&MapEnv::from_pairs([(URL_ENV, TWO_SEEDS)]))?;
        assert_eq!(config.tuning(), &Tuning::default());
        let small = Tuning {
            subscription_capacity: 8,
            ..Tuning::default()
        };
        let config = config.with_tuning(small.clone())?;
        assert_eq!(config.tuning(), &small);
        let error = config
            .with_tuning(Tuning {
                channel_capacity: 0,
                ..Tuning::default()
            })
            .err()
            .ok_or("an out-of-range limit must refuse")?;
        assert_eq!(error.code(), error_code::STARTUP_ENV_CHECK);
        Ok(())
    }
}
