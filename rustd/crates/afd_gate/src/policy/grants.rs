//! The integrations a workspace has standing approval to mint against.
//!
//! # Why a type, and not a slice of names
//!
//! The assembly fails closed: a mintable credential whose integration has no
//! standing grant parks the lease rather than degrading to "not available".
//! That reading is only safe if the caller cannot reach it by accident, and a
//! bare `&[Box<str>]` spells two different facts the same way — `&[]` is both
//! "this workspace has granted nothing" and "nobody has read the grant rows
//! yet". A caller that forgot the read would get the safe behaviour without
//! ever learning it forgot, and the bug would surface as leases parking on
//! integrations an operator can see are granted.
//!
//! [`Grants`] has no zero-argument constructor. The empty set is spelled
//! [`Grants::none`], which is a sentence someone wrote on purpose, and every
//! other way to get one starts from rows that were actually read.

use std::collections::BTreeSet;

use afd_credential::secrets::{Declared, Mintable};

/// The integrations this workspace may mint without asking a human again.
///
/// A set rather than a list: the assembly asks membership once per declared
/// mintable credential, and the grant rows are a workspace-wide list that has
/// no reason to be ordered or to hold duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grants(BTreeSet<Box<str>>);

impl Grants {
    /// A workspace holding no standing approval for anything.
    ///
    /// Every mintable credential a fleet declares will park against this. That
    /// is the correct answer for a workspace that has granted nothing — and
    /// naming it is what keeps it from also being the answer for a caller that
    /// never looked.
    #[must_use]
    pub fn none() -> Self {
        Self(BTreeSet::new())
    }

    /// Whether `integration` carries a standing grant.
    #[must_use]
    pub fn holds(&self, integration: &str) -> bool {
        self.0.contains(integration)
    }
}

/// Built from whatever the grant read returned, in any string shape.
impl<S: Into<Box<str>>> FromIterator<S> for Grants {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        Self(iter.into_iter().map(Into::into).collect())
    }
}

/// The first mintable credential whose integration has no standing grant.
///
/// Answers with the credential it found rather than with an [`Assembled`]:
/// a search says what it found, and leaving the outcome to the one function
/// that owns it keeps the park from being constructible in two places.
pub(super) fn first_ungranted<'a>(
    declared: &'a Declared,
    granted: &Grants,
) -> Option<&'a Mintable> {
    declared
        .mintable()
        .iter()
        .find(|wanted| !granted.holds(&wanted.integration))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::panic,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::Grants;

    use crate::policy::build::{Assembled, Inputs, assemble};
    use crate::policy::fixture::{config, provider};
    use afd_credential::secrets::Declared;

    #[test]
    fn a_workspace_that_granted_nothing_holds_nothing() {
        let granted = Grants::none();

        assert!(!granted.holds("github"));
        assert!(!granted.holds(""));
    }

    #[test]
    fn a_grant_is_held_by_its_exact_name() {
        let granted: Grants = ["github", "zoho"].into_iter().collect();

        assert!(granted.holds("github"));
        assert!(granted.holds("zoho"));
        // Exact, not a prefix: a grant for `github` is not a grant for
        // `github-enterprise`, which is a different third party under a
        // different standing decision.
        assert!(!granted.holds("github-enterprise"));
        assert!(!granted.holds("gith"));
    }

    #[test]
    fn a_repeated_row_is_the_same_single_grant() {
        let granted: Grants = ["github", "github"].into_iter().collect();

        assert_eq!(granted, ["github"].into_iter().collect::<Grants>());
    }

    #[test]
    fn a_mintable_credential_nobody_granted_parks_and_names_both_halves() {
        // The headline behaviour of this module. It parks rather than dropping
        // the credential, because a run that quietly lost its GitHub token
        // fails at the tool call — mid-run, after the work is billed — with a
        // message about a missing placeholder rather than a missing grant.
        let config = config("");
        let resolved = provider(None);
        let declared = Declared::with_mintable([("gh", "github")]);

        let parked = assemble(
            Inputs {
                config: &config,
                provider: &resolved,
                declared: &declared,
                repair_branch: None,
            },
            &Grants::none(),
        )
        .expect("an ungranted mint is a park, not a refusal");

        match parked {
            Assembled::Ungranted {
                credential,
                integration,
            } => {
                // Both halves, because "a grant is missing" is unactionable
                // and "`github` for `gh`" is a button someone can press.
                assert_eq!(credential, "gh");
                assert_eq!(integration, "github");
            }
            Assembled::Ready(_) => panic!("an ungranted mint must not assemble"),
        }
    }

    #[test]
    fn a_granted_mintable_credential_reaches_the_wire() {
        let config = config("");
        let resolved = provider(None);
        let declared = Declared::with_mintable([("gh", "github")]);
        let granted: Grants = ["github"].into_iter().collect();

        let assembled = assemble(
            Inputs {
                config: &config,
                provider: &resolved,
                declared: &declared,
                repair_branch: None,
            },
            &granted,
        )
        .expect("a granted mint assembles");

        let Assembled::Ready(policy) = assembled else {
            panic!("a granted mint must not park")
        };
        let carried = policy
            .mintable
            .first()
            .expect("a granted mint reaches the lease");
        assert_eq!(policy.mintable.len(), 1);
        assert_eq!(carried.name, "gh");
        assert_eq!(carried.integration, "github");
        // The grant permits the mint; it never ships a value to stand in for
        // one. The token is still fetched at the tool boundary.
        assert!(policy.secrets_map.is_none());
    }

    #[test]
    fn one_granted_integration_does_not_carry_an_ungranted_sibling() {
        // A partial grant set is the realistic case, and the pass is a search
        // for the FIRST gap rather than a check that anything was granted.
        let config = config("");
        let resolved = provider(None);
        let declared = Declared::with_mintable([("gh", "github"), ("desk", "zoho")]);
        let granted: Grants = ["github"].into_iter().collect();

        let parked = assemble(
            Inputs {
                config: &config,
                provider: &resolved,
                declared: &declared,
                repair_branch: None,
            },
            &granted,
        )
        .expect("an ungranted mint is a park, not a refusal");

        match parked {
            Assembled::Ungranted { integration, .. } => assert_eq!(integration, "zoho"),
            Assembled::Ready(_) => panic!("the ungranted sibling must still park"),
        }
    }
}
