//! Vendor-facing fixture clients never reach a real provider.

use super::*;
use afd_connector::{Connectors, Exchange, Grants, PlatformApp};
use afd_cron::{Fire, QStash, ScheduleService, Schedules as CronSchedules};

#[expect(
    clippy::disallowed_methods,
    reason = "the fixture connector clients stand in for production's vendor clients and never reach a real provider; a proxy rule and a deadline are not what the router suites prove"
)]
pub(super) fn connectors(database: &Db, queue: &Dragonfly) -> Connectors {
    Connectors::new(
        PlatformApp::new(vault(database.clone())),
        Grants::new(vault(database.clone()), database.clone(), Entropy::new()),
        Exchange::new(reqwest::Client::new()),
        reqwest::Client::new(),
        queue.clone(),
        Entropy::new(),
    )
}

#[expect(
    clippy::disallowed_methods,
    reason = "the fixture Slack client posts only to a port that refuses the connection; a proxy rule and a deadline are not what the router suites prove"
)]
pub(super) fn interjector(database: &Db) -> afd_outbound::Interjector {
    afd_outbound::Interjector::new(afd_outbound::SlackPoster::new(
        Grants::new(vault(database.clone()), database.clone(), Entropy::new()),
        reqwest::Client::new(),
        NOWHERE_SLACK.to_owned(),
    ))
}

pub(super) fn logins(queue: &Dragonfly) -> Logins {
    Logins::new(
        afd_dragonfly::SessionStore::new(queue.clone()),
        SecretBytes::new(FIXTURE_PEPPER.to_vec()),
        Entropy::new(),
        FIXTURE_APP_URL,
    )
}

#[expect(
    clippy::disallowed_methods,
    reason = "the fixture scheduler client names a reserved .test host that never resolves; a proxy rule and a deadline are not what the router suites prove"
)]
pub(super) fn schedules(database: &Db, ledger: &afd_admission::Admissions) -> SchedulePlane {
    SchedulePlane::new(
        ScheduleService::new(
            CronSchedules::new(database.clone(), Entropy::new()),
            QStash::new(
                reqwest::Client::new(),
                afd_crypto::secret::SecretString::new(String::new()),
                SCHEDULE_DESTINATION.to_owned(),
                SCHEDULE_API_BASE.to_owned(),
            ),
        ),
        Fire::new(ledger.clone()),
        Entropy::new(),
    )
}

pub(super) fn ingress(database: &Db, ledger: &afd_admission::Admissions) -> HarnessIngress {
    HarnessIngress::Unreachable(Box::new(afd_ingress::Ingress::new(
        database.clone(),
        vault(database.clone()),
        ledger.clone(),
    )))
}

pub(super) fn fleets(database: &Db, queue: &Dragonfly) -> Fleets {
    Fleets::new(database.clone(), queue.clone())
}

pub(super) fn dashboard() -> afd_connector::Dashboard {
    afd_connector::Dashboard::parse(FIXTURE_APP_URL).expect("fixture dashboard URL")
}
