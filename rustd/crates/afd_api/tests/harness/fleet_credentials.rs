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
    pub(crate) fn with_platform_admin(mut self, workspace: Uuid7) -> Self {
        self.platform_admin = Some(workspace);
        self
    }

    /// Bounds the invite email's send, for the case proving a stalled relay.
    pub(crate) fn with_mail_deadline(mut self, deadline: std::time::Duration) -> Self {
        self.invite_mail = self.invite_mail.with_deadline(deadline);
        self
    }

    /// Keeps every product event the routes report, for the suite to read.
    pub(crate) fn with_recorded_analytics(mut self) -> (Self, afd_observability::Recorded) {
        let (analytics, recorded) = afd_observability::Analytics::recording();
        self.analytics = analytics;
        (self, recorded)
    }

    /// Breaks call `ordinal` of one team-store write; the failpoint comes back
    /// so the suite can prove it fired.
    pub(crate) fn with_team_fault(
        mut self,
        step: super::TeamStep,
        ordinal: usize,
    ) -> (Self, std::sync::Arc<super::Failpoint>) {
        let (team, failpoint) = self.team.breaking(step, ordinal);
        self.team = team;
        (self, failpoint)
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
