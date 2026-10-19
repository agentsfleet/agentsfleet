//! The reviewed subject survives private-draft deletion without retaining a transcript.

use crate::{draft::read::Locked, types::SourcePin};
use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Subject {
    Draft {
        id: Uuid7,
    },
    Library {
        library_id: String,
        revision_id: Uuid7,
    },
    /// Setup the daemon wrote in code, such as a chat channel's resident.
    /// No person authored or reviewed it, so it carries no creator.
    System,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Context {
    pub(crate) subject: Subject,
    pub(crate) workspace_id: Uuid7,
    pub(crate) tenant_id: Uuid7,
    /// The authoring person; absent exactly for [`Subject::System`].
    pub(crate) creator_id: Option<Uuid7>,
    pub(crate) instance_name: Option<String>,
    pub(crate) source_pin: Option<SourcePin>,
    // Grant progress is a live check and never part of authored identity.
    #[serde(skip)]
    pub(crate) linked_fleet_id: Option<Uuid7>,
}

impl From<&Locked> for Context {
    fn from(draft: &Locked) -> Self {
        Self {
            subject: Subject::Draft {
                id: draft.id.clone(),
            },
            workspace_id: draft.workspace_id.clone(),
            tenant_id: draft.tenant_id.clone(),
            creator_id: Some(draft.user_id.clone()),
            instance_name: draft.instance_name.clone(),
            source_pin: draft.source_pin.clone(),
            linked_fleet_id: draft.fleet_id.clone(),
        }
    }
}

impl Context {
    /// The daemon's own setup for `workspace`, named `name`.
    pub(crate) fn system(workspace_id: Uuid7, tenant_id: Uuid7, name: &str) -> Self {
        Self {
            subject: Subject::System,
            workspace_id,
            tenant_id,
            creator_id: None,
            instance_name: Some(name.to_owned()),
            source_pin: None,
            linked_fleet_id: None,
        }
    }

    pub(super) fn review_href(&self) -> String {
        match (&self.subject, &self.linked_fleet_id) {
            (Subject::Draft { id }, _) => format!("/w/{}/drafts/{id}", self.workspace_id),
            (Subject::Library { library_id, .. }, _) => {
                format!("/w/{}/library/{library_id}", self.workspace_id)
            }
            (Subject::System, Some(fleet)) => format!("/w/{}/fleets/{fleet}", self.workspace_id),
            (Subject::System, None) => format!("/w/{}/fleets", self.workspace_id),
        }
    }

    pub(super) fn target(&self) -> Option<&Uuid7> {
        match &self.source_pin {
            Some(SourcePin::Fleet { fleet_id, .. }) => Some(fleet_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used, reason = "fixture identifiers are well formed")]

    use super::{Context, Subject};
    use afd_core::id::Uuid7;

    fn id(last: u8) -> Uuid7 {
        Uuid7::parse(&format!("01900000-0000-7000-8000-0000000000{last:02x}")).expect("fixture id")
    }

    #[test]
    fn a_review_points_its_reviewer_at_the_page_that_owns_its_subject() {
        let (workspace, draft, revision, fleet) = (id(1), id(2), id(3), id(4));
        let mut system = Context::system(workspace.clone(), id(5), "resident");
        assert_eq!(system.review_href(), format!("/w/{workspace}/fleets"));
        system.linked_fleet_id = Some(fleet.clone());
        assert_eq!(
            system.review_href(),
            format!("/w/{workspace}/fleets/{fleet}"),
            "a saved system fleet is reviewed on its own page"
        );
        for (subject, href) in [
            (
                Subject::Draft { id: draft.clone() },
                format!("/w/{workspace}/drafts/{draft}"),
            ),
            (
                Subject::Library {
                    library_id: "checkout-watch".to_owned(),
                    revision_id: revision,
                },
                format!("/w/{workspace}/library/checkout-watch"),
            ),
        ] {
            let context = Context {
                subject,
                ..system.clone()
            };
            assert_eq!(context.review_href(), href);
        }
    }
}
