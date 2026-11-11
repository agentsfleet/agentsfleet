//! Credential configuration for route fixtures.
use super::*;

impl Fleet {
    /// An instance holding the scheduler's signing keys.
    ///
    /// Blank by default, which is the fail-closed state a fire is refused in.
    pub(crate) fn with_schedule_keys(mut self, current: &str, next: &str) -> Self {
        self.scheduler = fixed_scheduler(Some(afd_cron::SigningKeys {
            current: af_crypto::secret::SecretString::new(current.to_owned()),
            next: af_crypto::secret::SecretString::new(next.to_owned()),
        }));
        self
    }

    /// An instance whose fire route verifies through `scheduler`, for a suite
    /// injecting a failure the fixture scheduler cannot produce.
    pub(crate) fn with_scheduler(
        mut self,
        scheduler: std::sync::Arc<dyn afd_cron::Scheduler>,
    ) -> Self {
        self.scheduler = scheduler;
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
    pub(crate) fn with_recorded_analytics(mut self) -> (Self, af_observability::Recorded) {
        let (analytics, recorded) = Analytics::recording();
        self.analytics = analytics;
        (self, recorded)
    }

    /// Breaks call `ordinal` of one team-store write; the failpoint comes back
    /// so the suite can prove it fired.
    pub(crate) fn with_team_fault(
        mut self,
        step: TeamStep,
        ordinal: usize,
    ) -> (Self, Arc<Failpoint>) {
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
        self.identity_webhook_secret = Some(SecretBytes::new(secret.as_bytes().to_vec()));
        self
    }
}

/// The fixture scheduler, verifying against `keys` when a suite sets them.
///
/// No keys is the fail-closed default: every fire is refused as unverified.
#[expect(
    clippy::disallowed_methods,
    reason = "the fixture scheduler client names a reserved .test host that never resolves; a proxy rule and a deadline are not what the router suites prove"
)]
pub(super) fn fixed_scheduler(
    keys: Option<afd_cron::SigningKeys>,
) -> std::sync::Arc<dyn afd_cron::Scheduler> {
    let fixed = afd_cron::QStash::fixed(
        reqwest::Client::new(),
        af_crypto::secret::SecretString::new(String::new()),
        SCHEDULE_DESTINATION.to_owned(),
        SCHEDULE_API_BASE.to_owned(),
    );
    std::sync::Arc::new(match keys {
        Some(keys) => fixed.with_keys(keys),
        None => fixed,
    })
}
