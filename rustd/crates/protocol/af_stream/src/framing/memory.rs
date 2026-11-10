//! Validated deployment limits; wire frame limits remain fixed by the protocol version.

use af_core::env::EnvSource;
use garde::{Unvalidated, Validate};
use serde::{Deserialize, Serialize};

use crate::{Result, error::ErrorKind};

/// Total control-frame memory for receive and send, in mebibytes.
pub const CONTROL_MEMORY_ENV: &str = "AGENTSFLEET_RUNNER_CONTROL_MEMORY_MIB";
/// Total work, report and activity frame memory for receive and send, in mebibytes.
pub const DATA_MEMORY_ENV: &str = "AGENTSFLEET_RUNNER_DATA_MEMORY_MIB";
const MIB: usize = 1024 * 1024;
const DEFAULT_CONTROL_MIB: usize = 16;
const DEFAULT_DATA_MIB: usize = 48;
// A default is a valid configuration, never a replacement for a failed parse.
const _: () = assert!(DEFAULT_CONTROL_MIB >= 12 && DEFAULT_CONTROL_MIB <= 512);
const _: () = assert!(DEFAULT_DATA_MIB >= 12 && DEFAULT_DATA_MIB <= 512);

/// Raw settings can also be deserialized from a configuration document before validation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct MemorySettings {
    /// Both directions need room for a maximum frame and its transient copy.
    #[garde(range(min = 12, max = 512))]
    pub control_mib: usize,
    /// The same minimum prevents a large valid report from waiting for an impossible reservation.
    #[garde(range(min = 12, max = 512))]
    pub data_mib: usize,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            control_mib: DEFAULT_CONTROL_MIB,
            data_mib: DEFAULT_DATA_MIB,
        }
    }
}

/// Private fields guarantee that every allocation consumes already-validated limits.
#[derive(Debug, Clone, Copy)]
pub struct MemoryLimits {
    pub(super) control_bytes: usize,
    pub(super) data_bytes: usize,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            control_bytes: DEFAULT_CONTROL_MIB * MIB,
            data_bytes: DEFAULT_DATA_MIB * MIB,
        }
    }
}

impl TryFrom<MemorySettings> for MemoryLimits {
    type Error = crate::Error;

    fn try_from(settings: MemorySettings) -> Result<Self> {
        let settings = Unvalidated::new(settings).validate()?;
        Ok(Self {
            control_bytes: settings.control_mib * MIB,
            data_bytes: settings.data_mib * MIB,
        })
    }
}

impl MemoryLimits {
    /// Resolve once at boot; an invalid configured value is a startup fault.
    ///
    /// # Errors
    /// Names a nonnumeric setting or refuses an unsafe memory budget before allocation.
    pub fn from_env(env: &(impl EnvSource + ?Sized)) -> Result<Self> {
        MemorySettings {
            control_mib: read(env, CONTROL_MEMORY_ENV, DEFAULT_CONTROL_MIB)?,
            data_mib: read(env, DATA_MEMORY_ENV, DEFAULT_DATA_MIB)?,
        }
        .try_into()
    }
}

fn read(env: &(impl EnvSource + ?Sized), setting: &'static str, default: usize) -> Result<usize> {
    match env.get(setting) {
        Some(raw) => raw
            .parse()
            .map_err(|source| ErrorKind::MemorySetting { setting, source }.into()),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::{MIB, MemoryLimits, MemorySettings};
    use crate::Budget;
    use af_wire::protocol::StreamKind;

    /// A deployment can change the total without coupling incoming and outgoing reservations.
    #[test]
    fn configured_memory_is_partitioned_between_independent_directions() -> crate::Result<()> {
        let limits = MemoryLimits::try_from(MemorySettings {
            control_mib: 24,
            data_mib: 64,
        })?;
        let (incoming, outgoing) = Budget::duplex(limits);
        assert_eq!(incoming.available(StreamKind::Control), 12 * MIB);
        assert_eq!(outgoing.available(StreamKind::Control), 12 * MIB);
        assert_eq!(incoming.available(StreamKind::Reports), 32 * MIB);
        assert_eq!(outgoing.available(StreamKind::Reports), 32 * MIB);
        Ok(())
    }

    /// Too little memory would wait forever for a reservation larger than its entire pool.
    #[test]
    fn impossible_or_excessive_memory_settings_are_refused() -> crate::Result<()> {
        for value in [0, 11, 513, usize::MAX] {
            for (settings, field) in [
                (
                    MemorySettings {
                        control_mib: value,
                        data_mib: 48,
                    },
                    "control_mib",
                ),
                (
                    MemorySettings {
                        control_mib: 16,
                        data_mib: value,
                    },
                    "data_mib",
                ),
            ] {
                let failure = MemoryLimits::try_from(settings)
                    .err()
                    .ok_or(crate::error::ErrorKind::Protocol)?;
                assert_eq!(failure.reason(), crate::Reason::Protocol);
                assert_eq!(
                    failure.code(),
                    af_core::error_code::INTERNAL_OPERATION_FAILED
                );
                let bounds = std::error::Error::source(&failure)
                    .and_then(|source| source.downcast_ref::<garde::Report>())
                    .ok_or(crate::error::ErrorKind::Protocol)?;
                assert!(bounds.iter().any(|(path, _)| path.to_string() == field));
            }
        }
        Ok(())
    }

    /// An operator who omits the settings gets the documented 16 MiB and 48 MiB budgets.
    #[test]
    fn unset_settings_resolve_to_the_documented_defaults() -> crate::Result<()> {
        let settings = MemorySettings::default();
        assert_eq!((settings.control_mib, settings.data_mib), (16, 48));
        let limits =
            MemoryLimits::from_env(&af_core::env::MapEnv::from_pairs([("UNRELATED", "1")]))?;
        assert_eq!(
            (limits.control_bytes, limits.data_bytes),
            (16 * MIB, 48 * MIB)
        );
        Ok(())
    }

    /// A non-numeric value names its setting rather than falling back to a default.
    #[test]
    fn a_nonnumeric_setting_is_refused_by_name() {
        let env = af_core::env::MapEnv::from_pairs([(super::DATA_MEMORY_ENV, "lots")]);
        let refused = MemoryLimits::from_env(&env).err();
        assert!(
            refused.as_ref().is_some_and(|error| error
                .to_string()
                .contains("Set AGENTSFLEET_RUNNER_DATA_MEMORY_MIB to a whole number of mebibytes")),
            "{refused:?}"
        );
    }
}
