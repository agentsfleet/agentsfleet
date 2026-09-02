//! The gallery's cursor and its rendering.
//!
//! Split out of [`super`] at the length cap. What is here is everything that
//! decides a value from values — the token a walk resumes from, and the cards a
//! page renders — none of which needs a pool. The two verbs above are the
//! router suite's, because what they add is a store and an ownership extractor.
//!
//! # The cursor tests are the interesting half
//!
//! A gallery token carries the WORKSPACE it was minted under, and the module
//! header calls that arm the one that matters: it is what stops a cursor minted
//! in one workspace from seeking inside another. That is a security boundary
//! rendered as a string comparison, so it is asserted directly rather than
//! through the page that happens to call it.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use afd_library::{
    GalleryPage, LibraryRequirements, Onboarded, PreparedBundle, Requirements, SummaryEntry,
};

use super::*;

/// The workspace every token below is minted under.
const WORKSPACE: &str = "019329c5-0000-7000-8000-0000000000b1";

/// A workspace that minted none of them.
const FOREIGN_WORKSPACE: &str = "019329c5-0000-7000-8000-0000000000b2";

/// The page size every token below is minted under.
const LIMIT: u32 = 25;

/// The bundle both fixtures below describe.
///
/// One set of names rather than two: the gallery card and the onboard response
/// render the SAME declared bundle through different functions, and a reader
/// comparing the two assertions has to be able to see that.
const BUNDLE_NAME: &str = "PR reviewer";

/// What that bundle says it does.
const BUNDLE_DESCRIPTION: &str = "Reviews pull requests";

/// The one credential it declares.
const DECLARED_CREDENTIAL: &str = "github";

/// The one tool it declares.
const DECLARED_TOOL: &str = "shell";

/// The one outbound host it declares.
const DECLARED_HOST: &str = "api.github.com";

/// A token for `workspace` at `limit`, minted the way the page mints one.
fn token(workspace: &str, limit: u32) -> String {
    struct_cursor::render(&Cursor {
        v: struct_cursor::VERSION,
        created_at: 1_760_000_000_000,
        tier_rank: Tier::Tenant.rank(),
        id: "bundle-1".to_owned(),
        workspace_uuid: workspace.to_owned(),
        limit,
    })
}

/// One catalogue entry, in `tier`.
fn entry(id: &str, tier: Tier) -> SummaryEntry {
    SummaryEntry {
        id: id.to_owned(),
        name: BUNDLE_NAME.to_owned(),
        description: BUNDLE_DESCRIPTION.to_owned(),
        tier,
        source_ref: "owner/repo@main".to_owned(),
        created_at_ms: 1_760_000_000_000,
        requirements: LibraryRequirements::fixture(
            vec![DECLARED_CREDENTIAL.to_owned()],
            vec![DECLARED_TOOL.to_owned()],
            vec![DECLARED_HOST.to_owned()],
            true,
        ),
        required_credentials_reasons: serde_json::Value::Null,
    }
}

/// A request with no token starts at the beginning rather than being refused.
///
/// An empty `starting_after=` is the same fact: a client that always sends the
/// parameter must not be refused for leaving it blank.
#[test]
fn should_start_at_the_beginning_when_no_token_is_sent() {
    assert!(
        resume_from("", WORKSPACE, LIMIT)
            .expect("an absent cursor is not a refusal")
            .is_none()
    );
    assert!(
        resume_from("starting_after=", WORKSPACE, LIMIT)
            .expect("an empty cursor is not a refusal")
            .is_none()
    );
}

/// A token this page minted resumes at the boundary it encodes.
#[test]
fn should_resume_from_a_token_this_walk_issued() {
    let query = format!("starting_after={}", token(WORKSPACE, LIMIT));

    let position = resume_from(&query, WORKSPACE, LIMIT)
        .expect("a token for this walk is accepted")
        .expect("a present token is a boundary");

    assert_eq!(position.created_at_ms, 1_760_000_000_000);
    assert_eq!(position.tier, Tier::Tenant);
    assert_eq!(position.id, "bundle-1");
}

/// A token minted in ANOTHER workspace is refused, whatever it encodes.
///
/// This is the arm the module header calls the one that matters: without it a
/// caller who owns workspace B could carry a cursor minted in workspace A and
/// seek inside A's gallery, because everything else about the token is valid.
#[test]
fn should_refuse_a_token_minted_in_another_workspace() {
    let query = format!("starting_after={}", token(FOREIGN_WORKSPACE, LIMIT));

    resume_from(&query, WORKSPACE, LIMIT)
        .expect_err("a cursor bound to another workspace does not seek in this one");
}

/// A token minted at another page size is refused too.
///
/// Its own arm rather than a variant of the one above: the boundary is only a
/// valid resume point for the page size that produced it, so a token replayed
/// at a different limit would serve a window that overlaps or skips.
#[test]
fn should_refuse_a_token_minted_at_another_page_size() {
    let query = format!("starting_after={}", token(WORKSPACE, LIMIT + 1));

    resume_from(&query, WORKSPACE, LIMIT).expect_err("the page size is part of the binding");
}

/// A token this endpoint did not issue is refused rather than half-read.
#[test]
fn should_refuse_a_token_this_endpoint_never_issued() {
    for opaque in ["not-a-cursor", "s:cHJvZA:019abc", "%%%"] {
        let query = format!("starting_after={opaque}");
        resume_from(&query, WORKSPACE, LIMIT)
            .expect_err("an unissued token is refused, not decoded");
    }
}

/// A rank no tier spells is refused, not silently placed.
///
/// The same answer a corrupt token gets, because to a caller the repair is the
/// same — and a rank this build cannot name is not a boundary it can seek from.
#[test]
fn should_refuse_a_token_naming_a_tier_rank_this_build_cannot_place() {
    let unplaceable = struct_cursor::render(&Cursor {
        v: struct_cursor::VERSION,
        created_at: 1_760_000_000_000,
        tier_rank: i32::MAX,
        id: "bundle-1".to_owned(),
        workspace_uuid: WORKSPACE.to_owned(),
        limit: LIMIT,
    });
    let query = format!("starting_after={unplaceable}");

    resume_from(&query, WORKSPACE, LIMIT).expect_err("no tier carries that rank");
}

/// A card carries the tier's LABEL as its visibility, and borrows every
/// declared name onto the wire.
///
/// `visibility` is the field whose meaning differs from the admin surface —
/// here it is which library the entry came from, not who may see it.
#[test]
fn should_render_a_card_from_its_entry_and_its_tier() {
    let page = GalleryPage {
        items: vec![entry("bundle-1", Tier::Tenant)],
        next: None,
    };

    let response = rendered(&page, WORKSPACE, LIMIT);
    let card = response.items.first().expect("the page carries its card");

    assert_eq!(card.id, "bundle-1");
    assert_eq!(card.name, BUNDLE_NAME);
    assert_eq!(card.visibility, Tier::Tenant.label());
    assert_eq!(card.source_ref, "owner/repo@main");
    assert_eq!(card.requirements.credentials, vec![DECLARED_CREDENTIAL]);
    assert_eq!(card.requirements.tools, vec![DECLARED_TOOL]);
    assert_eq!(card.requirements.network_hosts, vec![DECLARED_HOST]);
    assert!(card.requirements.trigger_present);
    assert_eq!(response.total, None, "a keyset page never counts");
}

/// A platform entry and a workspace entry render as one merged page, each
/// keeping its own library's label.
#[test]
fn should_merge_both_libraries_into_one_page_each_keeping_its_label() {
    let page = GalleryPage {
        items: vec![
            entry("platform-1", Tier::Platform),
            entry("tenant-1", Tier::Tenant),
        ],
        next: None,
    };

    let response = rendered(&page, WORKSPACE, LIMIT);

    assert_eq!(response.items.len(), 2);
    assert_eq!(response.items[0].visibility, Tier::Platform.label());
    assert_eq!(response.items[1].visibility, Tier::Tenant.label());
    assert_ne!(
        Tier::Platform.label(),
        Tier::Tenant.label(),
        "the labels must differ, or the assertion above proves nothing"
    );
}

/// A last page issues no cursor; a page with more behind it issues one this
/// walk's own parser accepts.
///
/// The round trip is the claim: rendering and parsing are the two halves of one
/// contract, and a token the page issued that its own `resume_from` refused
/// would strand a client mid-walk.
#[test]
fn should_issue_a_cursor_its_own_parser_accepts() {
    let last = GalleryPage {
        items: Vec::new(),
        next: None,
    };
    assert_eq!(rendered(&last, WORKSPACE, LIMIT).next_cursor, None);

    let more = GalleryPage {
        items: Vec::new(),
        next: Some(Position {
            created_at_ms: 1_760_000_000_001,
            tier: Tier::Platform,
            id: "bundle-9".to_owned(),
        }),
    };
    let issued = rendered(&more, WORKSPACE, LIMIT)
        .next_cursor
        .expect("a page with more behind it issues a cursor");

    let query = format!("starting_after={issued}");
    let round_tripped = resume_from(&query, WORKSPACE, LIMIT)
        .expect("the page issues a token its own parser accepts")
        .expect("the token is a boundary");

    assert_eq!(round_tripped.created_at_ms, 1_760_000_000_001);
    assert_eq!(round_tripped.tier, Tier::Platform);
    assert_eq!(round_tripped.id, "bundle-9");
}

/// An onboarded bundle renders into the TENANT library, whatever it declared.
///
/// The tier is not read from anything — [`created`] spells `Tier::Tenant`
/// outright, because `onboard` passed `Destination::Workspace` and there is no
/// other place this verb can put a bundle. Asserting it matters because the
/// response type is the operator catalogue's, whose own onboard renders
/// `Tier::Platform` into the same field: a copy-paste between the two would
/// tell a workspace its private bundle is published deployment-wide, and every
/// other field would still look right.
#[test]
fn should_render_an_onboarded_bundle_into_the_tenant_library() {
    let response = created(onboarded());

    assert_eq!(response.visibility, Tier::Tenant.label());
    assert_ne!(
        Tier::Tenant.label(),
        Tier::Platform.label(),
        "the labels must differ, or the assertion above proves nothing"
    );
    assert_eq!(
        response.id, "019329c5-0000-7000-8000-0000000000c1",
        "the row's identifier is what now stands — on a re-onboard that is the \
         FIRST write's id, not this bundle's content hash"
    );
    assert_eq!(response.name, BUNDLE_NAME);
    assert_eq!(response.content_hash, "0f".repeat(32));
}

/// The declared names ride the response; the support-file paths do not.
///
/// [`Requirements`] carries a fifth list, and the wire shape has four fields.
/// The omission is the claim: a support manifest is the bundle's internal file
/// layout, and answering it would tell a caller what is inside a bundle they
/// onboarded rather than what it needs from them.
#[test]
fn should_answer_what_a_bundle_needs_without_its_file_layout() {
    let response = created(onboarded());

    assert_eq!(response.requirements.credentials, [DECLARED_CREDENTIAL]);
    assert_eq!(response.requirements.tools, [DECLARED_TOOL]);
    assert_eq!(response.requirements.network_hosts, [DECLARED_HOST]);
    assert!(response.requirements.trigger_present);

    let document = serde_json::to_value(&response).expect("the response serialises");
    let requirements = document
        .get("requirements")
        .expect("the response carries its requirements");
    assert!(
        requirements.get("support_files").is_none(),
        "the support manifest is not on this wire: {requirements}"
    );
}

/// One onboarded bundle, as `library_onboard::run` answers with.
fn onboarded() -> Onboarded {
    Onboarded {
        id: "019329c5-0000-7000-8000-0000000000c1".to_owned(),
        bundle: PreparedBundle {
            name: BUNDLE_NAME.to_owned(),
            description: BUNDLE_DESCRIPTION.to_owned(),
            content_hash: "0f".repeat(32),
            snapshot_key: "bundles/0f.tar.zst".to_owned(),
            support_manifest: Vec::new(),
            requirements: Requirements {
                credentials: vec![DECLARED_CREDENTIAL.to_owned()],
                tools: vec![DECLARED_TOOL.to_owned()],
                network_hosts: vec![DECLARED_HOST.to_owned()],
                support_files: vec!["reference/checklist.md".to_owned()],
                trigger_present: true,
            },
        },
    }
}
