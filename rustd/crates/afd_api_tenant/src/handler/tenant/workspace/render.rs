//! The list and create replies, assembled from what the stores answered.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_core::paging::Cursor;
use afd_tenant::workspace::accounts::Accounts;
use afd_tenant::workspace::directory::{Created, WorkspacePage, WorkspaceRow};
use afd_wire::workspace::{
    CreatedWorkspaceResponse, WorkspaceAccount, WorkspaceSummary, WorkspacesResponse,
};

use crate::handler::Refusal;
use crate::request_id::RequestId;

/// A listed row whose account the caller does not hold: an internal fault.
const DETAIL_ACCOUNT_UNHELD: &str = "Workspace list could not be assembled";

/// One page across the caller's accounts, the caller's own account id, and the
/// cursor that continues it.
///
/// The cursor is emitted only when a row EXISTS beyond this page — `more` is
/// decided by over-fetching, not by the page being full — so a client never
/// spends a token on a page that comes back empty.
pub(super) fn page_response<'page>(
    page: &'page WorkspacePage,
    accounts: &'page Accounts,
) -> Result<WorkspacesResponse<'page>, Refusal> {
    let next_cursor = page.more.then(|| page.rows.last()).flatten().map(|last| {
        Cow::Owned(
            Cursor::Timestamp {
                at_ms: last.created_at_ms,
                id: last.id.clone(),
            }
            .to_string(),
        )
    });
    let items = page
        .rows
        .iter()
        .map(|row| summary(row, accounts))
        .collect::<Result<_, _>>()?;
    Ok(WorkspacesResponse {
        items,
        tenant_id: Cow::Borrowed(accounts.home.as_str()),
        // Never counted — `tenant_workspaces.zig` answers a literal null.
        total: None,
        next_cursor,
    })
}

/// One listed workspace as the wire shows it, with the held account its row
/// belongs to.
///
/// The page was asked only for held accounts, so a row outside them is this
/// daemon disagreeing with itself, answered as the internal fault it is rather
/// than dropped, which would shift every later cursor.
fn summary<'row>(
    row: &'row WorkspaceRow,
    accounts: &'row Accounts,
) -> Result<WorkspaceSummary<'row>, Refusal> {
    let account = accounts.get(&row.tenant_id).ok_or_else(|| {
        Refusal::coded(
            afd_core::error_code::INTERNAL_OPERATION_FAILED,
            DETAIL_ACCOUNT_UNHELD,
        )
    })?;
    Ok(WorkspaceSummary {
        id: Cow::Borrowed(&row.id),
        name: row.name.as_deref().map(Cow::Borrowed),
        created_at: row.created_at_ms,
        account: WorkspaceAccount {
            tenant_id: Cow::Borrowed(account.tenant.as_str()),
            owner_name: Cow::Borrowed(&account.owner_name),
        },
        role: Cow::Borrowed(account.role.wire()),
    })
}

/// The create reply, with the identifiers only this side knows.
pub(super) fn created_response<'created>(
    created: &'created Created,
    tenant: &'created Uuid7,
) -> CreatedWorkspaceResponse<'created> {
    CreatedWorkspaceResponse {
        workspace_id: Cow::Borrowed(created.id.as_str()),
        name: Cow::Borrowed(&created.name),
        request_id: Cow::Owned(RequestId::mint().into()),
        tenant_id: Cow::Borrowed(tenant.as_str()),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_core::id::Uuid7;
    use afd_tenant::workspace::access::Role;
    use afd_tenant::workspace::accounts::{Account, Accounts};
    use afd_tenant::workspace::directory::{WorkspacePage, WorkspaceRow};
    use axum::response::IntoResponse as _;
    use http::StatusCode;

    use super::page_response;

    const HOME: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";
    const STRANGER: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1022";
    const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1033";

    /// The page was asked only for held accounts, so a row from any other is
    /// the daemon disagreeing with itself. It answers as the internal fault it
    /// is; dropping the row would shift every later cursor.
    #[test]
    fn a_row_outside_the_held_accounts_is_an_internal_fault_not_a_dropped_row() {
        let home = Uuid7::parse(HOME).expect("the fixture identifier is UUIDv7");
        let accounts = Accounts {
            home: home.clone(),
            held: vec![Account {
                tenant: home,
                role: Role::Owner,
                owner_name: "John".to_owned(),
            }],
        };
        let page = WorkspacePage {
            rows: vec![WorkspaceRow {
                id: WORKSPACE.to_owned(),
                name: None,
                created_at_ms: 1,
                tenant_id: STRANGER.to_owned(),
            }],
            more: false,
        };
        let refused = page_response(&page, &accounts)
            .expect_err("a row from an unheld account refuses the page");
        assert_eq!(
            refused.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
