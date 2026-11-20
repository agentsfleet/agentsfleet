//! Supervised maintenance cadences are checked before opening any service.

use afd_runner::sweep::{liveness::LivenessSettings, retention::RetentionSettings};
use garde::Validate;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Maintenance {
    #[garde(range(max = 5000))]
    pub startup_spread_ms: u64,
    #[garde(skip)]
    pub runner_liveness: LivenessSettings,
    #[garde(skip)]
    pub runner_history: RetentionSettings,
    #[garde(dive)]
    pub expired_work: ExpiredWork,
    #[garde(dive)]
    pub fleet_census: FleetCensus,
    #[garde(dive)]
    pub setup_cleanup: SetupCleanup,
    #[garde(dive)]
    pub billing: Billing,
    #[garde(dive)]
    pub durable_tasks: DurableTasks,
    #[garde(dive)]
    pub checkpoint_storage: CheckpointStorage,
    #[garde(dive)]
    pub outbound_delivery: OutboundDelivery,
}

impl Default for Maintenance {
    fn default() -> Self {
        Self {
            startup_spread_ms: 5000,
            runner_liveness: LivenessSettings::default(),
            runner_history: RetentionSettings::default(),
            expired_work: ExpiredWork::default(),
            fleet_census: FleetCensus::default(),
            setup_cleanup: SetupCleanup::default(),
            billing: Billing::default(),
            durable_tasks: DurableTasks::default(),
            checkpoint_storage: CheckpointStorage::default(),
            outbound_delivery: OutboundDelivery::default(),
        }
    }
}

macro_rules! cadence {
    ($name:ident, $seconds:literal) => {
        #[derive(Debug, Clone, Copy, Deserialize, Validate)]
        #[serde(default, deny_unknown_fields)]
        pub(crate) struct $name {
            #[garde(range(min = 1, max = 86_400))]
            pub interval_seconds: u64,
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    interval_seconds: $seconds,
                }
            }
        }
    };
}

macro_rules! cycle {
    ($name:ident, $period:literal, $deadline:literal) => {
        #[derive(Debug, Clone, Copy, Deserialize, Validate)]
        #[serde(default, deny_unknown_fields)]
        pub(crate) struct $name {
            #[garde(range(min = 1, max = 86_400))]
            pub interval_seconds: u64,
            #[garde(range(min = 1, max = 86_400))]
            pub pass_timeout_seconds: u64,
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    interval_seconds: $period,
                    pass_timeout_seconds: $deadline,
                }
            }
        }
    };
}

cadence!(ExpiredWork, 2);
cadence!(FleetCensus, 30);
cadence!(SetupCleanup, 10);
cadence!(OutboundDelivery, 30);
cycle!(DurableTasks, 10, 30);
cycle!(CheckpointStorage, 300, 240);

#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Billing {
    #[garde(range(min = 1, max = 3600))]
    pub interval_seconds: u64,
    #[garde(range(min = self.interval_seconds, max = 86_400))]
    pub retry_oldest_after_seconds: u64,
    #[garde(range(min = 1, max = 100))]
    pub max_drain_passes: usize,
}

impl Default for Billing {
    fn default() -> Self {
        Self {
            interval_seconds: 5,
            retry_oldest_after_seconds: 60,
            max_drain_passes: 10,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MaintenancePolicy {
    pub loops: garde::Valid<Maintenance>,
    pub liveness: garde::Valid<LivenessSettings>,
    pub retention: garde::Valid<RetentionSettings>,
}

impl MaintenancePolicy {
    pub(crate) fn resolve(settings: Maintenance) -> Result<Self, garde::Report> {
        Ok(Self {
            liveness: garde::Unvalidated::new(settings.runner_liveness).validate()?,
            retention: garde::Unvalidated::new(settings.runner_history).validate()?,
            loops: garde::Unvalidated::new(settings).validate()?,
        })
    }
}
