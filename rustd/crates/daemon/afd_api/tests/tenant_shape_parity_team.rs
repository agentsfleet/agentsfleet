//! The team-account shapes, pinned field for field through
//! `tenant_shape_parity.rs`'s helpers: what the owner's invite and member
//! lists carry, and what the invitee's page reads.
#![cfg(feature = "test-util")]

use std::borrow::Cow;

use afd_api_wire::team::{
    AcceptedInviteResponse, InviteEmailResponse, InviteSummary, MemberSummary, WaitingInvite,
    WorkspaceMember,
};

use crate::tenant_shape_parity::{TEXT, WHEN, assert_shape};

/// An invite keeps `email_sent_at` on the wire as `null` until a relay takes
/// its email, so the members page can tell "never sent" from a missing key.
#[test]
fn an_invite_summary_keeps_its_unsent_instant_as_null() {
    assert_shape(
        &InviteSummary {
            id: Cow::Borrowed(TEXT),
            email: Cow::Borrowed(TEXT),
            role: Cow::Borrowed(TEXT),
            expires_at: WHEN,
            created_at: WHEN,
            link: Cow::Borrowed(TEXT),
            email_status: Cow::Borrowed(TEXT),
            email_sent_at: None,
        },
        "InviteSummary",
        &[
            "id",
            "email",
            "role",
            "expires_at",
            "created_at",
            "link",
            "email_status",
            "email_sent_at",
        ],
    );
}

/// The send-again answer, the waiting invite and the accept answer carry
/// exactly what the invitee's page reads.
#[test]
fn the_invitee_facing_shapes_carry_only_what_the_page_reads() {
    assert_shape(
        &InviteEmailResponse {
            email_status: Cow::Borrowed(TEXT),
        },
        "InviteEmailResponse",
        &["email_status"],
    );
    assert_shape(
        &WaitingInvite {
            id: Cow::Borrowed(TEXT),
            account: afd_api_wire::workspace::WorkspaceAccount {
                tenant_id: Cow::Borrowed(TEXT),
                owner_name: Cow::Borrowed(TEXT),
            },
            expires_at: WHEN,
        },
        "WaitingInvite",
        &["id", "account", "expires_at"],
    );
    assert_shape(
        &AcceptedInviteResponse {
            tenant_id: Cow::Borrowed(TEXT),
            workspace_ids: vec![Cow::Borrowed(TEXT)],
        },
        "AcceptedInviteResponse",
        &["tenant_id", "workspace_ids"],
    );
}

/// The owner's member list carries addresses; the list every member of a
/// workspace reads to name senders never does. A `email` key growing on the
/// second would show each teammate everyone's address.
#[test]
fn only_the_owners_member_list_carries_addresses() {
    assert_shape(
        &MemberSummary {
            user_id: Cow::Borrowed(TEXT),
            display_name: None,
            email: Cow::Borrowed(TEXT),
            role: Cow::Borrowed(TEXT),
            joined_at: WHEN,
        },
        "MemberSummary",
        &["user_id", "display_name", "email", "role", "joined_at"],
    );
    assert_shape(
        &WorkspaceMember {
            user_id: Cow::Borrowed(TEXT),
            display_name: None,
            role: Cow::Borrowed(TEXT),
            actor: Cow::Borrowed(TEXT),
        },
        "WorkspaceMember",
        &["user_id", "display_name", "role", "actor"],
    );
}
