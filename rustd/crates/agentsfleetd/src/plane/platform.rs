//! The stores that open this deployment's own platform credentials in the
//! admin workspace: the connect flow, which also seals a tenant's handle, and
//! the invite mailer, which reads the `smtp-relay` bag.

use std::sync::Arc;

use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_dragonfly::Dragonfly;
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
    queue: &Dragonfly,
    vendor_client: reqwest::Client,
) -> afd_connector::Connectors {
    afd_connector::Connectors::new(
        afd_connector::PlatformApp::new(SecretVault::new(
            database.clone(),
            Arc::clone(kek),
            Entropy::new(),
        )),
        afd_connector::Grants::new(
            SecretVault::new(database.clone(), Arc::clone(kek), Entropy::new()),
            database.clone(),
            Entropy::new(),
        ),
        afd_connector::Exchange::new(vendor_client.clone()),
        vendor_client,
        queue.clone(),
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
