//! The stores that open this deployment's own platform credentials in the
//! admin workspace: the connect flow, which also seals a tenant's handle, the
//! invite mailer, which reads the `smtp-relay` bag, and the scheduler client,
//! which reads the `qstash` bag.

use std::sync::Arc;

use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_db::Db;
use afd_mail::InviteMailer;
use afd_vault::Vault as SecretVault;

/// The connect flow, over this deployment's own vault and a tenant's.
///
/// Lifted out of the constructor because it is the largest thing there that
/// stands alone, and because the paragraph below wants somewhere to live that
/// is not the middle of a struct literal.
///
/// The SAME key every other sealing store takes, twice over and deliberately:
/// the platform half opens this deployment's own `<provider>-app` bags in the
/// admin workspace, and the grant half seals a tenant's handle in theirs. Two
/// `Vault` values over one table, for the reason the ingress beside them is
/// two — a reader of the deployment's credentials and a writer of a
/// workspace's are different surfaces, and one value serving both would let a
/// connector route reach the wrong workspace's secrets by holding the wrong
/// handle.
pub(super) fn connect_flow(
    database: &Db,
    kek: &Arc<Kek>,
    bus: &afd_bus::Handle,
    vendor_client: reqwest::Client,
) -> afd_connector::Connectors {
    afd_connector::Connectors::new(
        afd_connector::PlatformApp::new(SecretVault::new(
            database.clone(),
            Arc::clone(kek),
            Entropy::new(),
        )),
        afd_connector::Grants::new(
            SecretVault::new(database.clone(), Arc::clone(kek), Entropy::new())
                .with_activity_streams(bus.clone()),
            database.clone(),
            Entropy::new(),
        ),
        afd_connector::Exchange::new(vendor_client.clone()),
        vendor_client,
        database.clone(),
        Entropy::new(),
    )
}

/// The invite mailer, over its own reader of the admin workspace's vault.
///
/// Its own `Vault` value for the reason the connect flow's platform half is
/// one: it opens this deployment's credentials and nothing a tenant stored.
pub(super) fn invite_mailer(database: &Db, kek: &Arc<Kek>) -> InviteMailer {
    InviteMailer::new(SecretVault::new(
        database.clone(),
        Arc::clone(kek),
        Entropy::new(),
    ))
}

/// The scheduler this deployment ships, behind the [`afd_cron::Scheduler`]
/// seam, over its own reader of the admin workspace's vault.
///
/// Its own `Vault` value for the reason the invite mailer's is one: it opens
/// this deployment's `qstash` bag and nothing a tenant stored.
pub(super) fn qstash(
    database: &Db,
    kek: &Arc<Kek>,
    admin: Option<af_core::id::Uuid7>,
    schedule: super::ScheduleConfig,
) -> Arc<dyn afd_cron::Scheduler> {
    // Preflight refuses an api url that cannot form a callback, so this holds.
    // Should it ever not, the empty string matches no token's subject and every
    // fire is refused, rather than registering a truncated callback.
    let destination =
        afd_cron::qstash::destination_url(&schedule.api_url).unwrap_or_else(|refused| {
            let error_code = refused.code().as_str();
            tracing::error!(event = "scheduler_callback_unusable", error_code);
            String::new()
        });
    Arc::new(afd_cron::QStash::new(
        schedule.client,
        SecretVault::new(database.clone(), Arc::clone(kek), Entropy::new()),
        admin,
        destination,
    ))
}

/// The interim message poster: a Slack poster over its own grant store, posting
/// to the base the outbound worker posts answers to.
///
/// Its own grant store for the reason the worker has one: it opens a
/// workspace's bot token and nothing else, and sharing the connect flow's
/// would hand a sealing surface to a path that only ever reads.
pub(super) fn interjector(
    database: &Db,
    kek: &Arc<Kek>,
    vendor_client: reqwest::Client,
    slack_api_base: String,
) -> afd_outbound::Interjector {
    afd_outbound::Interjector::new(afd_outbound::SlackPoster::new(
        afd_connector::Grants::new(
            SecretVault::new(database.clone(), Arc::clone(kek), Entropy::new()),
            database.clone(),
            Entropy::new(),
        ),
        vendor_client,
        slack_api_base,
    ))
}
