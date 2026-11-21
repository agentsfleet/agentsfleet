//! Validated deployment limits; wire frame limits remain fixed by the protocol version.

use af_core::env::EnvSource;
use af_wire::protocol::{CONTROL_MESSAGE_BYTES_MAX, MESSAGE_BYTES_MAX};
use garde::{Unvalidated, Validate};
use serde::{Deserialize, Serialize};

use crate::{Result, error::ErrorKind};

/// Total control-frame memory for receive and send, in mebibytes.
pub const CONTROL_MEMORY_ENV: &str = "AGENTSFLEET_RUNNER_CONTROL_MEMORY_MIB";
/// Total work, report and activity frame memory for receive and send, in mebibytes.
pub const DATA_MEMORY_ENV: &str = "AGENTSFLEET_RUNNER_DATA_MEMORY_MIB";
const MIB: usize = 1024 * 1024;
/// Total control-frame bytes for both directions across one daemon replica.
pub const CONTROL_BUFFER_BYTES_ENV: &str = "AGENTSFLEET_RUNNER_CONTROL_BUFFER_BYTES";
/// Total work, report and activity bytes for both directions across one daemon replica.
pub const DATA_BUFFER_BYTES_ENV: &str = "AGENTSFLEET_RUNNER_DATA_BUFFER_BYTES";
// Each direction gets half a budget, and each half must hold one maximum frame
// of its kind, or a frame that size would wait for room its pool never has.
const CONTROL_MIN_MIB: usize = 12;
const DATA_MIN_MIB: usize = 2 * MESSAGE_BYTES_MAX / MIB;
const MAX_MIB: usize = 512;
const CONTROL_MIN_BYTES: usize = CONTROL_MIN_MIB * MIB;
const DATA_MIN_BYTES: usize = DATA_MIN_MIB * MIB;
const MAX_BYTES: usize = MAX_MIB * MIB;
const _: () = assert!(CONTROL_MIN_BYTES >= 2 * CONTROL_MESSAGE_BYTES_MAX);
const _: () = assert!(DATA_MIN_BYTES >= 2 * MESSAGE_BYTES_MAX && DATA_MIN_BYTES <= MAX_BYTES);

#[derive(Validate)]
#[garde(context(usize as floor))]
struct BufferBytes {
    #[garde(range(min = *floor, max = MAX_BYTES))]
    bytes: usize,
}

const DEFAULT_CONTROL_MIB: usize = 16;
const DEFAULT_DATA_MIB: usize = DATA_MIN_MIB;
// A default is a valid configuration, never a replacement for a failed parse.
const _: () = assert!(DEFAULT_CONTROL_MIB >= CONTROL_MIN_MIB && DEFAULT_CONTROL_MIB <= MAX_MIB);
const _: () = assert!(DEFAULT_DATA_MIB >= DATA_MIN_MIB && DEFAULT_DATA_MIB <= MAX_MIB);

/// Raw settings can also be deserialized from a configuration document before validation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct MemorySettings {
    /// Both directions need room for a maximum control frame.
    #[garde(range(min = CONTROL_MIN_MIB, max = MAX_MIB))]
    pub control_mib: usize,
    /// Both directions need room for a maximum work or report frame, so a
    /// large valid report never waits for an impossible reservation.
    #[garde(range(min = DATA_MIN_MIB, max = MAX_MIB))]
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
    /// Smallest control budget that holds a maximum control frame in each direction.
    pub const MIN_CONTROL_BUFFER_BYTES: usize = CONTROL_MIN_BYTES;
    /// Smallest data budget that holds a maximum work or report frame in each direction.
    pub const MIN_DATA_BUFFER_BYTES: usize = DATA_MIN_BYTES;
    /// Largest supported frame-buffer budget across both directions.
    pub const MAX_BUFFER_BYTES: usize = MAX_BYTES;

    /// Resolve once at boot; an invalid configured value is a startup fault.
    ///
    /// # Errors
    /// Names a nonnumeric setting or refuses an unsafe memory budget before allocation.
    pub fn from_env(env: &(impl EnvSource + ?Sized)) -> Result<Self> {
        Ok(Self {
            control_bytes: read_bytes(
                env,
                CONTROL_BUFFER_BYTES_ENV,
                CONTROL_MEMORY_ENV,
                DEFAULT_CONTROL_MIB,
                CONTROL_MIN_BYTES,
            )?,
            data_bytes: read_bytes(
                env,
                DATA_BUFFER_BYTES_ENV,
                DATA_MEMORY_ENV,
                DEFAULT_DATA_MIB,
                DATA_MIN_BYTES,
            )?,
        })
    }
}

fn read_bytes(
    env: &(impl EnvSource + ?Sized),
    byte_setting: &'static str,
    mib_setting: &'static str,
    fallback_mib: usize,
    floor: usize,
) -> Result<usize> {
    let bytes = match env.get(byte_setting) {
        Some(raw) => raw.parse().map_err(|source| ErrorKind::MemorySetting {
            setting: byte_setting,
            source,
        })?,
        None => read(env, mib_setting, fallback_mib)?.saturating_mul(MIB),
    };
    let count = Unvalidated::new(BufferBytes { bytes }).validate_with(&floor)?;
    Ok(count.bytes)
}

fn read(env: &(impl EnvSource + ?Sized), setting: &'static str, default: usize) -> Result<usize> {
    env.get(setting).map_or(Ok(default), |raw| {
        raw.parse()
            .map_err(|source| ErrorKind::MemorySetting { setting, source }.into())
    })
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
        // The data floor holds one 32 MiB frame in each direction.
        for (control, data) in [(0, 0), (11, 63), (513, 513), (usize::MAX, usize::MAX)] {
            for (settings, field) in [
                (
                    MemorySettings {
                        control_mib: control,
                        data_mib: 64,
                    },
                    "control_mib",
                ),
                (
                    MemorySettings {
                        control_mib: 16,
                        data_mib: data,
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

    /// An operator who omits the settings gets the documented 16 MiB and 64 MiB budgets.
    #[test]
    fn unset_settings_resolve_to_the_documented_defaults() -> crate::Result<()> {
        let settings = MemorySettings::default();
        assert_eq!((settings.control_mib, settings.data_mib), (16, 64));
        let limits =
            MemoryLimits::from_env(&af_core::env::MapEnv::from_pairs([("UNRELATED", "1")]))?;
        assert_eq!(
            (limits.control_bytes, limits.data_bytes),
            (16 * MIB, 64 * MIB)
        );
        Ok(())
    }

    /// A non-numeric value names its setting rather than falling back to a default.
    #[test]
    fn a_nonnumeric_setting_is_refused_by_name() -> crate::Result<()> {
        for setting in [
            super::DATA_MEMORY_ENV,
            super::CONTROL_BUFFER_BYTES_ENV,
            super::DATA_BUFFER_BYTES_ENV,
        ] {
            let env = af_core::env::MapEnv::from_pairs([(setting, "lots")]);
            let refused = MemoryLimits::from_env(&env)
                .err()
                .ok_or(crate::error::ErrorKind::Protocol)?;
            assert_eq!(
                refused.code(),
                af_core::error_code::INTERNAL_OPERATION_FAILED
            );
            assert!(
                refused
                    .to_string()
                    .contains(&format!("Set {setting} to a whole number."))
            );
            assert!(
                std::error::Error::source(&refused)
                    .is_some_and(<dyn std::error::Error>::is::<std::num::ParseIntError>)
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod byte_tests {
    use super::{CONTROL_BUFFER_BYTES_ENV, DATA_BUFFER_BYTES_ENV, MemoryLimits};
    use af_core::env::MapEnv;

    #[test]
    fn buffer_settings_retain_the_exact_configured_byte_count() -> crate::Result<()> {
        let env = MapEnv::from_pairs([
            (CONTROL_BUFFER_BYTES_ENV, "16777216"),
            (DATA_BUFFER_BYTES_ENV, "150994945"),
            (super::CONTROL_MEMORY_ENV, "invalid"),
        ]);
        let limits = MemoryLimits::from_env(&env)?;
        assert_eq!(limits.control_bytes, 16_777_216);
        assert_eq!(limits.data_bytes, 150_994_945);
        let (incoming, outgoing) = crate::Budget::duplex(limits);
        assert_eq!(
            incoming.available(af_wire::protocol::StreamKind::Control),
            8_388_608
        );
        assert_eq!(
            outgoing.available(af_wire::protocol::StreamKind::Work),
            75_497_472
        );
        Ok(())
    }

    #[test]
    fn byte_settings_refuse_impossible_and_overflowing_values() -> crate::Result<()> {
        for value in ["0", "12582911", "536870913", "18446744073709551615", "bad"] {
            let env = MapEnv::from_pairs([(CONTROL_BUFFER_BYTES_ENV, value)]);
            assert!(MemoryLimits::from_env(&env).is_err(), "{value}");
        }
        for value in ["12582912", "67108863", "536870913"] {
            let env = MapEnv::from_pairs([(DATA_BUFFER_BYTES_ENV, value)]);
            assert!(MemoryLimits::from_env(&env).is_err(), "{value}");
        }
        let floor = MapEnv::from_pairs([(DATA_BUFFER_BYTES_ENV, "67108864")]);
        assert_eq!(MemoryLimits::from_env(&floor)?.data_bytes, 67_108_864);
        Ok(())
    }
}
