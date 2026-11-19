//! Pool budgets resolve without a connection URL, before a command opens sockets.

use std::{num::ParseIntError, str::FromStr};

use af_core::env::EnvSource;
use garde::Validate;
use serde::Deserialize;

use super::DbRole;
use crate::{Result, error::ErrorKind};

/// File fields retain presence so omitted values can use existing environment inputs.
#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PoolOverrides {
    /// Maximum open sessions for this process and database role.
    pub max_connections: Option<u32>,
    /// Sessions warmed before traffic and retained while idle.
    pub min_connections: Option<u32>,
    /// Deadline for obtaining a pooled session, including a new handshake.
    pub acquire_timeout_ms: Option<u64>,
    /// Deadline for the direct startup connection probe.
    pub connect_timeout_ms: Option<u64>,
    /// Unused lifetime before the pool closes a session.
    pub idle_timeout_seconds: Option<u64>,
    /// Total lifetime before the pool replaces a session.
    pub max_lifetime_seconds: Option<u64>,
    /// Idle interval before checkout tests a session; zero tests every checkout.
    pub ping_after_idle_ms: Option<u64>,
}

/// Checked together after file and environment precedence has been applied.
#[derive(Debug, Clone, Copy, Validate)]
pub struct PoolSettings {
    #[garde(range(min = 1, max = 65_535))]
    /// Maximum open sessions for this process and database role.
    pub max_connections: u32,
    #[garde(range(max = self.max_connections))]
    /// Sessions warmed before traffic and retained while idle.
    pub min_connections: u32,
    #[garde(range(min = 1, max = 86_400_000))]
    /// Deadline for obtaining a pooled session, including a new handshake.
    pub acquire_timeout_ms: u64,
    #[garde(range(min = 1, max = 86_400_000))]
    /// Deadline for the direct startup connection probe.
    pub connect_timeout_ms: u64,
    #[garde(range(min = 1, max = 31_536_000))]
    /// Unused lifetime before the pool closes a session.
    pub idle_timeout_seconds: u64,
    #[garde(range(min = 1, max = 31_536_000))]
    /// Total lifetime before the pool replaces a session.
    pub max_lifetime_seconds: u64,
    #[garde(range(max = self.idle_timeout_seconds.saturating_mul(1000).saturating_sub(1)))]
    /// Idle interval before checkout tests a session; zero tests every checkout.
    pub ping_after_idle_ms: u64,
}

impl PoolSettings {
    /// Refuses a configured typo or impossible floor instead of silently changing it.
    ///
    /// # Errors
    /// Preserves numeric parsing and Garde field failures without retaining input values.
    pub fn resolve(
        env: &(impl EnvSource + ?Sized),
        role: DbRole,
        file: PoolOverrides,
    ) -> Result<garde::Valid<Self>> {
        let max_connections = read(
            env,
            role,
            super::POOL_SIZE_KNOB,
            file.max_connections,
            super::pool_size_default(),
        )?;
        let min_connections = read(
            env,
            role,
            super::MIN_POOL_SIZE_KNOB,
            file.min_connections,
            super::min_connections_default(max_connections),
        )?;
        let acquire_timeout_ms = read(
            env,
            role,
            super::ACQUIRE_TIMEOUT_KNOB,
            file.acquire_timeout_ms,
            super::ACQUIRE_TIMEOUT_DEFAULT.as_secs() * 1000,
        )?;
        Ok(garde::Unvalidated::new(Self {
            max_connections,
            min_connections,
            acquire_timeout_ms,
            connect_timeout_ms: file
                .connect_timeout_ms
                .unwrap_or(super::CONNECT_TIMEOUT_DEFAULT.as_secs() * 1000),
            idle_timeout_seconds: file
                .idle_timeout_seconds
                .unwrap_or(crate::pool::IDLE_TIMEOUT.as_secs()),
            max_lifetime_seconds: file
                .max_lifetime_seconds
                .unwrap_or(crate::pool::MAX_LIFETIME.as_secs()),
            ping_after_idle_ms: file
                .ping_after_idle_ms
                .unwrap_or(crate::pool::PING_AFTER_IDLE.as_secs() * 1000),
        })
        .validate()
        .map_err(|source| ErrorKind::InvalidPoolBudget { source })?)
    }
}

fn read<T: FromStr<Err = ParseIntError>>(
    env: &(impl EnvSource + ?Sized),
    role: DbRole,
    base: &str,
    file: Option<T>,
    default: T,
) -> Result<T> {
    if let Some(value) = file {
        return Ok(value);
    }
    let scoped = format!("{base}{}", role.knob_suffix());
    let selected = env
        .get(&scoped)
        .map(|value| (scoped, value))
        .or_else(|| env.get(base).map(|value| (base.to_owned(), value)));
    selected.map_or(Ok(default), |(knob, raw)| {
        raw.trim().parse().map_err(|source| {
            ErrorKind::InvalidPoolNumber {
                knob: knob.into(),
                source,
            }
            .into()
        })
    })
}
