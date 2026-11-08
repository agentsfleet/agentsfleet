//! The access seams: which workspace the harness owns, how access is decided,
//! and the browser session a suite signs in as.

use afd_auth::scope::Scope;

use super::*;

impl Fleet {
    /// Authorizes a minted workspace rather than the datastore-free fixture id.
    pub(crate) fn with_owned_workspace(mut self, workspace: Uuid7) -> Self {
        self.workspaces = Ownership::Stub(OneWorkspace::owning(workspace));
        self
    }

    /// Decides access with the production resolver over this harness's pool.
    ///
    /// For the suites whose subject is the decision itself: memberships and
    /// roles read from Postgres, where the stub would only echo its own answer.
    pub(crate) fn with_live_ownership(mut self) -> Self {
        self.workspaces = Ownership::Live(self.workspace_directory.clone());
        self
    }

    /// Accepts an unmarked bearer as a browser dashboard session.
    ///
    /// Session tokens do not use the credential directory: their verified
    /// claims are the identity. Rebuilding the two plane registries here keeps
    /// the fixture on the production authentication path while replacing only
    /// the key-set verifier that would otherwise need network access.
    pub(crate) fn with_dashboard(self, subject: &str) -> Self {
        self.with_dashboard_holding(subject, ScopeSet::EMPTY)
    }

    /// A browser session for `subject` whose token claims `scopes`: the caller
    /// a membership suite needs, since only a session is resolved through the
    /// user row a membership hangs from.
    ///
    /// A session's capabilities come from its own `scopes` claim, which is
    /// what the provider's token template projects, so they are written into
    /// the claim here rather than into the capability source. `None` for an
    /// empty set, the shape of a token that carries no claim at all.
    pub(crate) fn with_dashboard_holding(mut self, subject: &str, scopes: ScopeSet) -> Self {
        use afd_auth::verifier::VerifiedClaims;

        let subject = Subject::new(subject).expect("the fixture subject is not blank");
        let claimed: Vec<&str> = scopes.iter().map(Scope::wire).collect();
        let claims = VerifiedClaims {
            subject: subject.clone(),
            tenant: Some(tenant()),
            workspace_scope: None,
            scope_claim: (!claimed.is_empty()).then(|| claimed.join(" ").into_boxed_str()),
        };
        self.capabilities = self.capabilities.with(&subject, scopes);
        self.authenticator = Planes::new(
            self.directory.clone(),
            self.capabilities.clone(),
            MockVerifier::accepting(claims),
        );
        self
    }

    /// A handle a long-lived stream test can revoke after the router opens.
    pub(crate) fn ownership(&self) -> OneWorkspace {
        self.workspaces
            .stub()
            .expect("a live-ownership harness decides from Postgres; change the rows, not a stub")
            .clone()
    }
}
