#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{InviteLetter, account_label, render_invite};
use crate::test_util::VALID_DAYS;

const LINK: &str = "https://app.agentsfleet.test/invites/0190f5a2-4b2d-7c11-8d5e-2a5f31d98210";

fn letter<'a>(inviter_name: &'a str, owner_name: &'a str) -> InviteLetter<'a> {
    InviteLetter {
        inviter_name,
        owner_name,
        invite_url: LINK,
        valid_days: VALID_DAYS,
    }
}

/// Dimension 2.5: both parts match the reviewed snapshots, so a template edit
/// shows up as a diff in review.
#[test]
fn test_invite_render_snapshots() {
    let rendered = render_invite(&letter("John", "John")).expect("the invite renders");
    insta::assert_snapshot!("invite_html", rendered.html);
    insta::assert_snapshot!("invite_text", rendered.text);
    // pin test: literal is the contract
    assert_eq!(
        rendered.subject,
        "You're invited to join John's account on agentsfleet"
    );
}

/// Dimension 2.4: a display name carrying markup renders as text in the HTML
/// part.
#[test]
fn test_invite_template_escapes_names() {
    let rendered =
        render_invite(&letter("<b>x</b>", "<script>y</script>")).expect("the invite renders");
    assert!(
        rendered.html.contains("&#60;b&#62;x&#60;/b&#62;")
            || rendered.html.contains("&lt;b&gt;x&lt;/b&gt;")
    );
    assert!(!rendered.html.contains("<b>x</b>"));
    assert!(!rendered.html.contains("<script>"));
}

/// The link and both names reach both parts.
#[test]
fn both_parts_carry_the_link_and_names() {
    let rendered = render_invite(&letter("Ada", "John")).expect("the invite renders");
    for part in [&rendered.html, &rendered.text] {
        assert!(part.contains(LINK));
        assert!(part.contains("Ada"));
    }
    // The HTML part escapes the label's apostrophe; the text part does not.
    assert!(rendered.text.contains(&account_label("John")));
    assert!(rendered.html.contains("John&#39;s account"));
}
