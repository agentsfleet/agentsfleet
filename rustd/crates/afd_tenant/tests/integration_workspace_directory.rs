//! The workspace directory across the accounts a person holds, against live
//! Postgres: one keyset walk over all of them, a chosen name an account
//! already uses, and a generated name that collides.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::ENTROPY_LEN as ID_ENTROPY_LEN;
use afd_crypto::entropy::Entropy;
use afd_tenant::workspace::Workspaces;
use afd_tenant::workspace::directory::After;
use afd_tenant::workspace::name::Chosen;
use afd_tenant::workspace::name::ENTROPY_LEN as NAME_ENTROPY_LEN;

use crate::access_lane::{Fixture, id, session};

/// A name both accounts give a workspace, so a name filter must keep them apart.
const SHARED_NAME: &str = "shared-across-accounts";

/// When the extra workspaces are made: after every signup's own.
const LATER: UnixMillis = UnixMillis::from_millis(4_102_444_800_000);

/// Bob walks every workspace of both accounts he holds one row at a time:
/// each appears once, in creation order, and a name both accounts use comes
/// back once per account.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_walk_keyset_across_held_accounts() {
    let fixture = Fixture::create().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    for tenant in [&fixture.john.tenant, &fixture.bob.tenant] {
        let name = Chosen::parse(SHARED_NAME)
            .expect("a plain name")
            .expect("a chosen name");
        workspaces
            .create(&id(tenant), Some(name), "fixture", LATER)
            .await
            .expect("the shared name lands in each account");
    }
    let bob = session(&fixture.bob.tenant, &fixture.bob.subject);
    let accounts = workspaces
        .accounts_of(bob.person().expect("a session is a person"))
        .await
        .expect("the account read answers");
    let tenants = accounts.tenants();
    let whole = workspaces
        .page(&tenants, None, None, 50)
        .await
        .expect("the whole list reads");

    let walked = walk_one_row_at_a_time(&workspaces, &tenants).await;
    let listed: Vec<String> = whole.rows.iter().map(|row| row.id.clone()).collect();
    assert_eq!(
        walked, listed,
        "one row at a time is the whole list, in order"
    );
    assert!(
        whole.rows.len() >= 4,
        "two signups and two shared: {listed:?}"
    );
    the_shared_name_comes_back_once_per_account(&fixture, &workspaces, &tenants).await;
    fixture.cleanup().await;
}

/// Every row the walk holds, one row per page, each page resumed from the
/// last one's boundary.
async fn walk_one_row_at_a_time(workspaces: &Workspaces, tenants: &[&str]) -> Vec<String> {
    let mut walked = Vec::new();
    let mut after: Option<After> = None;
    loop {
        let page = workspaces
            .page(tenants, None, after.as_ref(), 1)
            .await
            .expect("a page reads");
        let Some(row) = page.rows.last() else { break };
        after = Some(After {
            created_at_ms: row.created_at_ms,
            id: id(&row.id),
        });
        walked.extend(page.rows.iter().map(|row| row.id.clone()));
        if !page.more {
            break;
        }
    }
    walked
}

/// The name filter returns John's and Bob's same-named workspaces once each,
/// and a resumed named page reaches the other account's.
async fn the_shared_name_comes_back_once_per_account(
    fixture: &Fixture,
    workspaces: &Workspaces,
    tenants: &[&str],
) {
    let named = workspaces
        .page(tenants, Some(SHARED_NAME), None, 50)
        .await
        .expect("the name filter reads");
    let mut owners: Vec<&str> = named
        .rows
        .iter()
        .map(|row| row.tenant_id.as_str())
        .collect();
    owners.sort_unstable();
    let mut expected = vec![fixture.john.tenant.as_str(), fixture.bob.tenant.as_str()];
    expected.sort_unstable();
    assert_eq!(owners, expected, "the shared name once per account");

    let first = workspaces
        .page(tenants, Some(SHARED_NAME), None, 1)
        .await
        .expect("the first named page reads");
    let boundary = first.rows.last().expect("one named row");
    let after = After {
        created_at_ms: boundary.created_at_ms,
        id: id(&boundary.id),
    };
    let rest = workspaces
        .page(tenants, Some(SHARED_NAME), Some(&after), 1)
        .await
        .expect("the resumed named page reads");
    assert!(first.more && !rest.more, "two named rows, one per page");
    assert_ne!(
        rest.rows.first().map(|row| row.tenant_id.as_str()),
        Some(boundary.tenant_id.as_str()),
        "the resumed page is the other account's"
    );
}

/// A chosen name the account already uses is refused as taken: the unique
/// index decides, and the refusal is the caller's, not the datastore's.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_refuse_chosen_workspace_name_when_account_already_uses_it() {
    let fixture = Fixture::create().await;
    let workspaces = Workspaces::new(fixture.database.clone(), Entropy::new());
    let tenant = id(&fixture.john.tenant);
    let chosen = || {
        Chosen::parse(SHARED_NAME)
            .expect("a plain name")
            .expect("a chosen name")
    };
    workspaces
        .create(&tenant, Some(chosen()), "fixture", LATER)
        .await
        .expect("the first lands");
    let taken = workspaces
        .create(&tenant, Some(chosen()), "fixture", LATER)
        .await
        .expect_err("the name is taken");
    assert_eq!(taken.code(), error_code::WORKSPACE_NAME_EXISTS);
    fixture.cleanup().await;
}

/// A generated name another workspace already holds is drawn again rather
/// than refused, since the caller never chose it; three collisions in a row
/// are the datastore's refusal, not a name conflict the caller could fix.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_draw_again_when_a_generated_name_collides() {
    let fixture = Fixture::create().await;
    let (entropy, draws) = Entropy::new_mocked();
    let workspaces = Workspaces::new(fixture.database.clone(), entropy);
    let tenant = id(&fixture.john.tenant);
    let taken = [1_u8; NAME_ENTROPY_LEN];
    let fresh = [2_u8; NAME_ENTROPY_LEN];
    let now = afd_core::clock::now();
    let mut id_byte = 10_u8;
    let mut queue = |name: &[u8]| {
        id_byte += 1;
        draws.push_bytes(name);
        draws.push_bytes(&[id_byte; ID_ENTROPY_LEN]);
    };

    queue(&taken);
    let first = workspaces
        .create(&tenant, None, "fixture", now)
        .await
        .expect("the first generated name lands");
    queue(&taken);
    queue(&fresh);
    let second = workspaces
        .create(&tenant, None, "fixture", now)
        .await
        .expect("a collided name is drawn again");
    assert_ne!(second.name, first.name);

    for _attempt in 0..3 {
        queue(&taken);
    }
    let exhausted = workspaces
        .create(&tenant, None, "fixture", now)
        .await
        .expect_err("three collisions in a row");
    assert_eq!(exhausted.code(), error_code::INTERNAL_DB_QUERY);
    fixture.cleanup().await;
}
