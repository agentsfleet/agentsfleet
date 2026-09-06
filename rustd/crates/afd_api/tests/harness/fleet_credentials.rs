//! Credential configuration for route fixtures.
use super::*;

impl Fleet {
    /// An instance holding the scheduler's signing keys.
    ///
    /// Absent by default, which is the fail-closed state a fire is refused in.
    pub(crate) fn with_schedule_keys(mut self, current: &str, next: &str) -> Self {
        self.schedule_keys = Some(afd_cron::SigningKeys {
            current: afd_crypto::secret::SecretString::new(current.to_owned()),
            next: afd_crypto::secret::SecretString::new(next.to_owned()),
        });
        self
    }

    /// An instance that configured a platform admin workspace.
    ///
    /// `None` is the default and it is a real deployment state rather than an
    /// unset fixture: an App signs every installation's deliveries with ONE
    /// secret belonging to the deployment, so a daemon that was given no admin
    /// workspace has nowhere to read it from and fails closed. Leaving the
    /// default alone is how a suite reaches that branch.
    /// Points this fixture's connect relay at `base`.
    ///
    /// Exists for the one case that needs an UNUSABLE base: `relay_uri` refuses
    /// a dashboard base that is not a URL, and answers it as unconfigured
    /// because a boot-time misconfiguration is what it is. Sending somebody to
    /// a page that cannot exist would be the alternative.
    pub(crate) fn with_dashboard_base(mut self, base: &str) -> Self {
        base.clone_into(&mut self.dashboard_base);
        self
    }

    pub(crate) fn with_platform_admin(mut self, workspace: Uuid7) -> Self {
        self.platform_admin = Some(workspace);
        self
    }

    /// Configures the secret a signup event is verified against.
    ///
    /// `None` is the default and it is a real deployment state rather than an
    /// unset fixture: a daemon given no secret refuses every delivery, because
    /// accepting an unverified one on a route that CREATES ACCOUNTS is worse
    /// than serving none. Leaving the default alone is how a suite reaches
    /// that branch.
    pub(crate) fn with_identity_secret(mut self, secret: &str) -> Self {
        self.identity_webhook_secret = Some(afd_crypto::secret::SecretBytes::new(
            secret.as_bytes().to_vec(),
        ));
        self
    }
}
